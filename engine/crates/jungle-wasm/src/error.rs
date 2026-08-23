use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum BridgeError {
    #[error("The command is not valid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("Unsupported command: {0}")]
    UnknownCommand(String),
    #[error("{0}")]
    InvalidPayload(String),
    #[error("{0}")]
    InvalidIndex(String),
    #[error("Side must be blue or red.")]
    InvalidSide,
    #[error("{0}")]
    InvalidSave(String),
    #[error("{0}")]
    IllegalMove(String),
    #[error("Only the configured human side can make a manual move.")]
    NotHumanTurn,
    #[error("The AI did not return a move for a non-terminal state.")]
    AiNoMove,
    #[error("Request revision {request} does not match current revision {current}.")]
    StaleRevision { request: u64, current: u64 },
    #[error("Position hash {request} does not match current position {current}.")]
    PositionMismatch { request: String, current: String },
    #[error("AI search failed: {0}")]
    SearchFailed(String),
    #[error("The mandatory exact two-piece tablebase has not been loaded.")]
    TablebaseNotLoaded,
}

impl BridgeError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::InvalidJson(_) => "INVALID_JSON",
            Self::UnknownCommand(_) => "UNKNOWN_COMMAND",
            Self::InvalidPayload(_) => "INVALID_PAYLOAD",
            Self::InvalidIndex(_) => "INVALID_INDEX",
            Self::InvalidSide => "INVALID_SIDE",
            Self::InvalidSave(_) => "INVALID_SAVE",
            Self::IllegalMove(_) => "ILLEGAL_MOVE",
            Self::NotHumanTurn => "NOT_HUMAN_TURN",
            Self::AiNoMove => "AI_NO_MOVE",
            Self::StaleRevision { .. } => "STALE_REVISION",
            Self::PositionMismatch { .. } => "POSITION_MISMATCH",
            Self::SearchFailed(_) => "SEARCH_FAILED",
            Self::TablebaseNotLoaded => "TABLEBASE_NOT_LOADED",
        }
    }

    pub(crate) fn details(&self) -> Option<Value> {
        match self {
            Self::StaleRevision { request, current } => Some(serde_json::json!({
                "requestRevision": request,
                "currentRevision": current,
            })),
            Self::PositionMismatch { request, current } => Some(serde_json::json!({
                "requestPositionHash": request,
                "currentPositionHash": current,
            })),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ErrorPayload {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    #[serde(skip_serializing_if = "is_false")]
    pub fatal: bool,
}

const fn is_false(value: &bool) -> bool {
    !*value
}

impl From<&BridgeError> for ErrorPayload {
    fn from(error: &BridgeError) -> Self {
        Self {
            code: error.code(),
            message: error.to_string(),
            details: error.details(),
            fatal: false,
        }
    }
}
