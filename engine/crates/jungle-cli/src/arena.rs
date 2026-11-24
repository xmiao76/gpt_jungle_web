use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command as ProcessCommand, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use jungle_core::{GameStateDto, Outcome, Session, Side, WinReason};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::corpus::OpeningCorpus;
use crate::protocol::{
    ApplyMoveData, ChooseMoveData, CloseData, Command, ErrorCode, HelloData, LimitSpec,
    MoveCoordinates, NewGameData, PositionData, ProtocolError, Response,
};
use crate::stats::{
    BootstrapInterval, CandidateOutcome, DEFAULT_BOOTSTRAP_SAMPLES, DEFAULT_BOOTSTRAP_SEED,
    PairScore, PromotionAssessment, WinDrawLoss, assess_promotion, paired_bootstrap,
};

pub const BENCHMARK_PLY_CAP: u16 = 256;
pub const DEFAULT_PROTOCOL_TIMEOUT_MS: u64 = 5_000;
pub const DEFAULT_DEADLINE_GRACE_MS: u64 = 250;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCommandSpec {
    pub program: PathBuf,
    #[serde(default)]
    pub arguments: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaConfig {
    pub candidate: EngineCommandSpec,
    pub champion: EngineCommandSpec,
    pub corpus: CorpusSource,
    pub limits: LimitSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_pairs: Option<usize>,
    pub bootstrap_samples: u32,
    pub bootstrap_seed: u64,
    pub protocol_timeout_ms: u64,
    pub deadline_grace_ms: u64,
}

impl ArenaConfig {
    #[must_use]
    pub fn with_defaults(
        candidate: EngineCommandSpec,
        champion: EngineCommandSpec,
        corpus: CorpusSource,
        limits: LimitSpec,
    ) -> Self {
        Self {
            candidate,
            champion,
            corpus,
            limits,
            maximum_pairs: None,
            bootstrap_samples: DEFAULT_BOOTSTRAP_SAMPLES,
            bootstrap_seed: DEFAULT_BOOTSTRAP_SEED,
            protocol_timeout_ms: DEFAULT_PROTOCOL_TIMEOUT_MS,
            deadline_grace_ms: DEFAULT_DEADLINE_GRACE_MS,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CorpusSource {
    File { path: PathBuf },
    Smoke,
}

pub fn run_arena(config: &ArenaConfig) -> Result<ArenaSummary, ArenaError> {
    config
        .limits
        .to_search_limits()
        .map_err(|error| ArenaError::Configuration(error.message))?;
    if config.bootstrap_samples == 0 {
        return Err(ArenaError::Configuration(
            "bootstrapSamples must be greater than zero".to_owned(),
        ));
    }
    if config.bootstrap_samples > crate::stats::MAX_BOOTSTRAP_SAMPLES {
        return Err(ArenaError::Configuration(format!(
            "bootstrapSamples must not exceed {}",
            crate::stats::MAX_BOOTSTRAP_SAMPLES
        )));
    }
    let corpus = match &config.corpus {
        CorpusSource::File { path } => {
            OpeningCorpus::from_path(path).map_err(|error| ArenaError::Corpus(error.to_string()))?
        }
        CorpusSource::Smoke => OpeningCorpus::smoke(),
    };
    corpus
        .validate_positions(|state| {
            Session::from_game_state_dto(state.clone()).map(|session| session.outcome().is_over())
        })
        .map_err(|error| ArenaError::Corpus(error.to_string()))?;
    let openings = corpus.selected(config.maximum_pairs);
    if openings.is_empty() {
        return Err(ArenaError::Configuration(
            "maximumPairs selected no openings".to_owned(),
        ));
    }

    let mut candidate = SubprocessEngine::new(EngineRole::Candidate, config.candidate.clone());
    let mut champion = SubprocessEngine::new(EngineRole::Champion, config.champion.clone());
    let candidate_hello = candidate
        .ensure_started(config.protocol_timeout_ms)
        .map_err(|error| ArenaError::Process(format!("candidate: {}", error.message)))?
        .clone();
    let champion_hello = champion
        .ensure_started(config.protocol_timeout_ms)
        .map_err(|error| ArenaError::Process(format!("champion: {}", error.message)))?
        .clone();
    require_limit_capability(&candidate_hello, config.limits, EngineRole::Candidate)?;
    require_limit_capability(&champion_hello, config.limits, EngineRole::Champion)?;

    let mut records = Vec::with_capacity(openings.len() * 2);
    for (pair_index, opening) in openings.iter().enumerate() {
        records.push(play_game(
            opening.id.as_str(),
            &opening.state,
            Side::Blue,
            game_seed(config.bootstrap_seed, pair_index, Side::Blue),
            config,
            &mut candidate,
            &mut champion,
        )?);
        records.push(play_game(
            opening.id.as_str(),
            &opening.state,
            Side::Red,
            game_seed(config.bootstrap_seed, pair_index, Side::Red),
            config,
            &mut candidate,
            &mut champion,
        )?);
    }
    candidate.close(config.protocol_timeout_ms);
    champion.close(config.protocol_timeout_ms);

    ArenaSummary::from_records(
        corpus.name,
        format!(
            "{} {}",
            candidate_hello.engine_name, candidate_hello.engine_version
        ),
        format!(
            "{} {}",
            champion_hello.engine_name, champion_hello.engine_version
        ),
        config.limits,
        records,
        config.bootstrap_samples,
        config.bootstrap_seed,
    )
}

#[allow(clippy::too_many_arguments)]
fn play_game(
    opening_id: &str,
    state: &GameStateDto,
    candidate_side: Side,
    seed: u64,
    config: &ArenaConfig,
    candidate: &mut SubprocessEngine,
    champion: &mut SubprocessEngine,
) -> Result<GameRecord, ArenaError> {
    let mut referee = Session::from_game_state_dto(state.clone())
        .map_err(|error| ArenaError::Referee(error.to_string()))?;
    let mut candidate_search = SearchTotals::default();
    let mut champion_search = SearchTotals::default();

    if let Err(error) = start_and_verify(candidate, seed, state, config.protocol_timeout_ms) {
        candidate.abort();
        return Ok(forfeit_record(
            opening_id,
            candidate_side,
            0,
            EngineRole::Candidate,
            error,
            candidate_search,
            champion_search,
        ));
    }
    if let Err(error) = start_and_verify(champion, seed, state, config.protocol_timeout_ms) {
        champion.abort();
        return Ok(forfeit_record(
            opening_id,
            candidate_side,
            0,
            EngineRole::Champion,
            error,
            candidate_search,
            champion_search,
        ));
    }

    for ply in 0..BENCHMARK_PLY_CAP {
        if let Outcome::Draw { reason } = referee.outcome() {
            return Ok(GameRecord {
                opening_id: opening_id.to_owned(),
                candidate_side,
                candidate_outcome: CandidateOutcome::Draw,
                plies: ply,
                termination: GameTermination::RulesDraw { reason },
                candidate_search,
                champion_search,
            });
        }
        if let Outcome::Win { winner, reason } = referee.outcome() {
            return Ok(rules_record(
                opening_id,
                candidate_side,
                ply,
                winner,
                reason,
                candidate_search,
                champion_search,
            ));
        }

        let active_role = if referee.position().side_to_move() == candidate_side {
            EngineRole::Candidate
        } else {
            EngineRole::Champion
        };
        let active = match active_role {
            EngineRole::Candidate => &mut *candidate,
            EngineRole::Champion => &mut *champion,
        };
        let choice = match active.choose_move(
            config.limits,
            config.protocol_timeout_ms,
            config.deadline_grace_ms,
        ) {
            Ok(choice) => choice,
            Err(error) => {
                active.abort();
                return Ok(forfeit_record(
                    opening_id,
                    candidate_side,
                    ply,
                    active_role,
                    error,
                    candidate_search,
                    champion_search,
                ));
            }
        };
        match active_role {
            EngineRole::Candidate => candidate_search.record(&choice),
            EngineRole::Champion => champion_search.record(&choice),
        }

        if let Some(error) = validate_principal_variation(&referee, &choice) {
            active.abort();
            return Ok(forfeit_record(
                opening_id,
                candidate_side,
                ply,
                active_role,
                error,
                candidate_search,
                champion_search,
            ));
        }
        if let Some(error) = enforce_reported_limit(config, &choice) {
            active.abort();
            return Ok(forfeit_record(
                opening_id,
                candidate_side,
                ply,
                active_role,
                error,
                candidate_search,
                champion_search,
            ));
        }
        let selected_move = choice.selected_move;
        if let Err(error) =
            referee.apply_coordinates(selected_move.origin, selected_move.destination)
        {
            active.abort();
            return Ok(forfeit_record(
                opening_id,
                candidate_side,
                ply,
                active_role,
                ProtocolError::new(ErrorCode::IllegalMove, error.to_string()),
                candidate_search,
                champion_search,
            ));
        }
        let canonical_state = referee.game_state_dto();

        if let Err(error) = apply_and_verify(
            candidate,
            selected_move,
            &canonical_state,
            config.protocol_timeout_ms,
        ) {
            candidate.abort();
            return Ok(forfeit_record(
                opening_id,
                candidate_side,
                ply + 1,
                EngineRole::Candidate,
                error,
                candidate_search,
                champion_search,
            ));
        }
        if let Err(error) = apply_and_verify(
            champion,
            selected_move,
            &canonical_state,
            config.protocol_timeout_ms,
        ) {
            champion.abort();
            return Ok(forfeit_record(
                opening_id,
                candidate_side,
                ply + 1,
                EngineRole::Champion,
                error,
                candidate_search,
                champion_search,
            ));
        }
    }

    if let Outcome::Win { winner, reason } = referee.outcome() {
        return Ok(rules_record(
            opening_id,
            candidate_side,
            BENCHMARK_PLY_CAP,
            winner,
            reason,
            candidate_search,
            champion_search,
        ));
    }
    Ok(GameRecord {
        opening_id: opening_id.to_owned(),
        candidate_side,
        candidate_outcome: CandidateOutcome::Draw,
        plies: BENCHMARK_PLY_CAP,
        termination: GameTermination::PlyCapDraw {
            cap: BENCHMARK_PLY_CAP,
        },
        candidate_search,
        champion_search,
    })
}

fn validate_principal_variation(
    referee: &Session,
    choice: &ChooseMoveData,
) -> Option<ProtocolError> {
    if choice
        .search
        .principal_variation
        .first()
        .is_some_and(|movement| *movement != choice.selected_move)
    {
        return Some(ProtocolError::new(
            ErrorCode::ProtocolViolation,
            "principal variation does not begin with selected move",
        ));
    }
    let mut replay = referee.clone();
    for (index, movement) in choice
        .search
        .principal_variation
        .iter()
        .copied()
        .enumerate()
    {
        if replay
            .apply_coordinates(movement.origin, movement.destination)
            .is_err()
        {
            return Some(ProtocolError::new(
                ErrorCode::ProtocolViolation,
                format!("principal variation is illegal at ply {}", index + 1),
            ));
        }
    }
    None
}

fn start_and_verify(
    engine: &mut SubprocessEngine,
    seed: u64,
    state: &GameStateDto,
    timeout_ms: u64,
) -> Result<(), ProtocolError> {
    let acknowledged = engine.start_game(seed, state, timeout_ms)?;
    if &acknowledged.state != state {
        return Err(ProtocolError::new(
            ErrorCode::ProtocolViolation,
            "setPosition acknowledgement differs from referee state",
        ));
    }
    Ok(())
}

fn apply_and_verify(
    engine: &mut SubprocessEngine,
    selected_move: MoveCoordinates,
    state: &GameStateDto,
    timeout_ms: u64,
) -> Result<(), ProtocolError> {
    let acknowledged = engine.apply_move(selected_move, timeout_ms)?;
    if acknowledged.applied_move != selected_move || &acknowledged.state != state {
        return Err(ProtocolError::new(
            ErrorCode::ProtocolViolation,
            "applyMove acknowledgement differs from referee state",
        ));
    }
    Ok(())
}

fn enforce_reported_limit(config: &ArenaConfig, choice: &ChooseMoveData) -> Option<ProtocolError> {
    match config.limits {
        LimitSpec::FixedNodes { nodes, .. } if choice.search.nodes > nodes => {
            Some(ProtocolError::new(
                ErrorCode::NodeLimitExceeded,
                format!(
                    "engine reported {} nodes for a {nodes}-node limit",
                    choice.search.nodes
                ),
            ))
        }
        LimitSpec::ExactTime { time_ms, .. }
            if choice.search.elapsed_ms
                > (time_ms as u64).saturating_add(config.deadline_grace_ms) as f64 =>
        {
            Some(ProtocolError::new(
                ErrorCode::DeadlineExceeded,
                format!(
                    "engine reported {:.3} ms for a {time_ms} ms limit with {} ms grace",
                    choice.search.elapsed_ms, config.deadline_grace_ms
                ),
            ))
        }
        _ => None,
    }
}

fn rules_record(
    opening_id: &str,
    candidate_side: Side,
    plies: u16,
    winner: Side,
    reason: WinReason,
    candidate_search: SearchTotals,
    champion_search: SearchTotals,
) -> GameRecord {
    GameRecord {
        opening_id: opening_id.to_owned(),
        candidate_side,
        candidate_outcome: if winner == candidate_side {
            CandidateOutcome::Win
        } else {
            CandidateOutcome::Loss
        },
        plies,
        termination: GameTermination::RulesWin { winner, reason },
        candidate_search,
        champion_search,
    }
}

fn forfeit_record(
    opening_id: &str,
    candidate_side: Side,
    plies: u16,
    engine: EngineRole,
    error: ProtocolError,
    candidate_search: SearchTotals,
    champion_search: SearchTotals,
) -> GameRecord {
    GameRecord {
        opening_id: opening_id.to_owned(),
        candidate_side,
        candidate_outcome: if engine == EngineRole::Candidate {
            CandidateOutcome::Loss
        } else {
            CandidateOutcome::Win
        },
        plies,
        termination: GameTermination::Forfeit {
            engine,
            code: error.code,
            message: error.message,
        },
        candidate_search,
        champion_search,
    }
}

impl SearchTotals {
    fn record(&mut self, choice: &ChooseMoveData) {
        self.moves += 1;
        self.nodes = self.nodes.saturating_add(choice.search.nodes);
        self.elapsed_ms += choice.search.elapsed_ms;
    }
}

fn require_limit_capability(
    hello: &HelloData,
    limits: LimitSpec,
    role: EngineRole,
) -> Result<(), ArenaError> {
    let supported = match limits {
        LimitSpec::FixedNodes { .. } => hello.capabilities.fixed_nodes,
        LimitSpec::ExactTime { .. } => hello.capabilities.exact_time,
    };
    if supported {
        Ok(())
    } else {
        Err(ArenaError::Configuration(format!(
            "{} does not support requested {:?} limits",
            role_label(role),
            limits
        )))
    }
}

fn game_seed(base: u64, pair_index: usize, candidate_side: Side) -> u64 {
    let side = match candidate_side {
        Side::Blue => 0x424c_5545,
        Side::Red => 0x5245_4421,
    };
    base ^ (pair_index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ side
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineRole {
    Candidate,
    Champion,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GameTermination {
    RulesDraw {
        reason: jungle_core::DrawReason,
    },
    RulesWin {
        winner: Side,
        reason: WinReason,
    },
    PlyCapDraw {
        cap: u16,
    },
    Forfeit {
        engine: EngineRole,
        code: ErrorCode,
        message: String,
    },
}

impl GameTermination {
    #[must_use]
    pub const fn is_strict_failure(&self) -> bool {
        matches!(self, Self::Forfeit { .. })
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchTotals {
    pub moves: u32,
    pub nodes: u64,
    pub elapsed_ms: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRecord {
    pub opening_id: String,
    pub candidate_side: Side,
    pub candidate_outcome: CandidateOutcome,
    pub plies: u16,
    pub termination: GameTermination,
    pub candidate_search: SearchTotals,
    pub champion_search: SearchTotals,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaSummary {
    pub schema: String,
    pub version: u32,
    pub corpus_schema: String,
    pub corpus_version: u32,
    pub corpus_name: String,
    pub candidate_engine: String,
    pub champion_engine: String,
    pub limits: LimitSpec,
    pub ply_cap: u16,
    pub pairs: u32,
    pub games: u32,
    pub overall: WinDrawLoss,
    pub candidate_as_blue: WinDrawLoss,
    pub candidate_as_red: WinDrawLoss,
    pub candidate_score: f64,
    pub candidate_blue_score: f64,
    pub candidate_red_score: f64,
    pub strict_failures: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paired_bootstrap: Option<BootstrapInterval>,
    pub promotion: PromotionAssessment,
    pub records: Vec<GameRecord>,
}

impl ArenaSummary {
    pub fn from_records(
        corpus_name: impl Into<String>,
        candidate_engine: impl Into<String>,
        champion_engine: impl Into<String>,
        limits: LimitSpec,
        records: Vec<GameRecord>,
        bootstrap_samples: u32,
        bootstrap_seed: u64,
    ) -> Result<Self, ArenaError> {
        if !records.len().is_multiple_of(2) {
            return Err(ArenaError::UnpairedRecords(records.len()));
        }

        let mut overall = WinDrawLoss::default();
        let mut blue = WinDrawLoss::default();
        let mut red = WinDrawLoss::default();
        let mut pair_scores = Vec::with_capacity(records.len() / 2);
        let mut strict_failures = 0_u32;

        for pair in records.chunks_exact(2) {
            if pair[0].opening_id != pair[1].opening_id {
                return Err(ArenaError::MismatchedPair {
                    first: pair[0].opening_id.clone(),
                    second: pair[1].opening_id.clone(),
                });
            }
            let blue_game = pair
                .iter()
                .find(|record| record.candidate_side == Side::Blue)
                .ok_or_else(|| ArenaError::MissingColor(pair[0].opening_id.clone()))?;
            let red_game = pair
                .iter()
                .find(|record| record.candidate_side == Side::Red)
                .ok_or_else(|| ArenaError::MissingColor(pair[0].opening_id.clone()))?;
            if std::ptr::eq(blue_game, red_game) {
                return Err(ArenaError::MissingColor(pair[0].opening_id.clone()));
            }

            blue.record(blue_game.candidate_outcome);
            red.record(red_game.candidate_outcome);
            overall.record(blue_game.candidate_outcome);
            overall.record(red_game.candidate_outcome);
            pair_scores.push(PairScore {
                candidate_as_blue: blue_game.candidate_outcome,
                candidate_as_red: red_game.candidate_outcome,
            });
            strict_failures += pair
                .iter()
                .filter(|record| record.termination.is_strict_failure())
                .count() as u32;
        }

        let interval = paired_bootstrap(&pair_scores, bootstrap_samples, bootstrap_seed);
        let promotion = assess_promotion(interval, blue, red, strict_failures);
        Ok(Self {
            schema: "jungle-arena-result".to_owned(),
            version: 1,
            corpus_schema: crate::corpus::CORPUS_SCHEMA.to_owned(),
            corpus_version: crate::corpus::CORPUS_VERSION,
            corpus_name: corpus_name.into(),
            candidate_engine: candidate_engine.into(),
            champion_engine: champion_engine.into(),
            limits,
            ply_cap: BENCHMARK_PLY_CAP,
            pairs: pair_scores.len() as u32,
            games: overall.games(),
            overall,
            candidate_as_blue: blue,
            candidate_as_red: red,
            candidate_score: overall.score(),
            candidate_blue_score: blue.score(),
            candidate_red_score: red.score(),
            strict_failures,
            paired_bootstrap: interval,
            promotion,
            records,
        })
    }
}

#[derive(Debug, Error)]
pub enum ArenaError {
    #[error("arena records must contain complete color-swapped pairs; got {0} records")]
    UnpairedRecords(usize),
    #[error("paired records refer to different openings: {first:?} and {second:?}")]
    MismatchedPair { first: String, second: String },
    #[error("opening pair {0:?} does not contain one game for each candidate color")]
    MissingColor(String),
    #[error("{0}")]
    Configuration(String),
    #[error("{0}")]
    Corpus(String),
    #[error("engine process failed: {0}")]
    Process(String),
    #[error("referee failed: {0}")]
    Referee(String),
}

enum ReaderEvent {
    Line(String),
    Io(String),
    Eof,
}

struct RunningEngine {
    child: Child,
    input: BufWriter<ChildStdin>,
    responses: Receiver<ReaderEvent>,
}

pub struct SubprocessEngine {
    role: EngineRole,
    spec: EngineCommandSpec,
    running: Option<RunningEngine>,
    next_id: u64,
    hello: Option<HelloData>,
}

impl SubprocessEngine {
    #[must_use]
    pub fn new(role: EngineRole, spec: EngineCommandSpec) -> Self {
        Self {
            role,
            spec,
            running: None,
            next_id: 1,
            hello: None,
        }
    }

    pub fn ensure_started(&mut self, timeout_ms: u64) -> Result<&HelloData, ProtocolError> {
        if self.running.is_none() {
            self.spawn()?;
            let id = self.take_id();
            let response = self.request(Command::Hello { id }, "hello", timeout_ms)?;
            let hello: HelloData = response.decode_data()?;
            if hello.protocol != crate::protocol::PROTOCOL_NAME
                || hello.protocol_version != crate::protocol::PROTOCOL_VERSION
            {
                self.invalidate();
                return Err(ProtocolError::new(
                    ErrorCode::ProtocolViolation,
                    format!(
                        "{} protocol is {} v{}, expected {} v{}",
                        role_label(self.role),
                        hello.protocol,
                        hello.protocol_version,
                        crate::protocol::PROTOCOL_NAME,
                        crate::protocol::PROTOCOL_VERSION
                    ),
                ));
            }
            if hello.ruleset_id != jungle_core::RULESET_ID
                || hello.rules_semantic_version != jungle_core::RULES_SEMANTIC_VERSION
            {
                self.invalidate();
                return Err(ProtocolError::new(
                    ErrorCode::ProtocolViolation,
                    format!(
                        "{} rules are {} v{}, expected {} v{}",
                        role_label(self.role),
                        hello.ruleset_id,
                        hello.rules_semantic_version,
                        jungle_core::RULESET_ID,
                        jungle_core::RULES_SEMANTIC_VERSION
                    ),
                ));
            }
            if !hello.capabilities.persistent_game_state
                || !hello.capabilities.structured_diagnostics
            {
                self.invalidate();
                return Err(ProtocolError::new(
                    ErrorCode::ProtocolViolation,
                    format!(
                        "{} lacks required arena capabilities",
                        role_label(self.role)
                    ),
                ));
            }
            self.hello = Some(hello);
        }
        self.hello.as_ref().ok_or_else(|| {
            ProtocolError::new(ErrorCode::Internal, "engine started without hello metadata")
        })
    }

    pub fn start_game(
        &mut self,
        seed: u64,
        state: &GameStateDto,
        timeout_ms: u64,
    ) -> Result<PositionData, ProtocolError> {
        self.ensure_started(timeout_ms)?;
        let id = self.take_id();
        let new_game = self.request(Command::NewGame { id, seed }, "newGame", timeout_ms)?;
        let new_game: NewGameData = new_game.decode_data()?;
        if new_game.seed != seed {
            return Err(ProtocolError::new(
                ErrorCode::ProtocolViolation,
                format!(
                    "{} echoed seed {}, expected {seed}",
                    role_label(self.role),
                    new_game.seed
                ),
            ));
        }
        let id = self.take_id();
        self.request(
            Command::SetPosition {
                id,
                state: state.clone(),
            },
            "setPosition",
            timeout_ms,
        )?
        .decode_data()
    }

    pub fn choose_move(
        &mut self,
        limits: LimitSpec,
        protocol_timeout_ms: u64,
        deadline_grace_ms: u64,
    ) -> Result<ChooseMoveData, ProtocolError> {
        self.ensure_started(protocol_timeout_ms)?;
        let timeout = limits
            .external_timeout_ms(deadline_grace_ms)
            .unwrap_or(protocol_timeout_ms);
        let id = self.take_id();
        self.request(Command::ChooseMove { id, limits }, "chooseMove", timeout)?
            .decode_data()
    }

    pub fn apply_move(
        &mut self,
        selected_move: MoveCoordinates,
        timeout_ms: u64,
    ) -> Result<ApplyMoveData, ProtocolError> {
        self.ensure_started(timeout_ms)?;
        let id = self.take_id();
        self.request(
            Command::ApplyMove { id, selected_move },
            "applyMove",
            timeout_ms,
        )?
        .decode_data()
    }

    pub fn close(&mut self, timeout_ms: u64) {
        if self.running.is_none() {
            return;
        }
        let id = self.take_id();
        let _ = self
            .request(Command::Close { id }, "close", timeout_ms)
            .and_then(|response| response.decode_data::<CloseData>());
        self.invalidate();
    }

    pub fn abort(&mut self) {
        self.invalidate();
    }

    fn spawn(&mut self) -> Result<(), ProtocolError> {
        let mut command = ProcessCommand::new(&self.spec.program);
        command
            .args(&self.spec.arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = command.spawn().map_err(|error| {
            ProtocolError::new(
                ErrorCode::EngineExited,
                format!(
                    "could not start {} executable {:?}: {error}",
                    role_label(self.role),
                    self.spec.program
                ),
            )
        })?;
        let input = child.stdin.take().ok_or_else(|| {
            ProtocolError::new(ErrorCode::EngineExited, "engine stdin was not piped")
        })?;
        let output = child.stdout.take().ok_or_else(|| {
            ProtocolError::new(ErrorCode::EngineExited, "engine stdout was not piped")
        })?;
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(output);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if sender.send(ReaderEvent::Line(line)).is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(ReaderEvent::Io(error.to_string()));
                        return;
                    }
                }
            }
            let _ = sender.send(ReaderEvent::Eof);
        });
        self.running = Some(RunningEngine {
            child,
            input: BufWriter::new(input),
            responses: receiver,
        });
        self.next_id = 1;
        Ok(())
    }

    fn request(
        &mut self,
        command: Command,
        expected_type: &str,
        timeout_ms: u64,
    ) -> Result<Response, ProtocolError> {
        let request_id = command.id();
        let running = self.running.as_mut().ok_or_else(|| {
            ProtocolError::new(ErrorCode::EngineExited, "engine process is not running")
        })?;
        serde_json::to_writer(&mut running.input, &command).map_err(|error| {
            ProtocolError::new(
                ErrorCode::ProtocolViolation,
                format!("could not serialize command: {error}"),
            )
        })?;
        running.input.write_all(b"\n").map_err(engine_write_error)?;
        running.input.flush().map_err(engine_write_error)?;

        let event = running
            .responses
            .recv_timeout(Duration::from_millis(timeout_ms.max(1)))
            .map_err(|error| {
                let code = match error {
                    mpsc::RecvTimeoutError::Timeout => ErrorCode::DeadlineExceeded,
                    mpsc::RecvTimeoutError::Disconnected => ErrorCode::EngineExited,
                };
                ProtocolError::new(
                    code,
                    format!(
                        "{} did not return request {request_id} within {timeout_ms} ms",
                        role_label(self.role)
                    ),
                )
            });
        let event = match event {
            Ok(value) => value,
            Err(error) => {
                self.invalidate();
                return Err(error);
            }
        };
        let line = match event {
            ReaderEvent::Line(line) => line,
            ReaderEvent::Io(message) => {
                self.invalidate();
                return Err(ProtocolError::new(
                    ErrorCode::EngineExited,
                    format!("{} stdout failed: {message}", role_label(self.role)),
                ));
            }
            ReaderEvent::Eof => {
                self.invalidate();
                return Err(ProtocolError::new(
                    ErrorCode::EngineExited,
                    format!("{} exited before responding", role_label(self.role)),
                ));
            }
        };
        let response: Response = match serde_json::from_str(&line) {
            Ok(response) => response,
            Err(error) => {
                self.invalidate();
                return Err(ProtocolError::new(
                    ErrorCode::ProtocolViolation,
                    format!(
                        "{} emitted non-protocol stdout: {error}",
                        role_label(self.role)
                    ),
                ));
            }
        };
        if response.id != Some(request_id) {
            self.invalidate();
            return Err(ProtocolError::new(
                ErrorCode::ProtocolViolation,
                format!(
                    "{} response id {:?} does not match request {request_id}",
                    role_label(self.role),
                    response.id
                ),
            ));
        }
        if response.ok && response.response_type != expected_type {
            self.invalidate();
            return Err(ProtocolError::new(
                ErrorCode::ProtocolViolation,
                format!(
                    "{} response type {:?} does not match {:?}",
                    role_label(self.role),
                    response.response_type,
                    expected_type
                ),
            ));
        }
        Ok(response)
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    fn invalidate(&mut self) {
        self.hello = None;
        if let Some(mut running) = self.running.take() {
            let _ = running.child.kill();
            let _ = running.child.wait();
        }
    }
}

impl Drop for SubprocessEngine {
    fn drop(&mut self) {
        self.invalidate();
    }
}

fn engine_write_error(error: std::io::Error) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::EngineExited,
        format!("could not write to engine: {error}"),
    )
}

const fn role_label(role: EngineRole) -> &'static str {
    match role {
        EngineRole::Candidate => "candidate",
        EngineRole::Champion => "champion",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::SearchReport;
    use jungle_search::{SearchDiagnostics, SearchTermination};

    fn record(opening: &str, side: Side, outcome: CandidateOutcome) -> GameRecord {
        GameRecord {
            opening_id: opening.to_owned(),
            candidate_side: side,
            candidate_outcome: outcome,
            plies: 1,
            termination: GameTermination::PlyCapDraw {
                cap: BENCHMARK_PLY_CAP,
            },
            candidate_search: SearchTotals::default(),
            champion_search: SearchTotals::default(),
        }
    }

    #[test]
    fn summary_keeps_opening_pairs_intact_for_bootstrap() {
        let records = vec![
            record("a", Side::Blue, CandidateOutcome::Win),
            record("a", Side::Red, CandidateOutcome::Win),
            record("b", Side::Blue, CandidateOutcome::Win),
            record("b", Side::Red, CandidateOutcome::Win),
        ];
        let summary = ArenaSummary::from_records(
            "test",
            "candidate",
            "champion",
            LimitSpec::FixedNodes {
                nodes: 1,
                max_depth: None,
            },
            records,
            100,
            1,
        )
        .expect("paired summary");
        assert_eq!(summary.overall.wins, 4);
        assert_eq!(summary.candidate_score, 1.0);
        assert_eq!(summary.paired_bootstrap.expect("interval").lower, 1.0);
        assert!(!summary.promotion.ready);
        assert!(
            summary
                .promotion
                .reasons
                .iter()
                .any(|reason| reason.contains("at least 200"))
        );
    }

    #[test]
    fn summary_rejects_duplicate_candidate_colors_in_a_pair() {
        let records = vec![
            record("a", Side::Blue, CandidateOutcome::Win),
            record("a", Side::Blue, CandidateOutcome::Loss),
        ];
        let error = ArenaSummary::from_records(
            "test",
            "candidate",
            "champion",
            LimitSpec::FixedNodes {
                nodes: 1,
                max_depth: None,
            },
            records,
            100,
            1,
        )
        .expect_err("missing Red game");
        assert!(matches!(error, ArenaError::MissingColor(_)));
    }

    fn test_config(limits: LimitSpec) -> ArenaConfig {
        let engine = EngineCommandSpec {
            program: PathBuf::from("engine"),
            arguments: Vec::new(),
        };
        ArenaConfig::with_defaults(engine.clone(), engine, CorpusSource::Smoke, limits)
    }

    fn choice(nodes: u64, elapsed_ms: f64) -> ChooseMoveData {
        ChooseMoveData {
            selected_move: MoveCoordinates {
                origin: 42,
                destination: 35,
            },
            search: SearchReport {
                score: 0,
                depth: 1,
                selective_depth: 1,
                nodes,
                elapsed_ms,
                nodes_per_second: 1,
                transposition_hits: 0,
                tablebase_hits: 0,
                principal_variation: Vec::new(),
                iterations: Vec::new(),
                termination: SearchTermination::NodeLimit,
                diagnostics: SearchDiagnostics::default(),
            },
        }
    }

    #[test]
    fn strict_reported_node_and_deadline_overruns_are_forfeits() {
        let fixed = test_config(LimitSpec::FixedNodes {
            nodes: 100,
            max_depth: None,
        });
        assert_eq!(
            enforce_reported_limit(&fixed, &choice(101, 0.0)).map(|error| error.code),
            Some(ErrorCode::NodeLimitExceeded)
        );

        let mut timed = test_config(LimitSpec::ExactTime {
            time_ms: 10,
            max_depth: None,
        });
        timed.deadline_grace_ms = 2;
        assert_eq!(
            enforce_reported_limit(&timed, &choice(1, 12.001)).map(|error| error.code),
            Some(ErrorCode::DeadlineExceeded)
        );
    }

    #[test]
    fn champion_forfeit_invalidates_promotion_instead_of_proving_strength() {
        let mut records = Vec::new();
        for index in 0..crate::stats::MINIMUM_PROMOTION_PAIRS {
            records.push(record(
                &format!("opening-{index}"),
                Side::Blue,
                CandidateOutcome::Win,
            ));
            records.push(record(
                &format!("opening-{index}"),
                Side::Red,
                CandidateOutcome::Win,
            ));
        }
        records[0].termination = GameTermination::Forfeit {
            engine: EngineRole::Champion,
            code: ErrorCode::EngineExited,
            message: "champion crashed".to_owned(),
        };
        let summary = ArenaSummary::from_records(
            "promotion",
            "candidate",
            "champion",
            LimitSpec::FixedNodes {
                nodes: 1,
                max_depth: None,
            },
            records,
            100,
            1,
        )
        .expect("summary");
        assert_eq!(summary.paired_bootstrap.expect("interval").lower, 1.0);
        assert_eq!(summary.strict_failures, 1);
        assert!(!summary.promotion.ready);
    }
}
