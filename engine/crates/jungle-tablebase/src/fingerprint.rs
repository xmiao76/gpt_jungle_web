use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{TablebaseError, hex_decode, hex_encode, sha256};

/// Canonical rules description used by the retained v1 two-piece asset.
pub const RULES_MANIFEST_JSON: &str = concat!(
    "{\"blue_den\":59,\"blue_traps\":[52,58,60],\"board\":[9,7],",
    "\"capture\":{\"elephant_cannot_capture_untrapped_rat\":true,",
    "\"elephant_captures_trapped_rat\":true,\"enemy_trap_rank\":0,",
    "\"equal_or_lower_effective_rank\":true,\"land_rat_captures_elephant\":true,",
    "\"shore_capture_forbidden\":true,\"water_capture\":\"rat-versus-rat-only\"},",
    "\"jumpers\":[6,7],\"movement\":{\"only_rat_enters_water\":true,",
    "\"orthogonal_steps\":true,\"own_den_forbidden\":true,",
    "\"rat_blocks_river_jump\":true},\"red_den\":3,",
    "\"red_traps\":[2,4,10],\"semantic_rules_version\":2,",
    "\"terminal\":[\"opponent-den\",\"capture-all\",\"no-legal-moves\"],",
    "\"tiger_jump_water_span\":2,\"trap_owners\":[[2,\"red\"],[4,\"red\"],",
    "[10,\"red\"],[52,\"blue\"],[58,\"blue\"],[60,\"blue\"]],",
    "\"water\":[22,23,25,26,29,30,32,33,36,37,39,40]}"
);

/// SHA-256 of [`RULES_MANIFEST_JSON`], matching `two_piece_v1.jgtb`.
pub const RULES_HASH: [u8; 32] = [
    0xbd, 0x5e, 0x29, 0xe5, 0xba, 0xf5, 0x01, 0x14, 0xaa, 0x5c, 0xe6, 0x2f, 0x7b, 0xfa, 0xee, 0x32,
    0x5a, 0x4a, 0xab, 0xc4, 0xba, 0x69, 0xe7, 0xb9, 0x83, 0xf3, 0xba, 0x79, 0xc0, 0x0a, 0x0c, 0x3a,
];

pub const CURRENT_RULES_MANIFEST_JSON: &str = RULES_MANIFEST_JSON;

/// SHA-256 of [`CURRENT_RULES_MANIFEST_JSON`].
pub const CURRENT_RULES_FINGERPRINT: RulesFingerprint = RulesFingerprint(RULES_HASH);

/// A semantic-rules SHA-256 fingerprint embedded into every asset.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RulesFingerprint(pub [u8; 32]);

impl RulesFingerprint {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn from_manifest_bytes(bytes: &[u8]) -> Self {
        Self(sha256(bytes))
    }

    pub fn from_hex(value: &str) -> Result<Self, TablebaseError> {
        Ok(Self(hex_decode(value)?))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        hex_encode(&self.0)
    }
}

impl fmt::Debug for RulesFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RulesFingerprint")
            .field(&self.to_hex())
            .finish()
    }
}

impl fmt::Display for RulesFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl Serialize for RulesFingerprint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for RulesFingerprint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_hex(&value).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_fingerprint_matches_canonical_manifest_and_legacy_asset() {
        assert_eq!(
            RulesFingerprint::from_manifest_bytes(CURRENT_RULES_MANIFEST_JSON.as_bytes()),
            CURRENT_RULES_FINGERPRINT
        );
        assert_eq!(
            CURRENT_RULES_FINGERPRINT.to_hex(),
            "bd5e29e5baf50114aa5ce62f7bfaee325a4aabc4ba69e7b983f3ba79c00a0c3a"
        );
    }

    #[test]
    fn fingerprint_json_is_stable_hex() {
        let json = serde_json::to_string(&CURRENT_RULES_FINGERPRINT).unwrap();
        assert_eq!(json.len(), 66);
        assert_eq!(
            serde_json::from_str::<RulesFingerprint>(&json).unwrap(),
            CURRENT_RULES_FINGERPRINT
        );
    }
}
