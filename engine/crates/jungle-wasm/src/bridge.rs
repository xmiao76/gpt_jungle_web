use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use jungle_core::{BOARD_SIZE, ENGINE_VERSION, GameStateDto, Move, Outcome, Session, Side};
use jungle_search::{
    EndgameOracle, SEARCH_ENGINE_VERSION, SearchConfig, SearchEngine, SearchLimits, SearchResult,
    SearchTermination,
};
use serde_json::{Map, Value, json};
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(target_arch = "wasm32")]
use js_sys::Function;

use crate::control::BridgeSearchControl;
use crate::error::BridgeError;
use crate::metadata::{self, CapabilityMetadata, RuntimeFeatures};
#[cfg(any(target_arch = "wasm32", test))]
use crate::protocol::DetachedSearchPayload;
use crate::protocol::{
    AiMovePayload, ApplyMovePayload, BridgeReply, CommandEnvelope, CommitAiMovePayload, Difficulty,
    ImportGamePayload, LegalTargetsPayload, NewGamePayload, PliesPayload, payload,
};
use crate::tablebase::{self, DescriptorFormat, LoadedTablebase};

/// One authoritative game/session boundary exported by wasm-bindgen.
#[wasm_bindgen]
pub struct WasmGameSession {
    session: Session,
    search_engine: SearchEngine,
    tablebase: Option<LoadedTablebase>,
    difficulty: Difficulty,
    human_side: Side,
    revision: u64,
    ai_stats: Option<Value>,
    runtime_features: RuntimeFeatures,
    search_lane_id: u32,
    active_search_id: AtomicU32,
    cancelled_search_id: AtomicU32,
}

#[wasm_bindgen]
impl WasmGameSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::new_unloaded()
    }

    pub fn initial_snapshot_json(&self) -> String {
        self.snapshot_value().to_string()
    }

    /// Execute an authority command. Errors are always returned as structured
    /// JSON so malformed input cannot trap across the wasm-bindgen boundary.
    pub fn handle(&mut self, command_json: &str) -> String {
        match self.handle_inner(command_json) {
            Ok(data) => BridgeReply::ok(self.revision, data).to_json(),
            Err(error) => BridgeReply::<Value>::error(self.revision, &error).to_json(),
        }
    }

    /// Search a detached state without mutating this session. The browser runs
    /// this method in a disposable child Worker, keeping authority/undo state
    /// alive when a hard cancellation terminates the search Worker.
    #[cfg(target_arch = "wasm32")]
    pub fn search(&mut self, command_json: &str, progress_callback: Option<Function>) -> String {
        match self.search_inner(command_json, progress_callback.as_ref()) {
            Ok((revision, data)) => BridgeReply::ok(revision, data).to_json(),
            Err(error) => BridgeReply::<Value>::error(self.revision, &error).to_json(),
        }
    }

    /// Cooperatively cancel an active search. Without shared Wasm memory the
    /// call naturally takes effect between searches; Worker termination is the
    /// hard-cancel fallback.
    pub fn cancel_search(&self, search_id: Option<u32>) -> bool {
        let target = search_id.unwrap_or_else(|| self.active_search_id.load(Ordering::Acquire));
        if target == 0 {
            return false;
        }
        self.cancelled_search_id.store(target, Ordering::Release);
        self.active_search_id.load(Ordering::Acquire) == target
    }

    /// Address of the `u32` cancellation word in linear memory. Thread-enabled
    /// glue may write the active search id with `Atomics.store`.
    pub fn cancellation_flag_ptr(&self) -> usize {
        (&self.cancelled_search_id as *const AtomicU32).cast::<u32>() as usize
    }

    pub fn engine_metadata_json(&self) -> String {
        metadata::metadata_json()
    }

    pub fn capabilities_json(&self) -> String {
        metadata::capabilities_json(self.runtime_features)
    }

    /// Give an independent search Worker a stable Lazy-SMP lane. Lane zero is
    /// the baseline ordering; other lanes only diversify equal-priority move
    /// ordering inside jungle-search.
    pub fn configure_search_lane(&mut self, lane_id: u32) -> String {
        if self.active_search_id.load(Ordering::Acquire) != 0 {
            return json!({
                "ok": false,
                "error": {
                    "code": "SEARCH_BUSY",
                    "message": "The search lane can only be changed while search is idle.",
                }
            })
            .to_string();
        }
        self.search_engine.configure_lane(lane_id);
        self.search_lane_id = lane_id;
        json!({
            "ok": true,
            "laneId": lane_id,
            "diversified": lane_id != 0,
        })
        .to_string()
    }

    /// Record browser feature detection without assuming that compile-time Wasm
    /// target features are usable in the current execution environment.
    pub fn configure_runtime_features(&mut self, json: &str) -> String {
        match serde_json::from_str::<RuntimeFeatures>(json) {
            Ok(features) => {
                self.runtime_features = features;
                self.capabilities_json()
            }
            Err(error) => json!({
                "ok": false,
                "error": {
                    "code": "INVALID_CAPABILITIES",
                    "message": error.to_string(),
                }
            })
            .to_string(),
        }
    }

    /// Eagerly validate and install the manifest-declared mandatory two-piece
    /// tablebase or an additive sparse shard. The prior composite oracle
    /// remains active on any failure.
    pub fn load_tablebase(&mut self, bytes: &[u8], descriptor_json: &str) -> String {
        self.install_tablebase(bytes, descriptor_json, None)
    }

    /// Explicit optional-shard alias used by asynchronous runtime loaders.
    /// It rejects a legacy descriptor so accidentally replaying the mandatory
    /// asset cannot be reported as optional coverage.
    pub fn load_tablebase_shard(&mut self, bytes: &[u8], descriptor_json: &str) -> String {
        self.install_tablebase(bytes, descriptor_json, Some(DescriptorFormat::SparseShard))
    }
}

impl WasmGameSession {
    fn new_unloaded() -> Self {
        Self {
            session: Session::default(),
            search_engine: SearchEngine::new(SearchConfig::default()),
            tablebase: None,
            difficulty: Difficulty::Medium,
            human_side: Side::Blue,
            revision: 0,
            ai_stats: None,
            runtime_features: RuntimeFeatures::default(),
            search_lane_id: 0,
            active_search_id: AtomicU32::new(0),
            cancelled_search_id: AtomicU32::new(0),
        }
    }

    fn install_tablebase(
        &mut self,
        bytes: &[u8],
        descriptor_json: &str,
        required_format: Option<DescriptorFormat>,
    ) -> String {
        if self.active_search_id.load(Ordering::Acquire) != 0 {
            return self.tablebase_load_error(
                "TABLEBASE_BUSY",
                "Tablebase assets can only be installed while search is idle.",
            );
        }
        let format = match tablebase::descriptor_format(descriptor_json) {
            Ok(format) => format,
            Err(error) => {
                return self
                    .tablebase_load_error("TABLEBASE_VALIDATION_FAILED", &error.to_string());
            }
        };
        if required_format.is_some_and(|required| required != format) {
            return self.tablebase_load_error(
                "TABLEBASE_VALIDATION_FAILED",
                "load_tablebase_shard requires a sparse-shard-v1 descriptor.",
            );
        }

        match format {
            DescriptorFormat::Legacy => {
                let loaded = match LoadedTablebase::load_legacy(bytes, descriptor_json) {
                    Ok(loaded) => loaded,
                    Err(error) => {
                        return self.tablebase_load_error(
                            "TABLEBASE_VALIDATION_FAILED",
                            &error.to_string(),
                        );
                    }
                };
                let idempotent = self.tablebase.is_some();
                if self.tablebase.is_none() {
                    let oracle: Arc<dyn EndgameOracle> = Arc::new(loaded.clone());
                    self.search_engine.set_endgame_oracle(Some(oracle));
                    self.tablebase = Some(loaded);
                }
                let tablebase = self.tablebase.as_ref().expect("legacy tablebase installed");
                json!({
                    "ok": true,
                    "tablebaseLoaded": true,
                    "artifactId": "tablebase:two-piece-v1",
                    "format": "legacy-two-piece-v1",
                    "idempotent": idempotent,
                    "rulesFingerprint": tablebase.rules_fingerprint().to_hex(),
                    "sha256": tablebase.asset_sha256(),
                    "maxDistance": tablebase.max_distance(),
                    "bytes": bytes.len(),
                    "loadedShards": tablebase.shard_count(),
                    "loadedMaterials": tablebase.material_count(),
                    "mandatoryMaterialCount": tablebase::LEGACY_MATERIAL_COUNT,
                    "optionalMaterialCount": tablebase.material_count(),
                    "totalMaterialCount": tablebase::LEGACY_MATERIAL_COUNT + tablebase.material_count(),
                    "maxLoadedPieces": tablebase.max_loaded_pieces(),
                    "optionalResidentBytes": tablebase.optional_resident_bytes(),
                    "optionalResidentCapBytes": tablebase::OPTIONAL_RESIDENT_CAP_BYTES,
                })
                .to_string()
            }
            DescriptorFormat::SparseShard => {
                let Some(current) = self.tablebase.as_ref() else {
                    return self.tablebase_load_error(
                        "TABLEBASE_NOT_LOADED",
                        "Load the mandatory exact two-piece tablebase before optional shards.",
                    );
                };
                let (updated, report) = match current.with_sparse_shard(bytes, descriptor_json) {
                    Ok(result) => result,
                    Err(error) => {
                        return self.tablebase_load_error(
                            "TABLEBASE_VALIDATION_FAILED",
                            &error.to_string(),
                        );
                    }
                };
                if !report.idempotent {
                    let oracle: Arc<dyn EndgameOracle> = Arc::new(updated.clone());
                    self.search_engine.set_endgame_oracle(Some(oracle));
                    self.tablebase = Some(updated);
                }
                let tablebase = self
                    .tablebase
                    .as_ref()
                    .expect("mandatory tablebase retained");
                json!({
                    "ok": true,
                    "tablebaseLoaded": true,
                    "artifactId": report.artifact_id,
                    "materialId": report.material_id,
                    "format": "sparse-shard-v1",
                    "idempotent": report.idempotent,
                    "rulesFingerprint": tablebase.rules_fingerprint().to_hex(),
                    "maxDistance": report.max_distance,
                    "bytes": report.bytes,
                    "loadedShards": tablebase.shard_count(),
                    "loadedMaterials": tablebase.material_count(),
                    "mandatoryMaterialCount": tablebase::LEGACY_MATERIAL_COUNT,
                    "optionalMaterialCount": tablebase.material_count(),
                    "totalMaterialCount": tablebase::LEGACY_MATERIAL_COUNT + tablebase.material_count(),
                    "maxLoadedPieces": tablebase.max_loaded_pieces(),
                    "optionalResidentBytes": tablebase.optional_resident_bytes(),
                    "optionalResidentCapBytes": tablebase::OPTIONAL_RESIDENT_CAP_BYTES,
                })
                .to_string()
            }
        }
    }

    fn tablebase_load_error(&self, code: &str, message: &str) -> String {
        json!({
            "ok": false,
            "tablebaseLoaded": self.tablebase.is_some(),
            "error": {
                "code": code,
                "message": message,
            }
        })
        .to_string()
    }

    fn handle_inner(&mut self, command_json: &str) -> Result<Value, BridgeError> {
        let command: CommandEnvelope = serde_json::from_str(command_json)?;
        match command.command_type.as_str() {
            "newGame" => self.new_game(payload(command.payload)?),
            "legalTargets" => self.legal_targets(payload(command.payload)?),
            "applyMove" => self.apply_human_move(payload(command.payload)?),
            "aiMove" => self.apply_legacy_ai_move(payload(command.payload)?),
            "commitAiMove" => self.commit_ai_move(payload(command.payload)?),
            "cancelAi" => self.cancel_ai_command(command.payload),
            "undo" => self.undo(payload(command.payload)?),
            "redo" => self.redo(payload(command.payload)?),
            "importGame" => self.import_game(payload(command.payload)?),
            "exportGame" => Ok(self.export_game()),
            other => Err(BridgeError::UnknownCommand(other.to_owned())),
        }
    }

    fn new_game(&mut self, request: NewGamePayload) -> Result<Value, BridgeError> {
        let human_side = parse_side(&request.human_side)?;
        self.session = Session::default();
        self.search_engine.reset();
        self.difficulty = request.difficulty;
        self.human_side = human_side;
        self.ai_stats = None;
        self.revision = self.revision.saturating_add(1);
        Ok(self.snapshot_value())
    }

    fn legal_targets(&self, request: LegalTargetsPayload) -> Result<Value, BridgeError> {
        let origin = checked_index(request.origin, "origin")?;
        let position = self.session.position();
        let selected = position.piece_at(origin);
        let targets = position
            .moves_from(origin)
            .iter()
            .map(|movement| {
                json!({
                    "destination": movement.destination(),
                    "capture": movement.captured().is_some(),
                    "captured": movement.captured().map(|piece| piece.kind().label()),
                    "jump": movement.is_jump(),
                    "note": movement.note(),
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "origin": origin,
            "piece": selected.map(|piece| json!({
                "side": side_name(piece.side()),
                "kind": piece.kind(),
                "name": piece.kind().label(),
            })),
            "effectiveRank": selected.map_or(0, |_| position.effective_rank(origin)),
            "targets": targets,
        }))
    }

    fn apply_human_move(&mut self, request: ApplyMovePayload) -> Result<Value, BridgeError> {
        let origin = checked_index(request.origin, "origin")?;
        let destination = checked_index(request.destination, "destination")?;
        let position = self.session.position();
        let piece = position.piece_at(origin).ok_or_else(|| {
            BridgeError::IllegalMove("No piece exists on the selected origin square.".to_owned())
        })?;
        if position.side_to_move() != self.human_side || piece.side() != self.human_side {
            return Err(BridgeError::NotHumanTurn);
        }
        let movement = self
            .session
            .apply_coordinates(i32::from(origin), i32::from(destination))
            .map_err(|error| BridgeError::IllegalMove(error.to_string()))?;
        self.ai_stats = None;
        self.revision = self.revision.saturating_add(1);
        Ok(self.snapshot_with_move(movement))
    }

    fn apply_legacy_ai_move(&mut self, request: AiMovePayload) -> Result<Value, BridgeError> {
        self.require_tablebase()?;
        let search_id = request
            .search_id
            .unwrap_or_else(|| (self.revision as u32).wrapping_add(1).max(1));
        validate_search_id(search_id)?;
        if let Some(base_revision) = request.base_revision
            && base_revision != self.revision
        {
            return Err(BridgeError::StaleRevision {
                request: base_revision,
                current: self.revision,
            });
        }
        let position_hash = self.position_hash();
        if let Some(request_hash) = request.position_hash.as_deref() {
            validate_position_hash(request_hash)?;
            if request_hash != position_hash {
                return Err(BridgeError::PositionMismatch {
                    request: request_hash.to_owned(),
                    current: position_hash,
                });
            }
        }
        let difficulty = request.difficulty.unwrap_or(self.difficulty);
        let limits = search_limits(difficulty, request.time_limit_ms, request.node_limit);
        let result = self.run_authority_search(search_id, limits, &position_hash)?;
        let stats = ai_stats_value(
            difficulty,
            search_id,
            self.search_lane_id,
            self.revision,
            &position_hash,
            &result,
        );
        let Some(movement) = result.best_move else {
            if matches!(self.session.position().outcome(), Outcome::Ongoing) {
                return Err(BridgeError::AiNoMove);
            }
            self.ai_stats = Some(stats);
            return Ok(self.snapshot_value());
        };
        let applied = self
            .session
            .apply_move(movement)
            .map_err(|error| BridgeError::IllegalMove(error.to_string()))?;
        self.difficulty = difficulty;
        self.ai_stats = Some(stats);
        self.revision = self.revision.saturating_add(1);
        Ok(self.snapshot_with_move(applied))
    }

    fn commit_ai_move(&mut self, request: CommitAiMovePayload) -> Result<Value, BridgeError> {
        validate_search_id(request.search_id)?;
        validate_position_hash(&request.position_hash)?;
        if request.base_revision != self.revision {
            return Err(BridgeError::StaleRevision {
                request: request.base_revision,
                current: self.revision,
            });
        }
        let current_hash = self.position_hash();
        if request.position_hash != current_hash {
            return Err(BridgeError::PositionMismatch {
                request: request.position_hash,
                current: current_hash,
            });
        }
        let origin = checked_index(request.origin, "origin")?;
        let destination = checked_index(request.destination, "destination")?;
        let stats = normalize_committed_stats(
            request.ai_stats,
            request.stop_reason,
            request.search_id,
            request.base_revision,
            &request.position_hash,
        )?;
        let movement = self
            .session
            .apply_coordinates(i32::from(origin), i32::from(destination))
            .map_err(|error| BridgeError::IllegalMove(error.to_string()))?;
        self.ai_stats = Some(stats);
        self.revision = self.revision.saturating_add(1);
        Ok(self.snapshot_with_move(movement))
    }

    fn cancel_ai_command(&self, value: Value) -> Result<Value, BridgeError> {
        let search_id = value
            .get("searchId")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok());
        self.cancel_search(search_id);
        Ok(self.snapshot_value())
    }

    fn undo(&mut self, request: PliesPayload) -> Result<Value, BridgeError> {
        if !(1..=16).contains(&request.plies) {
            return Err(BridgeError::InvalidPayload(
                "Undo plies must be an integer from 1 to 16.".to_owned(),
            ));
        }
        let mut applied = 0;
        for _ in 0..request.plies {
            if !self.session.undo() {
                break;
            }
            applied += 1;
        }
        if applied > 0 {
            self.ai_stats = None;
            self.revision = self.revision.saturating_add(1);
        }
        let mut snapshot = self.snapshot_value();
        insert(&mut snapshot, "appliedPlies", json!(applied));
        Ok(snapshot)
    }

    fn redo(&mut self, request: PliesPayload) -> Result<Value, BridgeError> {
        if !(1..=16).contains(&request.plies) {
            return Err(BridgeError::InvalidPayload(
                "Redo plies must be an integer from 1 to 16.".to_owned(),
            ));
        }
        let mut applied = 0;
        for _ in 0..request.plies {
            if !self.session.redo() {
                break;
            }
            applied += 1;
        }
        if applied > 0 {
            self.ai_stats = None;
            self.revision = self.revision.saturating_add(1);
        }
        let mut snapshot = self.snapshot_value();
        insert(&mut snapshot, "appliedPlies", json!(applied));
        Ok(snapshot)
    }

    fn import_game(&mut self, request: ImportGamePayload) -> Result<Value, BridgeError> {
        let dto: GameStateDto = serde_json::from_value(request.game_state)
            .map_err(|error| BridgeError::InvalidSave(error.to_string()))?;
        let candidate = Session::from_game_state_dto(dto)
            .map_err(|error| BridgeError::InvalidSave(error.to_string()))?;
        let human_side = parse_side(&request.human_side)?;
        self.session = candidate;
        self.search_engine.reset();
        self.difficulty = request.difficulty;
        self.human_side = human_side;
        self.ai_stats = None;
        self.revision = self.revision.saturating_add(1);
        Ok(self.snapshot_value())
    }

    fn export_game(&self) -> Value {
        json!({
            "gameState": self.session.game_state_dto(),
            "difficulty": self.difficulty,
            "humanSide": side_name(self.human_side),
            "engineVersion": ENGINE_VERSION,
        })
    }

    #[cfg(target_arch = "wasm32")]
    fn search_inner(
        &mut self,
        command_json: &str,
        progress_callback: Option<&Function>,
    ) -> Result<(u64, Value), BridgeError> {
        self.require_tablebase()?;
        let request = parse_detached_search(command_json)?;
        validate_search_id(request.search_id)?;
        let lane_id = request.lane_id.unwrap_or(self.search_lane_id);
        if lane_id != self.search_lane_id {
            return Err(BridgeError::InvalidPayload(format!(
                "Search payload laneId {lane_id} does not match configured lane {}.",
                self.search_lane_id
            )));
        }
        validate_position_hash(&request.position_hash)?;
        let dto: GameStateDto = serde_json::from_value(request.game_state)
            .map_err(|error| BridgeError::InvalidSave(error.to_string()))?;
        let detached = Session::from_game_state_dto(dto)
            .map_err(|error| BridgeError::InvalidSave(error.to_string()))?;
        let actual_hash = format_hash(detached.position().hash());
        if request.position_hash != actual_hash {
            return Err(BridgeError::PositionMismatch {
                request: request.position_hash,
                current: actual_hash,
            });
        }
        let limits = search_limits(
            request.difficulty,
            request.time_limit_ms,
            request.node_limit,
        );
        self.begin_search(request.search_id);
        let control = BridgeSearchControl::new(
            &self.cancelled_search_id,
            request.search_id,
            lane_id,
            request.base_revision,
            &actual_hash,
            progress_callback,
        );
        let result = self
            .search_engine
            .choose_move(detached.position(), limits, &control)
            .map_err(|error| BridgeError::SearchFailed(error.to_string()));
        self.finish_search(request.search_id);
        let result = result?;
        let stats = ai_stats_value(
            request.difficulty,
            request.search_id,
            lane_id,
            request.base_revision,
            &actual_hash,
            &result,
        );
        let stop_reason = stop_reason(result.termination);
        Ok((
            request.base_revision,
            json!({
                "searchId": request.search_id,
                "laneId": lane_id,
                "baseRevision": request.base_revision,
                "positionHash": actual_hash,
                "move": result.best_move.map(move_value),
                "aiStats": stats,
                "stopReason": stop_reason,
            }),
        ))
    }

    #[cfg(all(not(target_arch = "wasm32"), test))]
    pub(crate) fn search_for_test(&mut self, command_json: &str) -> String {
        match self.search_inner_native(command_json) {
            Ok((revision, data)) => BridgeReply::ok(revision, data).to_json(),
            Err(error) => BridgeReply::<Value>::error(self.revision, &error).to_json(),
        }
    }

    #[cfg(all(not(target_arch = "wasm32"), test))]
    fn search_inner_native(&mut self, command_json: &str) -> Result<(u64, Value), BridgeError> {
        self.require_tablebase()?;
        let request = parse_detached_search(command_json)?;
        validate_search_id(request.search_id)?;
        let lane_id = request.lane_id.unwrap_or(self.search_lane_id);
        if lane_id != self.search_lane_id {
            return Err(BridgeError::InvalidPayload(format!(
                "Search payload laneId {lane_id} does not match configured lane {}.",
                self.search_lane_id
            )));
        }
        validate_position_hash(&request.position_hash)?;
        let dto: GameStateDto = serde_json::from_value(request.game_state)
            .map_err(|error| BridgeError::InvalidSave(error.to_string()))?;
        let detached = Session::from_game_state_dto(dto)
            .map_err(|error| BridgeError::InvalidSave(error.to_string()))?;
        let actual_hash = format_hash(detached.position().hash());
        if request.position_hash != actual_hash {
            return Err(BridgeError::PositionMismatch {
                request: request.position_hash,
                current: actual_hash,
            });
        }
        let limits = search_limits(
            request.difficulty,
            request.time_limit_ms,
            request.node_limit,
        );
        self.begin_search(request.search_id);
        let control = BridgeSearchControl::new(
            &self.cancelled_search_id,
            request.search_id,
            lane_id,
            request.base_revision,
            &actual_hash,
        );
        let result = self
            .search_engine
            .choose_move(detached.position(), limits, &control)
            .map_err(|error| BridgeError::SearchFailed(error.to_string()));
        self.finish_search(request.search_id);
        let result = result?;
        let stats = ai_stats_value(
            request.difficulty,
            request.search_id,
            lane_id,
            request.base_revision,
            &actual_hash,
            &result,
        );
        Ok((
            request.base_revision,
            json!({
                "searchId": request.search_id,
                "laneId": lane_id,
                "baseRevision": request.base_revision,
                "positionHash": actual_hash,
                "move": result.best_move.map(move_value),
                "aiStats": stats,
                "stopReason": stop_reason(result.termination),
            }),
        ))
    }

    fn run_authority_search(
        &mut self,
        search_id: u32,
        limits: SearchLimits,
        position_hash: &str,
    ) -> Result<SearchResult, BridgeError> {
        self.begin_search(search_id);
        #[cfg(target_arch = "wasm32")]
        let control = BridgeSearchControl::new(
            &self.cancelled_search_id,
            search_id,
            self.search_lane_id,
            self.revision,
            position_hash,
            None,
        );
        #[cfg(not(target_arch = "wasm32"))]
        let control = BridgeSearchControl::new(
            &self.cancelled_search_id,
            search_id,
            self.search_lane_id,
            self.revision,
            position_hash,
        );
        let result = self
            .search_engine
            .choose_move(self.session.position(), limits, &control)
            .map_err(|error| BridgeError::SearchFailed(error.to_string()));
        self.finish_search(search_id);
        result
    }

    fn begin_search(&self, search_id: u32) {
        self.active_search_id.store(search_id, Ordering::Release);
    }

    fn finish_search(&self, search_id: u32) {
        let _ = self.active_search_id.compare_exchange(
            search_id,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        if self.cancelled_search_id.load(Ordering::Acquire) == search_id {
            self.cancelled_search_id.store(0, Ordering::Release);
        }
    }

    fn snapshot_with_move(&self, movement: Move) -> Value {
        let mut snapshot = self.snapshot_value();
        insert(
            &mut snapshot,
            "appliedMove",
            history_entry(movement, self.session.history().len()),
        );
        snapshot
    }

    fn snapshot_value(&self) -> Value {
        let history = self
            .session
            .history()
            .iter()
            .enumerate()
            .map(|(index, movement)| history_entry(*movement, index + 1))
            .collect::<Vec<_>>();
        let last_move = history.last().cloned().unwrap_or(Value::Null);
        let capabilities = CapabilityMetadata::detect(self.runtime_features);
        json!({
            "revision": self.revision,
            "positionHash": self.position_hash(),
            "gameState": self.session.game_state_dto(),
            "history": history,
            "lastMove": last_move,
            "canUndo": self.session.can_undo(),
            "canRedo": self.session.can_redo(),
            "difficulty": self.difficulty,
            "humanSide": side_name(self.human_side),
            "aiStats": self.ai_stats,
            "runtime": {
                "engineName": metadata::ENGINE_NAME,
                "engineVersion": ENGINE_VERSION,
                "searchVersion": SEARCH_ENGINE_VERSION,
                "rulesVersion": jungle_core::RULESET_ID,
                "rulesFingerprint": self.tablebase
                    .as_ref()
                    .map(|tablebase| tablebase.rules_fingerprint().to_hex())
                    .unwrap_or_else(|| jungle_core::RULES_HASH_HEX.to_owned()),
                "tablebaseLoaded": self.tablebase.is_some(),
                "tablebaseMaxDistance": self.tablebase
                    .as_ref()
                    .map(LoadedTablebase::max_distance),
                "tablebase": {
                    "mandatoryLoaded": self.tablebase.is_some(),
                    "legacyAssetSha256": self.tablebase
                        .as_ref()
                        .map(LoadedTablebase::asset_sha256),
                    "shardCount": self.tablebase
                        .as_ref()
                        .map_or(0, LoadedTablebase::shard_count),
                    "materialCount": self.tablebase
                        .as_ref()
                        .map_or(0, LoadedTablebase::material_count),
                    "mandatoryMaterialCount": if self.tablebase.is_some() {
                        tablebase::LEGACY_MATERIAL_COUNT
                    } else {
                        0
                    },
                    "optionalMaterialCount": self.tablebase
                        .as_ref()
                        .map_or(0, LoadedTablebase::material_count),
                    "totalMaterialCount": self.tablebase
                        .as_ref()
                        .map_or(0, |tablebase| {
                            tablebase::LEGACY_MATERIAL_COUNT + tablebase.material_count()
                        }),
                    "maxLoadedPieces": self.tablebase
                        .as_ref()
                        .map_or(0, LoadedTablebase::max_loaded_pieces),
                    "loadedArtifactIds": self.tablebase
                        .as_ref()
                        .map(LoadedTablebase::loaded_artifact_ids)
                        .unwrap_or_default(),
                    "optionalResidentBytes": self.tablebase
                        .as_ref()
                        .map_or(0, LoadedTablebase::optional_resident_bytes),
                    "optionalResidentCapBytes": tablebase::OPTIONAL_RESIDENT_CAP_BYTES,
                },
                "capabilities": {
                    "wasm": cfg!(target_arch = "wasm32"),
                    "scalar": capabilities.scalar,
                    "simd": capabilities.simd_available,
                    "threads": capabilities.threads_available,
                    "cancellable": capabilities.cooperative_cancel,
                    "searchProgress": capabilities.progress_callback,
                    "sharedMemory": capabilities.shared_memory_available,
                    "lazySmp": capabilities.lazy_smp_available,
                    "workerPoolSize": capabilities.worker_pool_size,
                },
                "pythonVersion": "n/a",
                "hardAiVersion": SEARCH_ENGINE_VERSION,
                "searchLaneId": self.search_lane_id,
            },
        })
    }

    fn position_hash(&self) -> String {
        format_hash(self.session.position().hash())
    }

    fn require_tablebase(&self) -> Result<(), BridgeError> {
        if self.tablebase.is_some() {
            Ok(())
        } else {
            Err(BridgeError::TablebaseNotLoaded)
        }
    }
}

impl Default for WasmGameSession {
    fn default() -> Self {
        Self::new_unloaded()
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn parse_detached_search(command_json: &str) -> Result<DetachedSearchPayload, BridgeError> {
    let value: Value = serde_json::from_str(command_json)?;
    let payload_value = if value.get("type").and_then(Value::as_str) == Some("search") {
        value.get("payload").cloned().ok_or_else(|| {
            BridgeError::InvalidPayload("A detached search envelope requires payload.".to_owned())
        })?
    } else {
        value
    };
    payload(payload_value)
}

fn search_limits(
    difficulty: Difficulty,
    requested_time_ms: Option<u32>,
    requested_nodes: Option<u64>,
) -> SearchLimits {
    let time_ms = requested_time_ms
        .unwrap_or_else(|| difficulty.default_time_ms())
        .clamp(1, 60_000);
    let mut limits = SearchLimits::exact_time(time_ms);
    limits.node_limit = requested_nodes.map(|nodes| nodes.clamp(1, 1_000_000_000));
    limits
}

fn ai_stats_value(
    difficulty: Difficulty,
    search_id: u32,
    lane_id: u32,
    base_revision: u64,
    position_hash: &str,
    result: &SearchResult,
) -> Value {
    json!({
        "difficulty": difficulty,
        "searchId": search_id,
        "laneId": lane_id,
        "baseRevision": base_revision,
        "positionHash": position_hash,
        "depth": result.depth,
        "selectiveDepth": result.selective_depth,
        "nodes": result.nodes,
        "nodesPerSecond": result.nodes_per_second,
        "transpositionHits": result.transposition_hits,
        "tablebaseHits": result.tablebase_hits,
        "elapsedMs": result.elapsed_ms,
        "score": result.score,
        "stopReason": stop_reason(result.termination),
        "termination": result.termination,
        "principalVariation": result
            .principal_variation
            .iter()
            .copied()
            .map(move_value)
            .collect::<Vec<_>>(),
        "iterations": result.iterations,
        "diagnostics": result.diagnostics,
    })
}

fn normalize_committed_stats(
    stats: Option<Value>,
    requested_stop_reason: Option<String>,
    search_id: u32,
    base_revision: u64,
    position_hash: &str,
) -> Result<Value, BridgeError> {
    let mut stats = stats
        .ok_or_else(|| BridgeError::InvalidPayload("commitAiMove requires aiStats.".to_owned()))?;
    let object = stats.as_object_mut().ok_or_else(|| {
        BridgeError::InvalidPayload("commitAiMove aiStats must be an object.".to_owned())
    })?;
    for field in [
        "difficulty",
        "depth",
        "nodes",
        "nodesPerSecond",
        "transpositionHits",
        "tablebaseHits",
        "elapsedMs",
        "score",
        "principalVariation",
    ] {
        if !object.contains_key(field) {
            return Err(BridgeError::InvalidPayload(format!(
                "commitAiMove aiStats is missing {field}."
            )));
        }
    }
    let stop_reason_value = requested_stop_reason
        .or_else(|| {
            object
                .get("stopReason")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .ok_or_else(|| {
            BridgeError::InvalidPayload("commitAiMove requires stopReason.".to_owned())
        })?;
    if !matches!(
        stop_reason_value.as_str(),
        "completed" | "time" | "nodes" | "cancelled" | "terminal"
    ) {
        return Err(BridgeError::InvalidPayload(
            "commitAiMove stopReason is invalid.".to_owned(),
        ));
    }
    if !object
        .get("principalVariation")
        .is_some_and(Value::is_array)
    {
        return Err(BridgeError::InvalidPayload(
            "commitAiMove principalVariation must be an array.".to_owned(),
        ));
    }
    object.insert("searchId".to_owned(), json!(search_id));
    object.insert("baseRevision".to_owned(), json!(base_revision));
    object.insert("positionHash".to_owned(), json!(position_hash));
    object.insert("stopReason".to_owned(), json!(stop_reason_value));
    Ok(stats)
}

fn stop_reason(termination: SearchTermination) -> &'static str {
    match termination {
        SearchTermination::SoftDeadline | SearchTermination::HardDeadline => "time",
        SearchTermination::NodeLimit => "nodes",
        SearchTermination::Cancelled => "cancelled",
        SearchTermination::Terminal | SearchTermination::NoLegalMove => "terminal",
        SearchTermination::MaxDepth | SearchTermination::Proven => "completed",
    }
}

fn move_value(movement: Move) -> Value {
    json!({
        "origin": movement.origin(),
        "destination": movement.destination(),
        "piece": {
            "side": side_name(movement.piece().side()),
            "kind": movement.piece().kind(),
        },
        "captured": movement.captured().map(|piece| json!({
            "side": side_name(piece.side()),
            "kind": piece.kind(),
        })),
        "is_jump": movement.is_jump(),
        "note": movement.note(),
    })
}

fn history_entry(movement: Move, number: usize) -> Value {
    let mut object = match move_value(movement) {
        Value::Object(object) => object,
        _ => Map::new(),
    };
    object.insert("number".to_owned(), json!(number));
    object.insert(
        "pieceName".to_owned(),
        json!(movement.piece().kind().label()),
    );
    object.insert(
        "sideName".to_owned(),
        json!(movement.piece().side().title()),
    );
    object.insert(
        "capturedName".to_owned(),
        movement
            .captured()
            .map(|piece| json!(piece.kind().label()))
            .unwrap_or(Value::Null),
    );
    let capture = movement
        .captured()
        .map(|piece| format!(" x {}", piece.kind().label()))
        .unwrap_or_default();
    let jump = if movement.is_jump() { " (jump)" } else { "" };
    object.insert(
        "notation".to_owned(),
        json!(format!(
            "{} {} {}->{}{}{}",
            movement.piece().side().title(),
            movement.piece().kind().label(),
            movement.origin(),
            movement.destination(),
            capture,
            jump,
        )),
    );
    Value::Object(object)
}

fn parse_side(value: &str) -> Result<Side, BridgeError> {
    match value {
        "blue" => Ok(Side::Blue),
        "red" => Ok(Side::Red),
        _ => Err(BridgeError::InvalidSide),
    }
}

const fn side_name(side: Side) -> &'static str {
    match side {
        Side::Blue => "blue",
        Side::Red => "red",
    }
}

fn checked_index(value: usize, label: &str) -> Result<u8, BridgeError> {
    if value >= BOARD_SIZE {
        return Err(BridgeError::InvalidIndex(format!(
            "{label} must be an integer from 0 to 62."
        )));
    }
    Ok(value as u8)
}

fn validate_search_id(search_id: u32) -> Result<(), BridgeError> {
    if search_id == 0 {
        Err(BridgeError::InvalidPayload(
            "searchId must be a positive integer.".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn validate_position_hash(hash: &str) -> Result<(), BridgeError> {
    if hash.len() == 16
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(BridgeError::InvalidPayload(
            "positionHash must be a 16-character lowercase hexadecimal string.".to_owned(),
        ))
    }
}

fn format_hash(hash: u64) -> String {
    format!("{hash:016x}")
}

fn insert(target: &mut Value, key: &str, value: Value) {
    if let Some(object) = target.as_object_mut() {
        object.insert(key.to_owned(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(value: &str) -> Value {
        serde_json::from_str(value).expect("valid reply JSON")
    }

    fn load_required_tablebase(session: &mut WasmGameSession) -> Value {
        reply(&session.load_tablebase(
            crate::tablebase::TEST_ASSET_BYTES,
            crate::tablebase::TEST_DESCRIPTOR_JSON,
        ))
    }

    #[test]
    fn initial_snapshot_matches_browser_contract() {
        let session = WasmGameSession::new();
        let snapshot = reply(&session.initial_snapshot_json());
        assert_eq!(snapshot["revision"], 0);
        assert_eq!(
            snapshot["gameState"]["board"].as_array().map(Vec::len),
            Some(63)
        );
        assert_eq!(snapshot["gameState"]["side_to_move"], "blue");
        assert_eq!(snapshot["positionHash"].as_str().map(str::len), Some(16));
        assert_eq!(snapshot["runtime"]["engineName"], metadata::ENGINE_NAME);
        assert_eq!(snapshot["runtime"]["capabilities"]["scalar"], true);
        assert_eq!(snapshot["runtime"]["tablebaseLoaded"], false);
    }

    #[test]
    fn json_bridge_applies_legal_move_and_rejects_stale_commit() {
        let mut session = WasmGameSession::new();
        let legal = reply(&session.handle(r#"{"type":"legalTargets","payload":{"origin":48}}"#));
        assert_eq!(legal["ok"], true);
        let destination = legal["data"]["targets"][0]["destination"]
            .as_u64()
            .expect("initial rat has a move");
        let applied = reply(&session.handle(&format!(
            r#"{{"type":"applyMove","payload":{{"origin":48,"destination":{destination}}}}}"#
        )));
        assert_eq!(applied["ok"], true);
        assert_eq!(applied["revision"], 1);

        let stale = reply(&session.handle(
            r#"{"type":"commitAiMove","payload":{"origin":14,"destination":21,"searchId":7,"baseRevision":0,"positionHash":"0000000000000000"}}"#,
        ));
        assert_eq!(stale["ok"], false);
        assert_eq!(stale["error"]["code"], "STALE_REVISION");
    }

    #[test]
    fn malformed_commands_are_structured_errors() {
        let mut session = WasmGameSession::new();
        let invalid_json = reply(&session.handle("{"));
        assert_eq!(invalid_json["ok"], false);
        assert_eq!(invalid_json["error"]["code"], "INVALID_JSON");

        let unknown = reply(&session.handle(r#"{"type":"launchMissiles","payload":{}}"#));
        assert_eq!(unknown["error"]["code"], "UNKNOWN_COMMAND");
    }

    #[test]
    fn export_import_round_trip_preserves_save_v1_game_state() {
        let mut session = WasmGameSession::new();
        let exported = reply(&session.handle(r#"{"type":"exportGame","payload":{}}"#));
        let command = json!({
            "type": "importGame",
            "payload": {
                "gameState": exported["data"]["gameState"],
                "difficulty": "hard",
                "humanSide": "red",
            }
        });
        let imported = reply(&session.handle(&command.to_string()));
        assert_eq!(imported["ok"], true);
        assert_eq!(imported["data"]["difficulty"], "hard");
        assert_eq!(imported["data"]["humanSide"], "red");
        assert_eq!(imported["data"]["canUndo"], false);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn detached_search_is_non_mutating_and_identity_bound() {
        let mut session = WasmGameSession::new();
        assert_eq!(load_required_tablebase(&mut session)["ok"], true);
        let before = reply(&session.initial_snapshot_json());
        let command = json!({
            "gameState": before["gameState"],
            "difficulty": "hard",
            "timeLimitMs": 10_000,
            "nodeLimit": 100,
            "searchId": 41,
            "baseRevision": 0,
            "positionHash": before["positionHash"],
        });
        let result = reply(&session.search_for_test(&command.to_string()));
        assert_eq!(result["ok"], true);
        assert_eq!(result["revision"], 0);
        assert_eq!(result["data"]["searchId"], 41);
        assert!(result["data"]["move"].is_object());
        assert_eq!(reply(&session.initial_snapshot_json())["revision"], 0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn tablebase_loader_is_strict_atomic_and_idempotent() {
        let mut session = WasmGameSession::new();
        let initial = reply(&session.initial_snapshot_json());
        let blocked = reply(
            &session.search_for_test(
                &json!({
                    "gameState": initial["gameState"],
                    "difficulty": "hard",
                    "timeLimitMs": 100,
                    "nodeLimit": 10,
                    "searchId": 1,
                    "baseRevision": 0,
                    "positionHash": initial["positionHash"],
                })
                .to_string(),
            ),
        );
        assert_eq!(blocked["error"]["code"], "TABLEBASE_NOT_LOADED");

        let mut wrong_hash: Value =
            serde_json::from_str(crate::tablebase::TEST_DESCRIPTOR_JSON).unwrap();
        wrong_hash["sha256"] = json!("00".repeat(32));
        let rejected = reply(
            &session.load_tablebase(crate::tablebase::TEST_ASSET_BYTES, &wrong_hash.to_string()),
        );
        assert_eq!(rejected["ok"], false);
        assert_eq!(rejected["error"]["code"], "TABLEBASE_VALIDATION_FAILED");
        assert_eq!(
            reply(&session.initial_snapshot_json())["runtime"]["tablebaseLoaded"],
            false
        );

        let first = load_required_tablebase(&mut session);
        let second = load_required_tablebase(&mut session);
        assert_eq!(first["ok"], true);
        assert_eq!(second["ok"], true);
        assert_eq!(first["sha256"], second["sha256"]);

        let mut wrong_rules: Value =
            serde_json::from_str(crate::tablebase::TEST_DESCRIPTOR_JSON).unwrap();
        wrong_rules["rulesFingerprint"] = json!("00".repeat(32));
        let rejected = reply(
            &session.load_tablebase(crate::tablebase::TEST_ASSET_BYTES, &wrong_rules.to_string()),
        );
        assert_eq!(rejected["ok"], false);
        assert_eq!(rejected["tablebaseLoaded"], true);

        let mut corrupt = crate::tablebase::TEST_ASSET_BYTES.to_vec();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        let rejected =
            reply(&session.load_tablebase(&corrupt, crate::tablebase::TEST_DESCRIPTOR_JSON));
        assert_eq!(rejected["ok"], false);
        assert_eq!(rejected["tablebaseLoaded"], true);
    }

    #[test]
    fn optional_sparse_shards_load_additively_through_json_abi() {
        let mut session = WasmGameSession::new();
        assert_eq!(load_required_tablebase(&mut session)["ok"], true);
        let (_, bytes, descriptor) = crate::tablebase::test_sparse_asset();

        let loaded = reply(&session.load_tablebase_shard(&bytes, &descriptor));
        assert_eq!(loaded["ok"], true);
        assert_eq!(loaded["format"], "sparse-shard-v1");
        assert_eq!(loaded["loadedShards"], 1);
        assert_eq!(loaded["loadedMaterials"], 1);
        assert_eq!(loaded["maxLoadedPieces"], 3);
        assert_eq!(loaded["idempotent"], false);

        let duplicate = reply(&session.load_tablebase(&bytes, &descriptor));
        assert_eq!(duplicate["ok"], true);
        assert_eq!(duplicate["idempotent"], true);
        let snapshot = reply(&session.initial_snapshot_json());
        assert_eq!(snapshot["runtime"]["tablebase"]["shardCount"], 1);
        assert_eq!(snapshot["runtime"]["tablebase"]["maxLoadedPieces"], 3);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn loaded_oracle_produces_exact_two_piece_search_hit() {
        let mut session = WasmGameSession::new();
        assert_eq!(load_required_tablebase(&mut session)["ok"], true);

        let mut board = vec![Value::Null; BOARD_SIZE];
        // The canonical tablebase fixture is a blue rat with a mate-in-one
        // capture against a red elephant.
        board[14] = json!({ "side": "blue", "kind": "RAT" });
        board[15] = json!({ "side": "red", "kind": "ELEPHANT" });
        let imported = reply(
            &session.handle(
                &json!({
                    "type": "importGame",
                    "payload": {
                        "gameState": {
                            "board": board,
                            "side_to_move": "blue",
                            "move_history": [],
                            "winner": null,
                            "result": "ongoing",
                            "result_reason": "",
                        },
                        "difficulty": "hard",
                        "humanSide": "blue",
                    }
                })
                .to_string(),
            ),
        );
        assert_eq!(imported["ok"], true);
        let snapshot = &imported["data"];
        let searched = reply(
            &session.search_for_test(
                &json!({
                    "type": "search",
                    "payload": {
                        "gameState": snapshot["gameState"],
                        "difficulty": "hard",
                        "timeLimitMs": 10_000,
                        "nodeLimit": 10_000,
                        "searchId": 77,
                        "baseRevision": snapshot["revision"],
                        "positionHash": snapshot["positionHash"],
                    }
                })
                .to_string(),
            ),
        );
        assert_eq!(searched["ok"], true);
        assert!(
            searched["data"]["aiStats"]["tablebaseHits"]
                .as_u64()
                .is_some_and(|hits| hits > 0)
        );
    }
}
