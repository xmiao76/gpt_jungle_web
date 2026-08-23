use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    MaterialSignature, PositionKey, RulesFingerprint, Shard, TablebaseError, TablebaseSet,
    hex_decode, hex_encode,
};

pub const DEFAULT_CACHE_BUDGET_BYTES: u64 = 250 * 1024 * 1024;
pub const MAX_SHARD_BYTES_EXCLUSIVE: u64 = 25 * 1024 * 1024;
pub const RUNTIME_MANIFEST_SCHEMA: &str = "gpt-jungle-engine-runtime";
pub const RUNTIME_MANIFEST_VERSION: u16 = 2;
pub const ENGINE_ABI: &str = "jungle-wasm-v2";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    Engine,
    Book,
    Model,
    Tablebase,
}

/// Runtime/cache descriptor plus exact shard integrity metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestShard {
    pub id: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
    pub kind: ArtifactKind,
    pub rules_fingerprint: RulesFingerprint,
    pub payload_sha256: String,
    pub records: u64,
    pub min_key: PositionKey,
    pub max_key: PositionKey,
    pub max_distance: u16,
}

impl ManifestShard {
    pub fn from_shard(id: impl Into<String>, url: impl Into<String>, shard: &Shard) -> Self {
        let header = shard.header();
        Self {
            id: id.into(),
            url: url.into(),
            sha256: hex_encode(&shard.content_sha256()),
            bytes: shard.as_bytes().len() as u64,
            kind: ArtifactKind::Tablebase,
            rules_fingerprint: header.rules_fingerprint,
            payload_sha256: hex_encode(&header.payload_sha256),
            records: header.entry_count,
            min_key: header.min_key,
            max_key: header.max_key,
            max_distance: header.max_distance,
        }
    }
}

/// A complete material signature may span several independently cacheable files.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialManifest {
    pub signature: MaterialSignature,
    pub complete: bool,
    pub position_count: u64,
    pub record_count: u64,
    pub bytes: u64,
    /// Integer utility gathered from a held-out probe corpus.
    pub probe_value: u64,
    pub sample_positions: u64,
    pub shards: Vec<ManifestShard>,
}

impl MaterialManifest {
    pub fn new(
        signature: MaterialSignature,
        complete: bool,
        position_count: u64,
        probe_value: u64,
        sample_positions: u64,
        mut shards: Vec<ManifestShard>,
    ) -> Self {
        shards.sort_by_key(|shard| shard.min_key);
        let record_count = shards.iter().map(|shard| shard.records).sum();
        let bytes = shards.iter().map(|shard| shard.bytes).sum();
        Self {
            signature,
            complete,
            position_count,
            record_count,
            bytes,
            probe_value,
            sample_positions,
            shards,
        }
    }

    pub fn value_per_byte(&self) -> f64 {
        if self.bytes == 0 {
            0.0
        } else {
            self.probe_value as f64 / self.bytes as f64
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TablebaseManifest {
    pub schema: String,
    pub version: u16,
    pub engine_abi: String,
    pub rules_fingerprint: RulesFingerprint,
    pub materials: Vec<MaterialManifest>,
}

impl TablebaseManifest {
    pub fn new(rules_fingerprint: RulesFingerprint, mut materials: Vec<MaterialManifest>) -> Self {
        materials.sort_by_key(|material| material.signature);
        Self {
            schema: RUNTIME_MANIFEST_SCHEMA.into(),
            version: RUNTIME_MANIFEST_VERSION,
            engine_abi: ENGINE_ABI.into(),
            rules_fingerprint,
            materials,
        }
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, TablebaseError> {
        let manifest: Self = serde_json::from_slice(bytes)?;
        manifest.validate(manifest.rules_fingerprint)?;
        Ok(manifest)
    }

    pub fn to_json_bytes(&self) -> Result<Vec<u8>, TablebaseError> {
        self.validate(self.rules_fingerprint)?;
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn validate(&self, expected_rules: RulesFingerprint) -> Result<(), TablebaseError> {
        if self.schema != RUNTIME_MANIFEST_SCHEMA {
            return Err(TablebaseError::InvalidManifest(format!(
                "schema {:?} is not {:?}",
                self.schema, RUNTIME_MANIFEST_SCHEMA
            )));
        }
        if self.version != RUNTIME_MANIFEST_VERSION {
            return Err(TablebaseError::InvalidManifest(format!(
                "version {} is not {}",
                self.version, RUNTIME_MANIFEST_VERSION
            )));
        }
        if self.engine_abi != ENGINE_ABI {
            return Err(TablebaseError::InvalidManifest(format!(
                "engine ABI {:?} is not {:?}",
                self.engine_abi, ENGINE_ABI
            )));
        }
        if self.rules_fingerprint != expected_rules {
            return Err(TablebaseError::RulesMismatch {
                expected: expected_rules,
                actual: self.rules_fingerprint,
            });
        }
        let mut ids = BTreeSet::new();
        let mut previous_signature = None;
        for material in &self.materials {
            material.signature.validate()?;
            if previous_signature.is_some_and(|previous| material.signature <= previous) {
                return Err(TablebaseError::InvalidManifest(
                    "material signatures must be unique and sorted".into(),
                ));
            }
            previous_signature = Some(material.signature);
            if material.shards.is_empty() {
                return Err(TablebaseError::InvalidManifest(format!(
                    "material {} has no shards",
                    material.signature
                )));
            }
            let records: u64 = material.shards.iter().map(|shard| shard.records).sum();
            let bytes: u64 = material.shards.iter().map(|shard| shard.bytes).sum();
            if material.record_count != records || material.bytes != bytes {
                return Err(TablebaseError::InvalidManifest(format!(
                    "material {} aggregate counts do not match its shards",
                    material.signature
                )));
            }
            if material.complete && material.position_count != material.record_count {
                return Err(TablebaseError::InvalidManifest(format!(
                    "complete material {} declares {} positions but contains {} records",
                    material.signature, material.position_count, material.record_count
                )));
            }
            if material.sample_positions == 0 && material.probe_value != 0 {
                return Err(TablebaseError::InvalidManifest(format!(
                    "material {} has measured probe value without samples",
                    material.signature
                )));
            }

            let mut previous_max_key = None;
            for shard in &material.shards {
                if shard.id.is_empty() || !ids.insert(shard.id.clone()) {
                    return Err(TablebaseError::InvalidManifest(format!(
                        "artifact id {:?} is empty or duplicated",
                        shard.id
                    )));
                }
                if shard.url.is_empty() {
                    return Err(TablebaseError::InvalidManifest(format!(
                        "artifact {} has an empty URL",
                        shard.id
                    )));
                }
                if shard.kind != ArtifactKind::Tablebase {
                    return Err(TablebaseError::InvalidManifest(format!(
                        "material shard {} is not a tablebase artifact",
                        shard.id
                    )));
                }
                if shard.rules_fingerprint != expected_rules {
                    return Err(TablebaseError::RulesMismatch {
                        expected: expected_rules,
                        actual: shard.rules_fingerprint,
                    });
                }
                hex_decode::<32>(&shard.sha256).map_err(|error| {
                    TablebaseError::InvalidManifest(format!(
                        "artifact {} content hash is invalid: {error}",
                        shard.id
                    ))
                })?;
                hex_decode::<32>(&shard.payload_sha256).map_err(|error| {
                    TablebaseError::InvalidManifest(format!(
                        "artifact {} payload hash is invalid: {error}",
                        shard.id
                    ))
                })?;
                if shard.bytes == 0 || shard.bytes >= MAX_SHARD_BYTES_EXCLUSIVE {
                    return Err(TablebaseError::InvalidManifest(format!(
                        "artifact {} has {} bytes; every shard must be smaller than {}",
                        shard.id, shard.bytes, MAX_SHARD_BYTES_EXCLUSIVE
                    )));
                }
                if shard.records == 0 || shard.min_key > shard.max_key {
                    return Err(TablebaseError::InvalidManifest(format!(
                        "artifact {} has an empty or inverted key range",
                        shard.id
                    )));
                }
                if previous_max_key.is_some_and(|previous| shard.min_key <= previous) {
                    return Err(TablebaseError::InvalidManifest(format!(
                        "material {} has overlapping or unsorted shard ranges",
                        material.signature
                    )));
                }
                previous_max_key = Some(shard.max_key);
            }
        }
        Ok(())
    }

    /// Load all listed shards through a filesystem, network, or embedded-data callback.
    pub fn load_with<F>(
        &self,
        expected_rules: RulesFingerprint,
        mut load: F,
    ) -> Result<TablebaseSet, TablebaseError>
    where
        F: FnMut(&ManifestShard) -> Result<Vec<u8>, TablebaseError>,
    {
        self.validate(expected_rules)?;
        let mut set = TablebaseSet::new(expected_rules);
        for material in &self.materials {
            for descriptor in &material.shards {
                let bytes = load(descriptor)?;
                if bytes.len() as u64 != descriptor.bytes {
                    return Err(TablebaseError::InvalidFormat(format!(
                        "artifact {} has {} bytes; manifest declares {}",
                        descriptor.id,
                        bytes.len(),
                        descriptor.bytes
                    )));
                }
                let actual_sha256 = crate::sha256(&bytes);
                let expected_sha256 = hex_decode::<32>(&descriptor.sha256)?;
                if actual_sha256 != expected_sha256 {
                    return Err(TablebaseError::ChecksumMismatch {
                        what: descriptor.id.clone(),
                        expected: descriptor.sha256.clone(),
                        actual: hex_encode(&actual_sha256),
                    });
                }
                let shard = Shard::from_bytes(bytes)?;
                let header = shard.header();
                if header.material_signature != material.signature
                    || header.entry_count != descriptor.records
                    || header.min_key != descriptor.min_key
                    || header.max_key != descriptor.max_key
                    || header.max_distance != descriptor.max_distance
                    || hex_encode(&header.payload_sha256) != descriptor.payload_sha256
                {
                    return Err(TablebaseError::InvalidManifest(format!(
                        "artifact {} header does not match its manifest descriptor",
                        descriptor.id
                    )));
                }
                set.insert(shard)?;
            }
        }
        Ok(set)
    }
}

/// Select complete material groups for an IndexedDB budget.
///
/// All complete one- and two-piece groups are mandatory. Remaining groups are
/// considered atomically in descending measured probe-value-per-byte order.
pub fn select_materials_within_budget(
    materials: &[MaterialManifest],
    budget_bytes: u64,
) -> Result<Vec<&MaterialManifest>, TablebaseError> {
    let mut required: Vec<_> = materials
        .iter()
        .filter(|material| material.complete && material.signature.total_pieces() <= 2)
        .collect();
    required.sort_by_key(|material| material.signature);
    let required_bytes = required.iter().try_fold(0_u64, |total, material| {
        total
            .checked_add(material.bytes)
            .ok_or_else(|| TablebaseError::InvalidManifest("material budget size overflow".into()))
    })?;
    if required_bytes > budget_bytes {
        return Err(TablebaseError::InvalidManifest(format!(
            "mandatory two-piece tablebases need {required_bytes} bytes but budget is {budget_bytes}"
        )));
    }

    let mut ranked: Vec<_> = materials
        .iter()
        .filter(|material| material.complete && material.signature.total_pieces() > 2)
        .collect();
    ranked.sort_by(|left, right| {
        // Compare ratios exactly without floating-point or platform-dependent ties.
        let left_scaled = u128::from(left.probe_value) * u128::from(right.bytes);
        let right_scaled = u128::from(right.probe_value) * u128::from(left.bytes);
        right_scaled
            .cmp(&left_scaled)
            .then_with(|| right.probe_value.cmp(&left.probe_value))
            .then_with(|| left.bytes.cmp(&right.bytes))
            .then_with(|| left.signature.cmp(&right.signature))
    });

    let mut selected = required;
    let mut used = required_bytes;
    for material in ranked {
        let Some(candidate_total) = used.checked_add(material.bytes) else {
            continue;
        };
        if candidate_total <= budget_bytes {
            selected.push(material);
            used = candidate_total;
        }
    }
    // Return load order by smaller endgames first and then canonical signature;
    // ranking was only a selection policy.
    selected.sort_by(|left, right| {
        left.signature
            .total_pieces()
            .cmp(&right.signature.total_pieces())
            .then_with(|| left.signature.cmp(&right.signature))
    });
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CURRENT_RULES_FINGERPRINT, PieceKind, TablebaseHit};

    fn material(signature: MaterialSignature, bytes: u64, probe_value: u64) -> MaterialManifest {
        let shard = Shard::from_records(
            CURRENT_RULES_FINGERPRINT,
            signature,
            true,
            [(PositionKey(1), TablebaseHit::draw())],
        )
        .unwrap();
        let mut descriptor = ManifestShard::from_shard(
            format!("{signature}"),
            format!("/generated/tablebase/{signature}.jgtbs"),
            &shard,
        );
        descriptor.bytes = bytes;
        MaterialManifest::new(signature, true, 1, probe_value, 1_000, vec![descriptor])
    }

    #[test]
    fn deterministic_manifest_round_trip() {
        let rat_rat = material(
            MaterialSignature::one_vs_one(PieceKind::Rat, PieceKind::Rat),
            200,
            1,
        );
        let manifest = TablebaseManifest::new(CURRENT_RULES_FINGERPRINT, vec![rat_rat]);
        let first = manifest.to_json_bytes().unwrap();
        let second = manifest.to_json_bytes().unwrap();
        assert_eq!(first, second);
        assert_eq!(TablebaseManifest::from_json(&first).unwrap(), manifest);
        let value: serde_json::Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(value["schema"], RUNTIME_MANIFEST_SCHEMA);
        assert_eq!(value["version"], RUNTIME_MANIFEST_VERSION);
        assert_eq!(value["engineAbi"], ENGINE_ABI);
        let descriptor = &value["materials"][0]["shards"][0];
        for field in ["id", "url", "sha256", "bytes", "kind", "rulesFingerprint"] {
            assert!(descriptor.get(field).is_some(), "missing {field}");
        }
    }

    #[test]
    fn budget_keeps_two_piece_and_ranks_remaining_by_value_per_byte() {
        let two = material(
            MaterialSignature::one_vs_one(PieceKind::Rat, PieceKind::Rat),
            100,
            0,
        );
        let mut three_signature = MaterialSignature::one_vs_one(PieceKind::Cat, PieceKind::Cat);
        three_signature.blue[2] = 1;
        let three_high_ratio = material(three_signature, 200, 100);
        let mut four_signature = MaterialSignature::one_vs_one(PieceKind::Dog, PieceKind::Dog);
        four_signature.blue[0] = 1;
        four_signature.red[0] = 1;
        let four_low_ratio = material(four_signature, 200, 10);

        let candidates = [
            two.clone(),
            four_low_ratio.clone(),
            three_high_ratio.clone(),
        ];
        let selected = select_materials_within_budget(&candidates, 300).unwrap();
        assert_eq!(selected.len(), 2);
        assert!(selected.iter().any(|item| item.signature == two.signature));
        assert!(
            selected
                .iter()
                .any(|item| item.signature == three_high_ratio.signature)
        );
        assert!(
            !selected
                .iter()
                .any(|item| item.signature == four_low_ratio.signature)
        );
    }

    #[test]
    fn budget_rejects_omitting_mandatory_two_piece_data() {
        let two = material(
            MaterialSignature::one_vs_one(PieceKind::Rat, PieceKind::Rat),
            101,
            0,
        );
        assert!(select_materials_within_budget(&[two], 100).is_err());
    }

    #[test]
    fn manifest_load_checks_outer_and_inner_checksums() {
        let signature = MaterialSignature::one_vs_one(PieceKind::Rat, PieceKind::Rat);
        let shard = Shard::from_records(
            CURRENT_RULES_FINGERPRINT,
            signature,
            true,
            [(PositionKey(1), TablebaseHit::win(2))],
        )
        .unwrap();
        let descriptor = ManifestShard::from_shard("rat-rat", "/rat-rat.jgtbs", &shard);
        let material = MaterialManifest::new(signature, true, 1, 1, 1, vec![descriptor]);
        let manifest = TablebaseManifest::new(CURRENT_RULES_FINGERPRINT, vec![material]);
        let set = manifest
            .load_with(CURRENT_RULES_FINGERPRINT, |_| Ok(shard.as_bytes().to_vec()))
            .unwrap();
        assert_eq!(set.shard_count(), 1);

        let result = manifest.load_with(CURRENT_RULES_FINGERPRINT, |_| {
            let mut corrupt = shard.as_bytes().to_vec();
            *corrupt.last_mut().unwrap() ^= 1;
            Ok(corrupt)
        });
        assert!(matches!(
            result,
            Err(TablebaseError::ChecksumMismatch { .. })
        ));
    }
}
