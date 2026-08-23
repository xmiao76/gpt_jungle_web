use std::{collections::BTreeMap, sync::Arc};

use jungle_search::{EndgameOracle, OracleHit, OracleOutcome};
use jungle_tablebase::{
    CURRENT_RULES_FINGERPRINT, LEGACY_ENTRY_COUNT, LegacyTwoPieceTablebase,
    MAX_SHARD_BYTES_EXCLUSIVE, MaterialSignature, PositionKey, RulesFingerprint, Shard, Wdl,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

const LEGACY_HEADER_BYTES: usize = 84;
const LEGACY_ENTRY_BYTES: usize = 2;
const LEGACY_ASSET_SHA256: &str =
    "a32b620d22d1735ef4f2a13b8d1388c07ad45151060f638c2434db988f97ece5";
const LEGACY_PAYLOAD_SHA256: &str =
    "d4c4047e2efefa420c60e19170f080d09d4cb13fc986c2e5173975c630539b45";
pub(crate) const OPTIONAL_RESIDENT_CAP_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const LEGACY_MATERIAL_COUNT: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DescriptorFormat {
    Legacy,
    SparseShard,
}

#[derive(Debug, Error)]
pub(crate) enum LoadError {
    #[error("tablebase descriptor JSON is invalid: {0}")]
    DescriptorJson(#[from] serde_json::Error),
    #[error("tablebase descriptor is invalid: {0}")]
    Descriptor(String),
    #[error("tablebase asset is invalid: {0}")]
    Asset(#[from] jungle_tablebase::TablebaseError),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyDescriptor {
    id: String,
    url: String,
    sha256: String,
    bytes: u64,
    kind: String,
    rules_fingerprint: String,
    tablebase: LegacyMetadata,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyMetadata {
    format: String,
    complete_material: bool,
    records: u64,
    max_distance: u16,
    payload_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SparseDescriptor {
    id: String,
    url: String,
    sha256: String,
    bytes: u64,
    kind: String,
    rules_fingerprint: String,
    tablebase: SparseMetadata,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SparseMetadata {
    format: String,
    material: SparseMaterial,
    complete_material: bool,
    records: u64,
    max_distance: u16,
    min_key: String,
    max_key: String,
    payload_sha256: String,
}

#[derive(Debug, Deserialize)]
struct SparseMaterial {
    id: String,
    blue: [u8; 8],
    red: [u8; 8],
}

#[derive(Debug, Deserialize)]
struct FormatProbe {
    tablebase: FormatName,
}

#[derive(Debug, Deserialize)]
struct FormatName {
    format: String,
}

#[derive(Clone, Debug)]
struct LoadedShard {
    id: String,
    asset_sha256: String,
    shard: Arc<Shard>,
}

#[derive(Clone, Debug)]
pub(crate) struct ShardLoadReport {
    pub artifact_id: String,
    pub material_id: String,
    pub idempotent: bool,
    pub max_distance: u16,
    pub bytes: usize,
}

/// Validated composite oracle. The mandatory legacy tablebase remains the
/// source of truth for exact one-versus-one material; independently validated
/// sparse shards extend it to larger material signatures.
#[derive(Clone, Debug)]
pub(crate) struct LoadedTablebase {
    legacy: Arc<LegacyTwoPieceTablebase>,
    asset_sha256: String,
    shards: BTreeMap<MaterialSignature, Vec<LoadedShard>>,
}

impl LoadedTablebase {
    pub(crate) fn load_legacy(bytes: &[u8], descriptor_json: &str) -> Result<Self, LoadError> {
        let descriptor: LegacyDescriptor = serde_json::from_str(descriptor_json)?;
        validate_legacy_descriptor(&descriptor, bytes.len())?;

        let asset_sha256 = hex_sha256(bytes);
        if descriptor.sha256 != asset_sha256 {
            return Err(LoadError::Descriptor(format!(
                "sha256 declares {}, actual asset is {asset_sha256}",
                descriptor.sha256
            )));
        }
        let payload = bytes.get(LEGACY_HEADER_BYTES..).ok_or_else(|| {
            LoadError::Descriptor("asset is shorter than the legacy header".to_owned())
        })?;
        let payload_sha256 = hex_sha256(payload);
        if descriptor.tablebase.payload_sha256 != payload_sha256 {
            return Err(LoadError::Descriptor(format!(
                "payloadSha256 declares {}, actual payload is {payload_sha256}",
                descriptor.tablebase.payload_sha256
            )));
        }

        // This performs independent magic/version/size/rules/payload checksum
        // and entry validation before the oracle is installed.
        let legacy = LegacyTwoPieceTablebase::from_bytes(bytes)?;
        if descriptor.tablebase.max_distance != legacy.max_distance() {
            return Err(LoadError::Descriptor(format!(
                "maxDistance declares {}, decoded asset is {}",
                descriptor.tablebase.max_distance,
                legacy.max_distance()
            )));
        }
        Ok(Self {
            legacy: Arc::new(legacy),
            asset_sha256,
            shards: BTreeMap::new(),
        })
    }

    pub(crate) fn with_sparse_shard(
        &self,
        bytes: &[u8],
        descriptor_json: &str,
    ) -> Result<(Self, ShardLoadReport), LoadError> {
        let descriptor: SparseDescriptor = serde_json::from_str(descriptor_json)?;
        let asset_sha256 = hex_sha256(bytes);
        if bytes.len() as u64 >= MAX_SHARD_BYTES_EXCLUSIVE {
            return Err(LoadError::Descriptor(format!(
                "sparse shard has {} bytes; limit is below {MAX_SHARD_BYTES_EXCLUSIVE}",
                bytes.len()
            )));
        }
        if descriptor.bytes != bytes.len() as u64 || descriptor.sha256 != asset_sha256 {
            return Err(LoadError::Descriptor(
                "sparse shard bytes do not match the descriptor integrity fields".to_owned(),
            ));
        }
        let shard = Shard::from_bytes(bytes.to_vec())?;
        validate_sparse_descriptor(&descriptor, bytes.len(), &asset_sha256, &shard)?;
        let header = shard.header();

        if let Some(existing) = self
            .shards
            .values()
            .flatten()
            .find(|existing| existing.id == descriptor.id)
        {
            if existing.asset_sha256 != asset_sha256
                || existing.shard.header() != header
                || existing.shard.as_bytes() != bytes
            {
                return Err(LoadError::Descriptor(format!(
                    "artifact id {:?} is already loaded with different content",
                    descriptor.id
                )));
            }
            return Ok((
                self.clone(),
                ShardLoadReport {
                    artifact_id: descriptor.id,
                    material_id: header.material_signature.to_string(),
                    idempotent: true,
                    max_distance: header.max_distance,
                    bytes: bytes.len(),
                },
            ));
        }

        let projected_resident_bytes = self.optional_resident_bytes().saturating_add(bytes.len());
        if projected_resident_bytes > OPTIONAL_RESIDENT_CAP_BYTES {
            return Err(LoadError::Descriptor(format!(
                "optional shard would raise resident bytes to {projected_resident_bytes}; per-lane cap is {OPTIONAL_RESIDENT_CAP_BYTES}"
            )));
        }

        let material_shards = self
            .shards
            .get(&header.material_signature)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if material_shards.iter().any(|existing| {
            let other = existing.shard.header();
            header.min_key <= other.max_key && other.min_key <= header.max_key
        }) {
            return Err(LoadError::Descriptor(format!(
                "artifact {:?} overlaps an already loaded shard for material {}",
                descriptor.id, header.material_signature
            )));
        }

        let mut updated = self.clone();
        let material_shards = updated.shards.entry(header.material_signature).or_default();
        material_shards.push(LoadedShard {
            id: descriptor.id.clone(),
            asset_sha256,
            shard: Arc::new(shard),
        });
        material_shards.sort_by_key(|candidate| candidate.shard.header().min_key);
        Ok((
            updated,
            ShardLoadReport {
                artifact_id: descriptor.id,
                material_id: header.material_signature.to_string(),
                idempotent: false,
                max_distance: header.max_distance,
                bytes: bytes.len(),
            },
        ))
    }

    pub(crate) fn rules_fingerprint(&self) -> RulesFingerprint {
        self.legacy.rules_fingerprint()
    }

    pub(crate) fn max_distance(&self) -> u16 {
        self.legacy.max_distance()
    }

    pub(crate) fn asset_sha256(&self) -> &str {
        &self.asset_sha256
    }

    pub(crate) fn shard_count(&self) -> usize {
        self.shards.values().map(Vec::len).sum()
    }

    pub(crate) fn material_count(&self) -> usize {
        self.shards.len()
    }

    pub(crate) fn max_loaded_pieces(&self) -> u16 {
        self.shards
            .keys()
            .map(|material| material.total_pieces())
            .max()
            .unwrap_or(2)
            .max(2)
    }

    pub(crate) fn loaded_artifact_ids(&self) -> Vec<&str> {
        let mut ids = self
            .shards
            .values()
            .flatten()
            .map(|shard| shard.id.as_str())
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids
    }

    pub(crate) fn optional_resident_bytes(&self) -> usize {
        self.shards
            .values()
            .flatten()
            .map(|shard| shard.shard.as_bytes().len())
            .sum()
    }
}

impl EndgameOracle for LoadedTablebase {
    fn probe(&self, position: &jungle_core::Position) -> Option<OracleHit> {
        let hit = self.legacy.probe_position(position).or_else(|| {
            let material = MaterialSignature::from_position(position);
            let key = PositionKey::from_position(position);
            let shards = self.shards.get(&material)?;
            let candidate = shards.partition_point(|shard| shard.shard.header().max_key < key);
            shards.get(candidate)?.shard.probe(key)
        });
        hit.map(|hit| OracleHit {
            outcome: match hit.wdl {
                Wdl::Win => OracleOutcome::Win,
                Wdl::Draw => OracleOutcome::Draw,
                Wdl::Loss => OracleOutcome::Loss,
            },
            distance: hit.distance,
        })
    }
}

pub(crate) fn descriptor_format(descriptor_json: &str) -> Result<DescriptorFormat, LoadError> {
    let probe: FormatProbe = serde_json::from_str(descriptor_json)?;
    match probe.tablebase.format.as_str() {
        "legacy-two-piece-v1" => Ok(DescriptorFormat::Legacy),
        "sparse-shard-v1" => Ok(DescriptorFormat::SparseShard),
        other => Err(LoadError::Descriptor(format!(
            "unsupported tablebase format {other:?}"
        ))),
    }
}

fn validate_legacy_descriptor(
    descriptor: &LegacyDescriptor,
    actual_bytes: usize,
) -> Result<(), LoadError> {
    if descriptor.id != "tablebase:two-piece-v1" {
        return Err(LoadError::Descriptor(format!(
            "unexpected id {:?}",
            descriptor.id
        )));
    }
    if descriptor.kind != "tablebase" {
        return Err(LoadError::Descriptor(format!(
            "unexpected kind {:?}",
            descriptor.kind
        )));
    }
    if descriptor.url.trim().is_empty() {
        return Err(LoadError::Descriptor("url must not be empty".to_owned()));
    }
    if descriptor.bytes != actual_bytes as u64 {
        return Err(LoadError::Descriptor(format!(
            "bytes declares {}, received {actual_bytes}",
            descriptor.bytes
        )));
    }
    if descriptor.sha256 != LEGACY_ASSET_SHA256 {
        return Err(LoadError::Descriptor(format!(
            "sha256 declares {}, expected canonical asset {LEGACY_ASSET_SHA256}",
            descriptor.sha256
        )));
    }
    if descriptor.rules_fingerprint != CURRENT_RULES_FINGERPRINT.to_hex() {
        return Err(LoadError::Descriptor(format!(
            "rulesFingerprint declares {}, expected {}",
            descriptor.rules_fingerprint,
            CURRENT_RULES_FINGERPRINT.to_hex()
        )));
    }
    if descriptor.tablebase.format != "legacy-two-piece-v1" {
        return Err(LoadError::Descriptor(format!(
            "unexpected tablebase format {:?}",
            descriptor.tablebase.format
        )));
    }
    if !descriptor.tablebase.complete_material {
        return Err(LoadError::Descriptor(
            "completeMaterial must be true".to_owned(),
        ));
    }
    if descriptor.tablebase.records != LEGACY_ENTRY_COUNT as u64 {
        return Err(LoadError::Descriptor(format!(
            "records declares {}, expected {LEGACY_ENTRY_COUNT}",
            descriptor.tablebase.records
        )));
    }
    if descriptor.tablebase.payload_sha256 != LEGACY_PAYLOAD_SHA256 {
        return Err(LoadError::Descriptor(format!(
            "payloadSha256 declares {}, expected canonical payload {LEGACY_PAYLOAD_SHA256}",
            descriptor.tablebase.payload_sha256
        )));
    }
    let expected_bytes = LEGACY_HEADER_BYTES + LEGACY_ENTRY_COUNT * LEGACY_ENTRY_BYTES;
    if actual_bytes != expected_bytes {
        return Err(LoadError::Descriptor(format!(
            "legacy asset has {actual_bytes} bytes; expected {expected_bytes}"
        )));
    }
    Ok(())
}

fn validate_sparse_descriptor(
    descriptor: &SparseDescriptor,
    actual_bytes: usize,
    asset_sha256: &str,
    shard: &Shard,
) -> Result<(), LoadError> {
    if descriptor.id.trim().is_empty() || descriptor.id == "tablebase:two-piece-v1" {
        return Err(LoadError::Descriptor(
            "a sparse shard requires a non-reserved artifact id".to_owned(),
        ));
    }
    if descriptor.kind != "tablebase" {
        return Err(LoadError::Descriptor(format!(
            "unexpected kind {:?}",
            descriptor.kind
        )));
    }
    if descriptor.url.trim().is_empty() {
        return Err(LoadError::Descriptor("url must not be empty".to_owned()));
    }
    if descriptor.bytes != actual_bytes as u64 {
        return Err(LoadError::Descriptor(format!(
            "bytes declares {}, received {actual_bytes}",
            descriptor.bytes
        )));
    }
    if descriptor.bytes >= MAX_SHARD_BYTES_EXCLUSIVE {
        return Err(LoadError::Descriptor(format!(
            "sparse shard has {} bytes; limit is below {MAX_SHARD_BYTES_EXCLUSIVE}",
            descriptor.bytes
        )));
    }
    if descriptor.sha256 != asset_sha256 {
        return Err(LoadError::Descriptor(format!(
            "sha256 declares {}, actual asset is {asset_sha256}",
            descriptor.sha256
        )));
    }
    if descriptor.rules_fingerprint != CURRENT_RULES_FINGERPRINT.to_hex() {
        return Err(LoadError::Descriptor(format!(
            "rulesFingerprint declares {}, expected {}",
            descriptor.rules_fingerprint,
            CURRENT_RULES_FINGERPRINT.to_hex()
        )));
    }
    if descriptor.tablebase.format != "sparse-shard-v1" {
        return Err(LoadError::Descriptor(format!(
            "unexpected tablebase format {:?}",
            descriptor.tablebase.format
        )));
    }

    let header = shard.header();
    let material = MaterialSignature::new(
        descriptor.tablebase.material.blue,
        descriptor.tablebase.material.red,
    );
    material.validate()?;
    if material.total_pieces() <= 2 {
        return Err(LoadError::Descriptor(
            "sparse shards must extend material above the mandatory two-piece tablebase".to_owned(),
        ));
    }
    let min_key = PositionKey::from_hex(&descriptor.tablebase.min_key)?;
    let max_key = PositionKey::from_hex(&descriptor.tablebase.max_key)?;
    if descriptor.tablebase.material.id != material.to_string()
        || header.rules_fingerprint != CURRENT_RULES_FINGERPRINT
        || header.material_signature != material
        || header.complete_material != descriptor.tablebase.complete_material
        || header.entry_count != descriptor.tablebase.records
        || header.max_distance != descriptor.tablebase.max_distance
        || header.min_key != min_key
        || header.max_key != max_key
        || hex_bytes(&header.payload_sha256) != descriptor.tablebase.payload_sha256
    {
        return Err(LoadError::Descriptor(format!(
            "artifact {:?} decoded header does not match its descriptor",
            descriptor.id
        )));
    }
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
pub(crate) const TEST_ASSET_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../python/jungle/ai/data/two_piece_v1.jgtb"
));

#[cfg(test)]
pub(crate) const TEST_DESCRIPTOR_JSON: &str = r#"{
  "id":"tablebase:two-piece-v1",
  "url":"/generated/tablebase/two_piece_v1.jgtb",
  "sha256":"a32b620d22d1735ef4f2a13b8d1388c07ad45151060f638c2434db988f97ece5",
  "bytes":1016148,
  "kind":"tablebase",
  "rulesFingerprint":"bd5e29e5baf50114aa5ce62f7bfaee325a4aabc4ba69e7b983f3ba79c00a0c3a",
  "tablebase":{"format":"legacy-two-piece-v1","completeMaterial":true,"records":508032,"maxDistance":34,"payloadSha256":"d4c4047e2efefa420c60e19170f080d09d4cb13fc986c2e5173975c630539b45"}
}"#;

#[cfg(test)]
pub(crate) fn test_sparse_asset() -> (jungle_core::Position, Vec<u8>, String) {
    use jungle_core::{Piece, PieceKind, Position, Side};
    use jungle_tablebase::TablebaseHit;

    let position = Position::from_piece_placements_checked(
        &[
            (14, Piece::new(Side::Blue, PieceKind::Rat)),
            (21, Piece::new(Side::Blue, PieceKind::Cat)),
            (15, Piece::new(Side::Red, PieceKind::Elephant)),
        ],
        Side::Blue,
    )
    .unwrap();
    let material = MaterialSignature::from_position(&position);
    let shard = Shard::from_records(
        CURRENT_RULES_FINGERPRINT,
        material,
        true,
        [(PositionKey::from_position(&position), TablebaseHit::win(3))],
    )
    .unwrap();
    let header = shard.header();
    let bytes = shard.as_bytes().to_vec();
    let descriptor = serde_json::json!({
        "id": "tablebase:test-three-piece",
        "url": "/generated/tablebase/test-three-piece.jgtbs",
        "sha256": hex_sha256(&bytes),
        "bytes": bytes.len(),
        "kind": "tablebase",
        "rulesFingerprint": CURRENT_RULES_FINGERPRINT.to_hex(),
        "tablebase": {
            "format": "sparse-shard-v1",
            "material": {
                "id": material.to_string(),
                "blue": material.blue,
                "red": material.red,
            },
            "completeMaterial": true,
            "records": header.entry_count,
            "maxDistance": header.max_distance,
            "minKey": header.min_key.to_hex(),
            "maxKey": header.max_key.to_hex(),
            "payloadSha256": hex_bytes(&header.payload_sha256),
        }
    })
    .to_string();
    (position, bytes, descriptor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_legacy_asset_and_descriptor_validate() {
        let tablebase = LoadedTablebase::load_legacy(TEST_ASSET_BYTES, TEST_DESCRIPTOR_JSON)
            .expect("real tablebase must validate");
        assert_eq!(tablebase.rules_fingerprint(), CURRENT_RULES_FINGERPRINT);
        assert_eq!(tablebase.max_distance(), 34);
    }

    #[test]
    fn descriptor_integrity_mismatches_are_rejected() {
        let mut value: serde_json::Value = serde_json::from_str(TEST_DESCRIPTOR_JSON).unwrap();
        value["sha256"] = serde_json::json!("00".repeat(32));
        assert!(LoadedTablebase::load_legacy(TEST_ASSET_BYTES, &value.to_string()).is_err());

        let mut value: serde_json::Value = serde_json::from_str(TEST_DESCRIPTOR_JSON).unwrap();
        value["bytes"] = serde_json::json!(TEST_ASSET_BYTES.len() - 1);
        assert!(LoadedTablebase::load_legacy(TEST_ASSET_BYTES, &value.to_string()).is_err());

        let mut value: serde_json::Value = serde_json::from_str(TEST_DESCRIPTOR_JSON).unwrap();
        value["rulesFingerprint"] = serde_json::json!("00".repeat(32));
        assert!(LoadedTablebase::load_legacy(TEST_ASSET_BYTES, &value.to_string()).is_err());
    }

    #[test]
    fn sparse_shards_are_additive_idempotent_and_probeable() {
        let base = LoadedTablebase::load_legacy(TEST_ASSET_BYTES, TEST_DESCRIPTOR_JSON).unwrap();
        let (position, bytes, descriptor) = test_sparse_asset();
        let (loaded, report) = base.with_sparse_shard(&bytes, &descriptor).unwrap();
        assert!(!report.idempotent);
        assert_eq!(loaded.shard_count(), 1);
        assert_eq!(loaded.material_count(), 1);
        assert_eq!(loaded.max_loaded_pieces(), 3);
        let hit = EndgameOracle::probe(&loaded, &position).unwrap();
        assert_eq!(hit.outcome, OracleOutcome::Win);
        assert_eq!(hit.distance, Some(3));

        let (same, report) = loaded.with_sparse_shard(&bytes, &descriptor).unwrap();
        assert!(report.idempotent);
        assert_eq!(same.shard_count(), 1);

        let mut duplicate: serde_json::Value = serde_json::from_str(&descriptor).unwrap();
        duplicate["id"] = serde_json::json!("tablebase:test-overlap");
        assert!(
            same.with_sparse_shard(&bytes, &duplicate.to_string())
                .is_err()
        );
    }
}
