use crate::{
    CURRENT_RULES_FINGERPRINT, MaterialSignature, PieceKind, PositionKey, RulesFingerprint, Shard,
    TablebaseError, TablebaseHit, hex_encode,
};

pub const LEGACY_TWO_PIECE_ASSET_NAME: &str = "two_piece_v1.jgtb";
pub const LEGACY_FORMAT_VERSION: u16 = 1;
pub const LEGACY_ENTRY_COUNT: usize = 8 * 8 * 63 * 63 * 2;

const LEGACY_MAGIC: &[u8; 8] = b"JGTB2P\0\0";
const LEGACY_HEADER_BYTES: usize = 84;
const LEGACY_ENTRY_BYTES: usize = 2;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum SideToMove {
    Blue = 0,
    Red = 1,
}

/// Collision-free key for an exact one-blue-piece versus one-red-piece signature.
pub fn two_piece_position_key(
    blue_position: u8,
    red_position: u8,
    side_to_move: SideToMove,
) -> Option<PositionKey> {
    if blue_position >= 63 || red_position >= 63 {
        return None;
    }
    let key =
        (u128::from(blue_position) * 64 + u128::from(red_position)) * 2 + side_to_move as u128;
    Some(PositionKey(key))
}

pub fn legacy_dense_index(
    blue_kind: PieceKind,
    blue_position: u8,
    red_kind: PieceKind,
    red_position: u8,
    side_to_move: SideToMove,
) -> Option<usize> {
    if blue_position >= 63 || red_position >= 63 {
        return None;
    }
    let mut value = usize::from(blue_kind.rank() - 1) * 8 + usize::from(red_kind.rank() - 1);
    value = value * 63 + usize::from(blue_position);
    value = value * 63 + usize::from(red_position);
    Some(value * 2 + side_to_move as usize)
}

/// Reader/importer for the manifest-locked Python `two_piece_v1.jgtb` asset.
#[derive(Clone, Debug)]
pub struct LegacyTwoPieceTablebase {
    entries: Vec<u16>,
    rules_fingerprint: RulesFingerprint,
    max_distance: u16,
}

impl LegacyTwoPieceTablebase {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TablebaseError> {
        Self::from_bytes_for_rules(bytes, CURRENT_RULES_FINGERPRINT)
    }

    pub fn from_bytes_for_rules(
        bytes: &[u8],
        expected_rules: RulesFingerprint,
    ) -> Result<Self, TablebaseError> {
        if bytes.len() < LEGACY_HEADER_BYTES {
            return Err(TablebaseError::InvalidFormat(
                "legacy two-piece tablebase header is truncated".into(),
            ));
        }
        if &bytes[..8] != LEGACY_MAGIC {
            return Err(TablebaseError::InvalidFormat(
                "legacy two-piece tablebase magic does not match".into(),
            ));
        }
        let version = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
        if version != LEGACY_FORMAT_VERSION {
            return Err(TablebaseError::InvalidFormat(format!(
                "unsupported legacy two-piece tablebase version {version}"
            )));
        }
        let entry_bytes = u16::from_le_bytes(bytes[10..12].try_into().unwrap());
        if usize::from(entry_bytes) != LEGACY_ENTRY_BYTES {
            return Err(TablebaseError::InvalidFormat(format!(
                "unsupported legacy tablebase entry size {entry_bytes}"
            )));
        }
        let entry_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        if entry_count != LEGACY_ENTRY_COUNT {
            return Err(TablebaseError::InvalidFormat(format!(
                "legacy tablebase has {entry_count} entries; expected {LEGACY_ENTRY_COUNT}"
            )));
        }
        let max_distance_u32 = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let max_distance = u16::try_from(max_distance_u32).map_err(|_| {
            TablebaseError::InvalidFormat(format!(
                "legacy maximum distance {max_distance_u32} is not representable"
            ))
        })?;
        if max_distance > 0x3fff {
            return Err(TablebaseError::InvalidFormat(format!(
                "legacy maximum distance {max_distance} exceeds the entry format"
            )));
        }
        let rules_fingerprint = RulesFingerprint(bytes[20..52].try_into().unwrap());
        if rules_fingerprint != expected_rules {
            return Err(TablebaseError::RulesMismatch {
                expected: expected_rules,
                actual: rules_fingerprint,
            });
        }
        let expected_payload_sha256: [u8; 32] = bytes[52..84].try_into().unwrap();
        let expected_size = LEGACY_HEADER_BYTES + LEGACY_ENTRY_COUNT * LEGACY_ENTRY_BYTES;
        if bytes.len() != expected_size {
            return Err(TablebaseError::InvalidFormat(format!(
                "legacy tablebase has {} bytes; expected {expected_size}",
                bytes.len()
            )));
        }
        let payload = &bytes[LEGACY_HEADER_BYTES..];
        let actual_payload_sha256 = crate::sha256(payload);
        if actual_payload_sha256 != expected_payload_sha256 {
            return Err(TablebaseError::ChecksumMismatch {
                what: "legacy two-piece payload".into(),
                expected: hex_encode(&expected_payload_sha256),
                actual: hex_encode(&actual_payload_sha256),
            });
        }

        let mut entries = Vec::with_capacity(LEGACY_ENTRY_COUNT);
        let mut observed_max_distance = 0;
        for encoded in payload.chunks_exact(2) {
            let value = u16::from_le_bytes(encoded.try_into().unwrap());
            if let Some(hit) = TablebaseHit::decode(value)? {
                observed_max_distance = observed_max_distance.max(hit.distance.unwrap_or(0));
            }
            entries.push(value);
        }
        if observed_max_distance != max_distance {
            return Err(TablebaseError::InvalidFormat(format!(
                "legacy maximum distance is {observed_max_distance}; header declares {max_distance}"
            )));
        }

        Ok(Self {
            entries,
            rules_fingerprint,
            max_distance,
        })
    }

    pub fn rules_fingerprint(&self) -> RulesFingerprint {
        self.rules_fingerprint
    }

    pub fn max_distance(&self) -> u16 {
        self.max_distance
    }

    pub fn probe_codes(
        &self,
        blue_kind: PieceKind,
        blue_position: u8,
        red_kind: PieceKind,
        red_position: u8,
        side_to_move: SideToMove,
    ) -> Option<TablebaseHit> {
        let index = legacy_dense_index(
            blue_kind,
            blue_position,
            red_kind,
            red_position,
            side_to_move,
        )?;
        TablebaseHit::decode(self.entries[index]).ok().flatten()
    }

    /// Convert one exact material signature to the sparse, streamable shard format.
    pub fn import_material_shard(
        &self,
        blue_kind: PieceKind,
        red_kind: PieceKind,
    ) -> Result<Shard, TablebaseError> {
        let mut records = Vec::new();
        for blue_position in 0..63 {
            for red_position in 0..63 {
                for side in [SideToMove::Blue, SideToMove::Red] {
                    if let Some(hit) =
                        self.probe_codes(blue_kind, blue_position, red_kind, red_position, side)
                    {
                        records.push((
                            two_piece_position_key(blue_position, red_position, side)
                                .expect("loop positions are valid"),
                            hit,
                        ));
                    }
                }
            }
        }
        Shard::from_records(
            self.rules_fingerprint,
            MaterialSignature::one_vs_one(blue_kind, red_kind),
            true,
            records,
        )
    }

    /// Convert all 64 one-versus-one material signatures deterministically.
    pub fn import_all_material_shards(&self) -> Result<Vec<Shard>, TablebaseError> {
        let mut shards = Vec::with_capacity(64);
        for blue_kind in PieceKind::ALL {
            for red_kind in PieceKind::ALL {
                shards.push(self.import_material_shard(blue_kind, red_kind)?);
            }
        }
        Ok(shards)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::{TablebaseProbe, TablebaseSet};

    fn legacy_asset() -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../python/jungle/ai/data/two_piece_v1.jgtb");
        fs::read(path).expect("repository includes the manifest-locked legacy tablebase")
    }

    #[test]
    fn validates_real_legacy_asset() {
        let bytes = legacy_asset();
        assert_eq!(
            hex_encode(&crate::sha256(&bytes)),
            "a32b620d22d1735ef4f2a13b8d1388c07ad45151060f638c2434db988f97ece5"
        );
        let tablebase = LegacyTwoPieceTablebase::from_bytes(&bytes).unwrap();
        assert_eq!(tablebase.rules_fingerprint(), CURRENT_RULES_FINGERPRINT);
        assert_eq!(tablebase.max_distance(), 34);
    }

    #[test]
    fn legacy_import_round_trips_one_material_signature() {
        let tablebase = LegacyTwoPieceTablebase::from_bytes(&legacy_asset()).unwrap();
        let shard = tablebase
            .import_material_shard(PieceKind::Rat, PieceKind::Elephant)
            .unwrap();
        let signature = MaterialSignature::one_vs_one(PieceKind::Rat, PieceKind::Elephant);
        let mut set = TablebaseSet::new(CURRENT_RULES_FINGERPRINT);
        set.insert(shard).unwrap();
        for blue_position in 0..63 {
            for red_position in 0..63 {
                for side in [SideToMove::Blue, SideToMove::Red] {
                    let expected = tablebase.probe_codes(
                        PieceKind::Rat,
                        blue_position,
                        PieceKind::Elephant,
                        red_position,
                        side,
                    );
                    let key = two_piece_position_key(blue_position, red_position, side).unwrap();
                    assert_eq!(set.probe_key(signature, key).unwrap(), expected);
                }
            }
        }
    }

    #[test]
    fn legacy_corruption_is_rejected() {
        let mut bytes = legacy_asset();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x40;
        assert!(matches!(
            LegacyTwoPieceTablebase::from_bytes(&bytes),
            Err(TablebaseError::ChecksumMismatch { .. })
        ));
    }
}
