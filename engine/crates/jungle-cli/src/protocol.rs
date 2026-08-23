use jungle_core::{GameStateDto, Move};
use jungle_search::{
    MAX_SEARCH_DEPTH, SearchDiagnostics, SearchLimits, SearchResult, SearchTermination,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_NAME: &str = "jungle-jsonl";
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Command {
    Hello {
        id: u64,
    },
    NewGame {
        id: u64,
        #[serde(default)]
        seed: u64,
    },
    SetPosition {
        id: u64,
        state: GameStateDto,
    },
    ChooseMove {
        id: u64,
        limits: LimitSpec,
    },
    ApplyMove {
        id: u64,
        #[serde(rename = "move")]
        selected_move: MoveCoordinates,
    },
    Close {
        id: u64,
    },
}

impl Command {
    #[must_use]
    pub const fn id(&self) -> u64 {
        match self {
            Self::Hello { id }
            | Self::NewGame { id, .. }
            | Self::SetPosition { id, .. }
            | Self::ChooseMove { id, .. }
            | Self::ApplyMove { id, .. }
            | Self::Close { id } => *id,
        }
    }

    #[must_use]
    pub const fn response_type(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "hello",
            Self::NewGame { .. } => "newGame",
            Self::SetPosition { .. } => "setPosition",
            Self::ChooseMove { .. } => "chooseMove",
            Self::ApplyMove { .. } => "applyMove",
            Self::Close { .. } => "close",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum LimitSpec {
    FixedNodes {
        nodes: u64,
        #[serde(rename = "maxDepth")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_depth: Option<u8>,
    },
    ExactTime {
        #[serde(rename = "timeMs")]
        time_ms: u32,
        #[serde(rename = "maxDepth")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_depth: Option<u8>,
    },
}

impl LimitSpec {
    pub fn to_search_limits(self) -> Result<SearchLimits, ProtocolError> {
        let (mut limits, max_depth) = match self {
            Self::FixedNodes { nodes: 0, .. } => {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidLimit,
                    "fixedNodes requires nodes greater than zero",
                ));
            }
            Self::FixedNodes { nodes, max_depth } => (SearchLimits::fixed_nodes(nodes), max_depth),
            Self::ExactTime { time_ms: 0, .. } => {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidLimit,
                    "exactTime requires timeMs greater than zero",
                ));
            }
            Self::ExactTime { time_ms, max_depth } => {
                (SearchLimits::exact_time(time_ms), max_depth)
            }
        };
        if let Some(depth) = max_depth {
            if depth == 0 || depth > MAX_SEARCH_DEPTH {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidLimit,
                    format!("maxDepth must be in 1..={MAX_SEARCH_DEPTH}"),
                ));
            }
            limits.max_depth = depth;
        }
        Ok(limits)
    }

    #[must_use]
    pub const fn external_timeout_ms(self, grace_ms: u64) -> Option<u64> {
        match self {
            Self::FixedNodes { .. } => None,
            Self::ExactTime { time_ms, .. } => Some((time_ms as u64).saturating_add(grace_ms)),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub fixed_nodes: bool,
    pub exact_time: bool,
    pub persistent_game_state: bool,
    pub structured_diagnostics: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloData {
    pub protocol: String,
    pub protocol_version: u32,
    pub engine_name: String,
    pub engine_version: String,
    pub ruleset_id: String,
    pub rules_semantic_version: u32,
    pub capabilities: Capabilities,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionData {
    pub state: GameStateDto,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewGameData {
    pub state: GameStateDto,
    pub seed: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveCoordinates {
    pub origin: i32,
    pub destination: i32,
}

impl From<Move> for MoveCoordinates {
    fn from(value: Move) -> Self {
        Self {
            origin: i32::from(value.origin()),
            destination: i32::from(value.destination()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchReport {
    pub score: i32,
    pub depth: u8,
    pub selective_depth: u8,
    pub nodes: u64,
    pub elapsed_ms: f64,
    pub nodes_per_second: u64,
    pub transposition_hits: u64,
    pub tablebase_hits: u64,
    pub principal_variation: Vec<MoveCoordinates>,
    pub iterations: Vec<SearchIterationReport>,
    pub termination: SearchTermination,
    pub diagnostics: SearchDiagnostics,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchIterationReport {
    pub depth: u8,
    pub selective_depth: u8,
    pub nodes: u64,
    pub elapsed_ms: f64,
    pub score: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_move: Option<MoveCoordinates>,
    pub principal_variation: Vec<MoveCoordinates>,
}

impl From<SearchResult> for SearchReport {
    fn from(value: SearchResult) -> Self {
        Self {
            score: value.score,
            depth: value.depth,
            selective_depth: value.selective_depth,
            nodes: value.nodes,
            elapsed_ms: value.elapsed_ms,
            nodes_per_second: value.nodes_per_second,
            transposition_hits: value.transposition_hits,
            tablebase_hits: value.tablebase_hits,
            principal_variation: value
                .principal_variation
                .into_iter()
                .map(MoveCoordinates::from)
                .collect(),
            iterations: value
                .iterations
                .into_iter()
                .map(|iteration| SearchIterationReport {
                    depth: iteration.depth,
                    selective_depth: iteration.selective_depth,
                    nodes: iteration.nodes,
                    elapsed_ms: iteration.elapsed_ms,
                    score: iteration.score,
                    best_move: iteration.best_move.map(MoveCoordinates::from),
                    principal_variation: iteration
                        .principal_variation
                        .into_iter()
                        .map(MoveCoordinates::from)
                        .collect(),
                })
                .collect(),
            termination: value.termination,
            diagnostics: value.diagnostics,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChooseMoveData {
    #[serde(rename = "move")]
    pub selected_move: MoveCoordinates,
    pub search: SearchReport,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyMoveData {
    pub applied_move: MoveCoordinates,
    pub state: GameStateDto,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CloseData {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidJson,
    InvalidCommand,
    InvalidLimit,
    NotInitialized,
    InvalidPosition,
    IllegalMove,
    GameOver,
    SearchFailed,
    DeadlineExceeded,
    NodeLimitExceeded,
    ProtocolViolation,
    EngineExited,
    Internal,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl ProtocolError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    #[must_use]
    pub fn with_details(mut self, details: impl Serialize) -> Self {
        self.details = serde_json::to_value(details).ok();
        self
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(rename = "type")]
    pub response_type: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProtocolError>,
}

impl Response {
    pub fn success(
        id: u64,
        response_type: impl Into<String>,
        data: impl Serialize,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            id: Some(id),
            response_type: response_type.into(),
            ok: true,
            data: Some(serde_json::to_value(data)?),
            error: None,
        })
    }

    #[must_use]
    pub fn failure(id: Option<u64>, error: ProtocolError) -> Self {
        Self {
            id,
            response_type: "error".to_owned(),
            ok: false,
            data: None,
            error: Some(error),
        }
    }

    pub fn decode_data<T: for<'de> Deserialize<'de>>(&self) -> Result<T, ProtocolError> {
        if !self.ok {
            return Err(self.error.clone().unwrap_or_else(|| {
                ProtocolError::new(ErrorCode::ProtocolViolation, "error response omitted error")
            }));
        }
        let value = self.data.clone().ok_or_else(|| {
            ProtocolError::new(
                ErrorCode::ProtocolViolation,
                "success response omitted data",
            )
        })?;
        serde_json::from_value(value).map_err(|error| {
            ProtocolError::new(
                ErrorCode::ProtocolViolation,
                format!("invalid response data: {error}"),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixed_node_and_exact_time_commands() {
        let fixed: Command = serde_json::from_str(
            r#"{"type":"chooseMove","id":7,"limits":{"kind":"fixedNodes","nodes":2000}}"#,
        )
        .expect("fixed node command");
        let timed: Command = serde_json::from_str(
            r#"{"type":"chooseMove","id":8,"limits":{"kind":"exactTime","timeMs":1800}}"#,
        )
        .expect("exact time command");
        assert_eq!(fixed.id(), 7);
        assert_eq!(timed.id(), 8);
    }

    #[test]
    fn rejects_zero_limits_and_out_of_range_depths() {
        assert_eq!(
            LimitSpec::FixedNodes {
                nodes: 0,
                max_depth: None,
            }
            .to_search_limits()
            .expect_err("zero nodes")
            .code,
            ErrorCode::InvalidLimit
        );
        assert_eq!(
            LimitSpec::ExactTime {
                time_ms: 1,
                max_depth: Some(0),
            }
            .to_search_limits()
            .expect_err("zero depth")
            .code,
            ErrorCode::InvalidLimit
        );
    }

    #[test]
    fn command_schema_rejects_unknown_fields() {
        let error = serde_json::from_str::<Command>(r#"{"type":"hello","id":1,"surprise":true}"#)
            .expect_err("unknown command field");
        assert!(error.to_string().contains("unknown field"));
    }
}
