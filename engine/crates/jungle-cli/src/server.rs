use std::io::{self, BufRead, Write};
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use jungle_core::{Outcome, RULES_SEMANTIC_VERSION, RULESET_ID, Session, SessionError};
use jungle_search::{
    EndgameOracle, NoopControl, OracleHit, OracleOutcome, SEARCH_ENGINE_VERSION, SearchConfig,
    SearchEngine, SearchTermination, SystemControl,
};
use jungle_tablebase::{LegacyTwoPieceTablebase, Wdl};

use crate::protocol::{
    ApplyMoveData, Capabilities, ChooseMoveData, CloseData, Command, ErrorCode, HelloData,
    LimitSpec, MoveCoordinates, NewGameData, PROTOCOL_NAME, PROTOCOL_VERSION, PositionData,
    ProtocolError, Response, SearchReport,
};

pub trait CommandBackend {
    fn execute(&mut self, command: Command) -> Result<Value, ProtocolError>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ServerSummary {
    pub commands: u64,
    pub errors: u64,
}

/// Serve the strict one-command/one-response JSONL protocol.
///
/// Parse and backend failures are protocol responses and do not terminate the
/// process. `close` is acknowledged before the loop exits. I/O failures are the
/// only errors returned to the caller.
pub fn serve_jsonl<R: BufRead, W: Write, B: CommandBackend>(
    reader: R,
    mut writer: W,
    backend: &mut B,
) -> io::Result<ServerSummary> {
    let mut summary = ServerSummary::default();
    for line in reader.lines() {
        let line = line?;
        summary.commands += 1;
        let parsed_value = serde_json::from_str::<Value>(&line);
        let parsed = parsed_value
            .as_ref()
            .ok()
            .cloned()
            .map(serde_json::from_value::<Command>);
        let (response, should_close) = match parsed {
            Some(Ok(command)) => {
                let id = command.id();
                let response_type = command.response_type();
                let should_close = matches!(command, Command::Close { .. });
                match backend.execute(command) {
                    Ok(data) => match Response::success(id, response_type, data) {
                        Ok(response) => (response, should_close),
                        Err(error) => {
                            summary.errors += 1;
                            (
                                Response::failure(
                                    Some(id),
                                    ProtocolError::new(
                                        ErrorCode::Internal,
                                        format!("could not serialize response: {error}"),
                                    ),
                                ),
                                false,
                            )
                        }
                    },
                    Err(error) => {
                        summary.errors += 1;
                        (Response::failure(Some(id), error), false)
                    }
                }
            }
            Some(Err(error)) => {
                summary.errors += 1;
                let id = extract_id(&line);
                (
                    Response::failure(
                        id,
                        ProtocolError::new(ErrorCode::InvalidCommand, error.to_string()),
                    ),
                    false,
                )
            }
            None => {
                summary.errors += 1;
                let message = parsed_value
                    .expect_err("failed JSON parse has an error")
                    .to_string();
                (
                    Response::failure(None, ProtocolError::new(ErrorCode::InvalidJson, message)),
                    false,
                )
            }
        };

        serde_json::to_writer(&mut writer, &response)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        if should_close && response.ok {
            break;
        }
    }
    Ok(summary)
}

fn extract_id(line: &str) -> Option<u64> {
    serde_json::from_str::<Value>(line)
        .ok()?
        .as_object()?
        .get("id")?
        .as_u64()
}

pub fn data(value: impl Serialize) -> Result<Value, ProtocolError> {
    serde_json::to_value(value).map_err(|error| {
        ProtocolError::new(
            ErrorCode::Internal,
            format!("could not serialize response data: {error}"),
        )
    })
}

pub struct NativeBackend {
    session: Option<Session>,
    search: SearchEngine,
}

#[derive(Debug)]
struct NativeTwoPieceOracle(LegacyTwoPieceTablebase);

impl NativeTwoPieceOracle {
    fn embedded() -> Self {
        const BYTES: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../python/jungle/ai/data/two_piece_v1.jgtb"
        ));
        Self(LegacyTwoPieceTablebase::from_bytes(BYTES).expect("embedded tablebase validates"))
    }
}

impl EndgameOracle for NativeTwoPieceOracle {
    fn probe(&self, position: &jungle_core::Position) -> Option<OracleHit> {
        self.0.probe_position(position).map(|hit| OracleHit {
            outcome: match hit.wdl {
                Wdl::Win => OracleOutcome::Win,
                Wdl::Draw => OracleOutcome::Draw,
                Wdl::Loss => OracleOutcome::Loss,
            },
            distance: hit.distance,
        })
    }
}

impl NativeBackend {
    #[must_use]
    pub fn new() -> Self {
        Self {
            session: None,
            search: SearchEngine::with_oracle(
                SearchConfig::hard(),
                Arc::new(NativeTwoPieceOracle::embedded()),
            ),
        }
    }

    fn session(&self) -> Result<&Session, ProtocolError> {
        self.session.as_ref().ok_or_else(|| {
            ProtocolError::new(
                ErrorCode::NotInitialized,
                "newGame or setPosition is required before this command",
            )
        })
    }

    fn session_mut(&mut self) -> Result<&mut Session, ProtocolError> {
        self.session.as_mut().ok_or_else(|| {
            ProtocolError::new(
                ErrorCode::NotInitialized,
                "newGame or setPosition is required before this command",
            )
        })
    }

    fn choose_move(&mut self, limits: LimitSpec) -> Result<ChooseMoveData, ProtocolError> {
        let search_limits = limits.to_search_limits()?;
        let session = self.session()?;
        if let Outcome::Win { winner, reason } = session.outcome() {
            return Err(
                ProtocolError::new(ErrorCode::GameOver, "game is already over")
                    .with_details(serde_json::json!({ "winner": winner, "reason": reason })),
            );
        }
        let position = session.position().clone();
        let result = match limits {
            LimitSpec::FixedNodes { .. } => {
                self.search
                    .choose_move(&position, search_limits, &NoopControl)
            }
            LimitSpec::ExactTime { .. } => {
                let control = SystemControl::new();
                self.search.choose_move(&position, search_limits, &control)
            }
        }
        .map_err(|error| {
            ProtocolError::new(ErrorCode::SearchFailed, format!("search failed: {error}"))
        })?;
        let selected = result.best_move.ok_or_else(|| {
            ProtocolError::new(ErrorCode::SearchFailed, "search returned no legal move")
        })?;
        if !position
            .legal_moves()
            .iter()
            .any(|movement| *movement == selected)
        {
            return Err(ProtocolError::new(
                ErrorCode::IllegalMove,
                "search returned a move rejected by the independent referee",
            )
            .with_details(MoveCoordinates::from(selected)));
        }
        if result
            .principal_variation
            .first()
            .is_some_and(|movement| *movement != selected)
        {
            return Err(ProtocolError::new(
                ErrorCode::SearchFailed,
                "principal variation does not begin with the selected move",
            ));
        }
        let mut replay = position.clone();
        for (index, movement) in result.principal_variation.iter().copied().enumerate() {
            if replay.try_make_move(movement).is_err() {
                return Err(ProtocolError::new(
                    ErrorCode::SearchFailed,
                    format!(
                        "principal variation contains an illegal move at ply {}",
                        index + 1
                    ),
                ));
            }
        }
        if let LimitSpec::FixedNodes { nodes, .. } = limits
            && result.nodes > nodes
        {
            return Err(ProtocolError::new(
                ErrorCode::NodeLimitExceeded,
                format!(
                    "search visited {} nodes for a {nodes}-node limit",
                    result.nodes
                ),
            ));
        }
        if let LimitSpec::ExactTime { time_ms, .. } = limits {
            const NATIVE_REPORTING_TOLERANCE_MS: f64 = 25.0;
            if result.elapsed_ms > f64::from(time_ms) + NATIVE_REPORTING_TOLERANCE_MS {
                return Err(ProtocolError::new(
                    ErrorCode::DeadlineExceeded,
                    format!(
                        "search exceeded its {time_ms} ms hard deadline: {:.3} ms",
                        result.elapsed_ms
                    ),
                ));
            }
        }
        debug_assert!(matches!(
            result.termination,
            SearchTermination::MaxDepth
                | SearchTermination::SoftDeadline
                | SearchTermination::HardDeadline
                | SearchTermination::NodeLimit
                | SearchTermination::Cancelled
                | SearchTermination::Proven
                | SearchTermination::Terminal
                | SearchTermination::NoLegalMove
        ));
        Ok(ChooseMoveData {
            selected_move: MoveCoordinates::from(selected),
            search: SearchReport::from(result),
        })
    }
}

impl Default for NativeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandBackend for NativeBackend {
    fn execute(&mut self, command: Command) -> Result<Value, ProtocolError> {
        match command {
            Command::Hello { .. } => data(HelloData {
                protocol: PROTOCOL_NAME.to_owned(),
                protocol_version: PROTOCOL_VERSION,
                engine_name: "jungle-search-native".to_owned(),
                engine_version: SEARCH_ENGINE_VERSION.to_owned(),
                ruleset_id: RULESET_ID.to_owned(),
                rules_semantic_version: RULES_SEMANTIC_VERSION,
                capabilities: Capabilities {
                    fixed_nodes: true,
                    exact_time: true,
                    persistent_game_state: true,
                    structured_diagnostics: true,
                },
            }),
            Command::NewGame { seed, .. } => {
                let session = Session::new();
                let state = session.game_state_dto();
                self.session = Some(session);
                self.search.reset();
                data(NewGameData { state, seed })
            }
            Command::SetPosition { state, .. } => {
                let session = Session::from_game_state_dto(state).map_err(|error| {
                    ProtocolError::new(ErrorCode::InvalidPosition, error.to_string())
                })?;
                let state = session.game_state_dto();
                self.session = Some(session);
                self.search.reset();
                data(PositionData { state })
            }
            Command::ChooseMove { limits, .. } => data(self.choose_move(limits)?),
            Command::ApplyMove { selected_move, .. } => {
                let applied = self
                    .session_mut()?
                    .apply_coordinates(selected_move.origin, selected_move.destination)
                    .map_err(map_session_error)?;
                let state = self.session()?.game_state_dto();
                data(ApplyMoveData {
                    applied_move: MoveCoordinates::from(applied),
                    state,
                })
            }
            Command::Close { .. } => data(CloseData::default()),
        }
    }
}

fn map_session_error(error: SessionError) -> ProtocolError {
    let code = match &error {
        SessionError::GameOver => ErrorCode::GameOver,
        SessionError::InvalidSquare(_) | SessionError::IllegalMove { .. } => ErrorCode::IllegalMove,
        SessionError::InvalidPosition(_)
        | SessionError::InvalidState(_)
        | SessionError::InvalidHistory { .. }
        | SessionError::InvalidSave(_)
        | SessionError::Json(_) => ErrorCode::InvalidPosition,
    };
    ProtocolError::new(code, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::CloseData;

    #[derive(Default)]
    struct EchoBackend;

    impl CommandBackend for EchoBackend {
        fn execute(&mut self, command: Command) -> Result<Value, ProtocolError> {
            match command {
                Command::Hello { .. } => data(serde_json::json!({ "ready": true })),
                Command::Close { .. } => data(CloseData::default()),
                _ => Err(ProtocolError::new(
                    ErrorCode::NotInitialized,
                    "test backend",
                )),
            }
        }
    }

    #[test]
    fn server_correlates_errors_and_acknowledges_close_before_stopping() {
        let input = concat!(
            "{\"type\":\"hello\",\"id\":1}\n",
            "{\"type\":\"unknown\",\"id\":2}\n",
            "{\"type\":\"close\",\"id\":3}\n",
            "{\"type\":\"hello\",\"id\":4}\n",
        );
        let mut output = Vec::new();
        let summary = serve_jsonl(input.as_bytes(), &mut output, &mut EchoBackend)
            .expect("in-memory protocol");
        let responses: Vec<Response> = String::from_utf8(output)
            .expect("utf8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("response"))
            .collect();
        assert_eq!(
            summary,
            ServerSummary {
                commands: 3,
                errors: 1
            }
        );
        assert_eq!(responses.len(), 3);
        assert!(responses[0].ok);
        assert_eq!(responses[1].id, Some(2));
        assert_eq!(
            responses[1].error.as_ref().map(|error| error.code),
            Some(ErrorCode::InvalidCommand)
        );
        assert!(responses[2].ok);
    }

    #[test]
    fn malformed_json_has_no_fabricated_request_id() {
        let mut output = Vec::new();
        serve_jsonl(b"not-json\n".as_slice(), &mut output, &mut EchoBackend)
            .expect("in-memory protocol");
        let response: Response = serde_json::from_slice(&output).expect("response");
        assert_eq!(response.id, None);
        assert_eq!(
            response.error.map(|error| error.code),
            Some(ErrorCode::InvalidJson)
        );
    }
}
