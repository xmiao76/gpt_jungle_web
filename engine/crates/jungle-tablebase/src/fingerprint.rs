use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{TablebaseError, hex_decode, hex_encode, sha256};

/// Canonical rules description used by the retained v1 two-piece asset.
pub const CURRENT_RULES_MANIFEST_JSON: &str = jungle_core::RULES_MANIFEST_JSON;

/// SHA-256 of [`CURRENT_RULES_MANIFEST_JSON`].
pub const CURRENT_RULES_FINGERPRINT: RulesFingerprint = RulesFingerprint(jungle_core::RULES_HASH);

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
