use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{BridgeError, ErrorPayload};

#[derive(Debug, Deserialize)]
pub(crate) struct CommandEnvelope {
    #[serde(rename = "type")]
    pub command_type: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum BridgeReply<T: Serialize> {
    Ok {
        ok: bool,
        revision: u64,
        data: T,
    },
    Error {
        ok: bool,
        revision: u64,
        error: ErrorPayload,
    },
}

impl<T: Serialize> BridgeReply<T> {
    pub(crate) fn ok(revision: u64, data: T) -> Self {
        Self::Ok {
            ok: true,
            revision,
            data,
        }
    }

    pub(crate) fn error(revision: u64, error: &BridgeError) -> Self {
        Self::Error {
            ok: false,
            revision,
            error: ErrorPayload::from(error),
        }
    }

    pub(crate) fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|serialization_error| {
            format!(
                "{{\"ok\":false,\"revision\":0,\"error\":{{\"code\":\"SERIALIZATION_ERROR\",\"message\":{:?}}}}}",
                serialization_error.to_string()
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Difficulty {
    Easy,
    #[default]
    Medium,
    Hard,
}

impl Difficulty {
    pub(crate) const fn default_time_ms(self) -> u32 {
        match self {
            Self::Easy => 250,
            Self::Medium => 750,
            Self::Hard => 2_000,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewGamePayload {
    pub difficulty: Difficulty,
    pub human_side: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LegalTargetsPayload {
    pub origin: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApplyMovePayload {
    pub origin: usize,
    pub destination: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiMovePayload {
    pub difficulty: Option<Difficulty>,
    pub time_limit_ms: Option<u32>,
    pub node_limit: Option<u64>,
    pub search_id: Option<u32>,
    pub base_revision: Option<u64>,
    pub position_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PliesPayload {
    #[serde(default = "one")]
    pub plies: usize,
}

const fn one() -> usize {
    1
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportGamePayload {
    pub game_state: Value,
    #[serde(default)]
    pub difficulty: Difficulty,
    #[serde(default = "default_human_side")]
    pub human_side: String,
}

fn default_human_side() -> String {
    "blue".to_owned()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) struct DetachedSearchPayload {
    pub game_state: Value,
    pub difficulty: Difficulty,
    pub time_limit_ms: Option<u32>,
    pub node_limit: Option<u64>,
    pub search_id: u32,
    pub base_revision: u64,
    pub position_hash: String,
    #[serde(default)]
    pub lane_id: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommitAiMovePayload {
    pub origin: usize,
    pub destination: usize,
    pub search_id: u32,
    pub base_revision: u64,
    pub position_hash: String,
    #[serde(default)]
    pub ai_stats: Option<Value>,
    #[serde(default)]
    pub stop_reason: Option<String>,
}

pub(crate) fn payload<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, BridgeError> {
    let value = if value.is_null() {
        Value::Object(serde_json::Map::new())
    } else {
        value
    };
    serde_json::from_value(value).map_err(|error| BridgeError::InvalidPayload(error.to_string()))
}
