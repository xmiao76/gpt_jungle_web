//! Exact Jungle endgame tablebase storage and probing.
//!
//! The crate deliberately does not depend on the search implementation. A caller
//! converts its canonical position to a [`MaterialSignature`] and collision-free
//! [`PositionKey`], then probes a [`TablebaseSet`]. Outcomes are always from the
//! side-to-move's point of view.

mod entry;
mod fingerprint;
mod legacy;
mod manifest;
mod material;
mod shard;

pub use entry::{TablebaseHit, Wdl};
pub use fingerprint::{CURRENT_RULES_FINGERPRINT, CURRENT_RULES_MANIFEST_JSON, RulesFingerprint};
pub use legacy::{
    LEGACY_ENTRY_COUNT, LEGACY_FORMAT_VERSION, LEGACY_TWO_PIECE_ASSET_NAME,
    LegacyTwoPieceTablebase, SideToMove, legacy_dense_index, two_piece_position_key,
};
pub use manifest::{
    ArtifactKind, DEFAULT_CACHE_BUDGET_BYTES, ENGINE_ABI, MAX_SHARD_BYTES_EXCLUSIVE, ManifestShard,
    MaterialManifest, RUNTIME_MANIFEST_SCHEMA, RUNTIME_MANIFEST_VERSION, TablebaseManifest,
    select_materials_within_budget,
};
pub use material::{MaterialSignature, PieceKind, PositionKey};
pub use shard::{
    SHARD_FORMAT_VERSION, SHARD_HEADER_BYTES, Shard, ShardHeader, ShardSummary,
    StreamingShardWriter, TablebaseProbe, TablebaseSet,
};

use thiserror::Error;

/// Failures are reported while loading or building assets, never as search panics.
#[derive(Debug, Error)]
pub enum TablebaseError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    InvalidFormat(String),
    #[error("tablebase rules fingerprint {actual} does not match engine fingerprint {expected}")]
    RulesMismatch {
        expected: RulesFingerprint,
        actual: RulesFingerprint,
    },
    #[error("checksum mismatch for {what}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        what: String,
        expected: String,
        actual: String,
    },
    #[error("tablebase manifest is invalid: {0}")]
    InvalidManifest(String),
}

pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    Sha256::digest(bytes).into()
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

pub(crate) fn hex_decode<const N: usize>(value: &str) -> Result<[u8; N], TablebaseError> {
    if value.len() != N * 2 {
        return Err(TablebaseError::InvalidFormat(format!(
            "expected {} hexadecimal characters, got {}",
            N * 2,
            value.len()
        )));
    }

    fn nibble(value: u8) -> Option<u8> {
        match value {
            b'0'..=b'9' => Some(value - b'0'),
            b'a'..=b'f' => Some(value - b'a' + 10),
            b'A'..=b'F' => Some(value - b'A' + 10),
            _ => None,
        }
    }

    let bytes = value.as_bytes();
    let mut decoded = [0; N];
    for (index, pair) in bytes.chunks_exact(2).enumerate() {
        let high = nibble(pair[0]).ok_or_else(|| {
            TablebaseError::InvalidFormat(format!("invalid hexadecimal digest {value:?}"))
        })?;
        let low = nibble(pair[1]).ok_or_else(|| {
            TablebaseError::InvalidFormat(format!("invalid hexadecimal digest {value:?}"))
        })?;
        decoded[index] = (high << 4) | low;
    }
    Ok(decoded)
}
