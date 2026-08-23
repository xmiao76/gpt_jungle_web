use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Seek, SeekFrom, Write},
};

use sha2::{Digest, Sha256};

use crate::{
    MaterialSignature, PositionKey, RulesFingerprint, TablebaseError, TablebaseHit, hex_encode,
};

pub const SHARD_FORMAT_VERSION: u16 = 1;
pub const SHARD_HEADER_BYTES: usize = 140;
const SHARD_MAGIC: &[u8; 8] = b"JGTBSH\0\0";
const RECORD_BYTES: usize = 18;
const COMPLETE_MATERIAL_FLAG: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShardHeader {
    pub rules_fingerprint: RulesFingerprint,
    pub material_signature: MaterialSignature,
    pub complete_material: bool,
    pub entry_count: u64,
    pub max_distance: u16,
    pub min_key: PositionKey,
    pub max_key: PositionKey,
    pub payload_sha256: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShardSummary {
    pub header: ShardHeader,
    pub byte_size: u64,
    pub content_sha256: [u8; 32],
}

/// A seekable, constant-memory writer for sorted tablebase records.
///
/// Generators may enumerate arbitrarily large state spaces and rotate to a new
/// writer before the hosting-size ceiling. Records must arrive in strictly
/// increasing canonical-key order.
pub struct StreamingShardWriter<W: Read + Write + Seek> {
    output: W,
    rules_fingerprint: RulesFingerprint,
    material_signature: MaterialSignature,
    complete_material: bool,
    payload_hasher: Sha256,
    entry_count: u64,
    max_distance: u16,
    min_key: Option<PositionKey>,
    last_key: Option<PositionKey>,
}

impl<W: Read + Write + Seek> StreamingShardWriter<W> {
    pub fn new(
        mut output: W,
        rules_fingerprint: RulesFingerprint,
        material_signature: MaterialSignature,
        complete_material: bool,
    ) -> Result<Self, TablebaseError> {
        material_signature.validate()?;
        let existing_bytes = output.seek(SeekFrom::End(0))?;
        if existing_bytes != 0 {
            return Err(TablebaseError::InvalidFormat(format!(
                "streaming shard output must be empty, found {existing_bytes} existing bytes"
            )));
        }
        output.seek(SeekFrom::Start(0))?;
        output.write_all(&[0; SHARD_HEADER_BYTES])?;
        Ok(Self {
            output,
            rules_fingerprint,
            material_signature,
            complete_material,
            payload_hasher: Sha256::new(),
            entry_count: 0,
            max_distance: 0,
            min_key: None,
            last_key: None,
        })
    }

    pub fn push(&mut self, key: PositionKey, hit: TablebaseHit) -> Result<(), TablebaseError> {
        if self.last_key.is_some_and(|last| key <= last) {
            return Err(TablebaseError::InvalidFormat(format!(
                "shard records must be strictly increasing; {key} follows {}",
                self.last_key.expect("checked above")
            )));
        }
        let encoded = hit.encode()?;
        let mut record = [0; RECORD_BYTES];
        record[..16].copy_from_slice(&key.get().to_le_bytes());
        record[16..].copy_from_slice(&encoded.to_le_bytes());
        self.output.write_all(&record)?;
        self.payload_hasher.update(record);
        self.entry_count = self
            .entry_count
            .checked_add(1)
            .ok_or_else(|| TablebaseError::InvalidFormat("shard entry count overflow".into()))?;
        self.max_distance = self.max_distance.max(hit.distance.unwrap_or(0));
        self.min_key.get_or_insert(key);
        self.last_key = Some(key);
        Ok(())
    }

    pub fn entry_count(&self) -> u64 {
        self.entry_count
    }

    pub fn projected_byte_size(&self, additional_records: u64) -> Option<u64> {
        self.entry_count
            .checked_add(additional_records)?
            .checked_mul(RECORD_BYTES as u64)?
            .checked_add(SHARD_HEADER_BYTES as u64)
    }

    pub fn finish(mut self) -> Result<(W, ShardSummary), TablebaseError> {
        let min_key = self.min_key.ok_or_else(|| {
            TablebaseError::InvalidFormat("a tablebase shard cannot be empty".into())
        })?;
        let max_key = self.last_key.expect("a nonempty writer has a last key");
        let payload_sha256: [u8; 32] = self.payload_hasher.finalize().into();
        let header = ShardHeader {
            rules_fingerprint: self.rules_fingerprint,
            material_signature: self.material_signature,
            complete_material: self.complete_material,
            entry_count: self.entry_count,
            max_distance: self.max_distance,
            min_key,
            max_key,
            payload_sha256,
        };
        let header_bytes = encode_header(header);
        self.output.seek(SeekFrom::Start(0))?;
        self.output.write_all(&header_bytes)?;
        self.output.flush()?;
        let byte_size = (SHARD_HEADER_BYTES as u64)
            .checked_add(self.entry_count * RECORD_BYTES as u64)
            .expect("entry count was bounded by addressable output");

        // Hash the completed stream without requiring its contents in memory.
        self.output.seek(SeekFrom::Start(0))?;
        let mut content_hasher = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        let mut remaining = byte_size;
        while remaining != 0 {
            let wanted = usize::try_from(remaining.min(buffer.len() as u64)).unwrap();
            let read = self.output.read(&mut buffer[..wanted])?;
            if read == 0 {
                return Err(TablebaseError::InvalidFormat(
                    "completed shard stream ended before its declared size".into(),
                ));
            }
            content_hasher.update(&buffer[..read]);
            remaining -= read as u64;
        }
        let summary = ShardSummary {
            header,
            byte_size,
            content_sha256: content_hasher.finalize().into(),
        };
        self.output.seek(SeekFrom::End(0))?;
        Ok((self.output, summary))
    }
}

/// An immutable validated shard backed by its exact serialized bytes.
#[derive(Clone, Debug)]
pub struct Shard {
    bytes: Vec<u8>,
    header: ShardHeader,
}

impl Shard {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, TablebaseError> {
        let header = decode_header(&bytes)?;
        validate_payload(&bytes, header)?;
        Ok(Self { bytes, header })
    }

    pub fn from_records<I>(
        rules_fingerprint: RulesFingerprint,
        material_signature: MaterialSignature,
        complete_material: bool,
        records: I,
    ) -> Result<Self, TablebaseError>
    where
        I: IntoIterator<Item = (PositionKey, TablebaseHit)>,
    {
        let cursor = Cursor::new(Vec::new());
        let mut writer = StreamingShardWriter::new(
            cursor,
            rules_fingerprint,
            material_signature,
            complete_material,
        )?;
        for (key, hit) in records {
            writer.push(key, hit)?;
        }
        let (cursor, _) = writer.finish()?;
        Self::from_bytes(cursor.into_inner())
    }

    pub fn header(&self) -> ShardHeader {
        self.header
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn content_sha256(&self) -> [u8; 32] {
        crate::sha256(&self.bytes)
    }

    pub fn probe(&self, key: PositionKey) -> Option<TablebaseHit> {
        if key < self.header.min_key || key > self.header.max_key {
            return None;
        }
        let mut low = 0_u64;
        let mut high = self.header.entry_count;
        while low < high {
            let middle = low + (high - low) / 2;
            let offset = SHARD_HEADER_BYTES + usize::try_from(middle).ok()? * RECORD_BYTES;
            let found = PositionKey(u128::from_le_bytes(
                self.bytes[offset..offset + 16].try_into().ok()?,
            ));
            match found.cmp(&key) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => {
                    let encoded =
                        u16::from_le_bytes(self.bytes[offset + 16..offset + 18].try_into().ok()?);
                    return TablebaseHit::decode(encoded).ok().flatten();
                }
            }
        }
        None
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (PositionKey, TablebaseHit)> + '_ {
        self.bytes[SHARD_HEADER_BYTES..]
            .chunks_exact(RECORD_BYTES)
            .map(|record| {
                let key = PositionKey(u128::from_le_bytes(record[..16].try_into().unwrap()));
                let encoded = u16::from_le_bytes(record[16..].try_into().unwrap());
                let hit = TablebaseHit::decode(encoded)
                    .expect("shard entries validated at construction")
                    .expect("sparse shard records cannot contain unknown entries");
                (key, hit)
            })
    }
}

pub trait TablebaseProbe {
    fn probe_key(
        &self,
        material: MaterialSignature,
        key: PositionKey,
    ) -> Result<Option<TablebaseHit>, TablebaseError>;
}

/// Loaded shards grouped by exact material signature.
#[derive(Debug)]
pub struct TablebaseSet {
    rules_fingerprint: RulesFingerprint,
    shards: BTreeMap<MaterialSignature, Vec<Shard>>,
}

impl TablebaseSet {
    pub fn new(rules_fingerprint: RulesFingerprint) -> Self {
        Self {
            rules_fingerprint,
            shards: BTreeMap::new(),
        }
    }

    pub fn rules_fingerprint(&self) -> RulesFingerprint {
        self.rules_fingerprint
    }

    pub fn insert(&mut self, shard: Shard) -> Result<(), TablebaseError> {
        let header = shard.header();
        if header.rules_fingerprint != self.rules_fingerprint {
            return Err(TablebaseError::RulesMismatch {
                expected: self.rules_fingerprint,
                actual: header.rules_fingerprint,
            });
        }
        let shards = self.shards.entry(header.material_signature).or_default();
        if shards.iter().any(|existing| {
            let other = existing.header();
            header.min_key <= other.max_key && other.min_key <= header.max_key
        }) {
            return Err(TablebaseError::InvalidFormat(format!(
                "overlapping shard key ranges for material {}",
                header.material_signature
            )));
        }
        shards.push(shard);
        shards.sort_by_key(|candidate| candidate.header().min_key);
        Ok(())
    }

    pub fn material_count(&self) -> usize {
        self.shards.len()
    }

    pub fn shard_count(&self) -> usize {
        self.shards.values().map(Vec::len).sum()
    }

    pub fn probe_position(
        &self,
        position: &jungle_core::Position,
    ) -> Result<Option<TablebaseHit>, TablebaseError> {
        self.probe_key(
            MaterialSignature::from_position(position),
            PositionKey::from_position(position),
        )
    }
}

impl TablebaseProbe for TablebaseSet {
    fn probe_key(
        &self,
        material: MaterialSignature,
        key: PositionKey,
    ) -> Result<Option<TablebaseHit>, TablebaseError> {
        let Some(shards) = self.shards.get(&material) else {
            return Ok(None);
        };
        let candidate = shards.partition_point(|shard| shard.header().max_key < key);
        Ok(shards.get(candidate).and_then(|shard| shard.probe(key)))
    }
}

fn encode_header(header: ShardHeader) -> [u8; SHARD_HEADER_BYTES] {
    let mut bytes = [0; SHARD_HEADER_BYTES];
    bytes[..8].copy_from_slice(SHARD_MAGIC);
    bytes[8..10].copy_from_slice(&SHARD_FORMAT_VERSION.to_le_bytes());
    bytes[10..12].copy_from_slice(&(SHARD_HEADER_BYTES as u16).to_le_bytes());
    let flags = u32::from(header.complete_material) * COMPLETE_MATERIAL_FLAG;
    bytes[12..16].copy_from_slice(&flags.to_le_bytes());
    bytes[16..48].copy_from_slice(header.rules_fingerprint.as_bytes());
    bytes[48..64].copy_from_slice(&header.material_signature.as_bytes());
    bytes[64..72].copy_from_slice(&header.entry_count.to_le_bytes());
    bytes[72..74].copy_from_slice(&header.max_distance.to_le_bytes());
    // 74..76 is reserved and remains zero.
    bytes[76..92].copy_from_slice(&header.min_key.get().to_le_bytes());
    bytes[92..108].copy_from_slice(&header.max_key.get().to_le_bytes());
    bytes[108..140].copy_from_slice(&header.payload_sha256);
    bytes
}

fn decode_header(bytes: &[u8]) -> Result<ShardHeader, TablebaseError> {
    if bytes.len() < SHARD_HEADER_BYTES {
        return Err(TablebaseError::InvalidFormat(format!(
            "tablebase shard header is truncated: {} bytes",
            bytes.len()
        )));
    }
    if &bytes[..8] != SHARD_MAGIC {
        return Err(TablebaseError::InvalidFormat(
            "tablebase shard magic does not match".into(),
        ));
    }
    let version = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
    if version != SHARD_FORMAT_VERSION {
        return Err(TablebaseError::InvalidFormat(format!(
            "unsupported tablebase shard version {version}"
        )));
    }
    let header_bytes = u16::from_le_bytes(bytes[10..12].try_into().unwrap());
    if usize::from(header_bytes) != SHARD_HEADER_BYTES {
        return Err(TablebaseError::InvalidFormat(format!(
            "unsupported tablebase shard header length {header_bytes}"
        )));
    }
    let flags = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    if flags & !COMPLETE_MATERIAL_FLAG != 0 {
        return Err(TablebaseError::InvalidFormat(format!(
            "unsupported tablebase shard flags {flags:#x}"
        )));
    }
    if bytes[74..76] != [0, 0] {
        return Err(TablebaseError::InvalidFormat(
            "tablebase shard reserved header bytes are nonzero".into(),
        ));
    }
    let rules_fingerprint = RulesFingerprint(bytes[16..48].try_into().unwrap());
    let material_signature = MaterialSignature::from_bytes(bytes[48..64].try_into().unwrap())?;
    let entry_count = u64::from_le_bytes(bytes[64..72].try_into().unwrap());
    if entry_count == 0 {
        return Err(TablebaseError::InvalidFormat(
            "a tablebase shard cannot be empty".into(),
        ));
    }
    let max_distance = u16::from_le_bytes(bytes[72..74].try_into().unwrap());
    if max_distance > 0x3fff {
        return Err(TablebaseError::InvalidFormat(format!(
            "tablebase maximum distance {max_distance} is not representable"
        )));
    }
    let min_key = PositionKey(u128::from_le_bytes(bytes[76..92].try_into().unwrap()));
    let max_key = PositionKey(u128::from_le_bytes(bytes[92..108].try_into().unwrap()));
    if min_key > max_key {
        return Err(TablebaseError::InvalidFormat(
            "tablebase shard minimum key exceeds maximum key".into(),
        ));
    }
    let payload_sha256 = bytes[108..140].try_into().unwrap();
    Ok(ShardHeader {
        rules_fingerprint,
        material_signature,
        complete_material: flags & COMPLETE_MATERIAL_FLAG != 0,
        entry_count,
        max_distance,
        min_key,
        max_key,
        payload_sha256,
    })
}

fn validate_payload(bytes: &[u8], header: ShardHeader) -> Result<(), TablebaseError> {
    let payload_bytes = header
        .entry_count
        .checked_mul(RECORD_BYTES as u64)
        .and_then(|length| length.checked_add(SHARD_HEADER_BYTES as u64))
        .ok_or_else(|| TablebaseError::InvalidFormat("tablebase shard size overflow".into()))?;
    let expected_size = usize::try_from(payload_bytes).map_err(|_| {
        TablebaseError::InvalidFormat("tablebase shard does not fit address space".into())
    })?;
    if bytes.len() != expected_size {
        return Err(TablebaseError::InvalidFormat(format!(
            "tablebase shard has {} bytes; expected {expected_size}",
            bytes.len()
        )));
    }
    let payload = &bytes[SHARD_HEADER_BYTES..];
    let actual_hash = crate::sha256(payload);
    if actual_hash != header.payload_sha256 {
        return Err(TablebaseError::ChecksumMismatch {
            what: "tablebase shard payload".into(),
            expected: hex_encode(&header.payload_sha256),
            actual: hex_encode(&actual_hash),
        });
    }

    let mut last_key = None;
    let mut actual_max_distance = 0;
    for record in payload.chunks_exact(RECORD_BYTES) {
        let key = PositionKey(u128::from_le_bytes(record[..16].try_into().unwrap()));
        if last_key.is_some_and(|last| key <= last) {
            return Err(TablebaseError::InvalidFormat(
                "tablebase shard records are not strictly increasing".into(),
            ));
        }
        let encoded = u16::from_le_bytes(record[16..].try_into().unwrap());
        let hit = TablebaseHit::decode(encoded)?.ok_or_else(|| {
            TablebaseError::InvalidFormat("sparse tablebase shard contains an unknown entry".into())
        })?;
        actual_max_distance = actual_max_distance.max(hit.distance.unwrap_or(0));
        last_key = Some(key);
    }
    if last_key != Some(header.max_key) {
        return Err(TablebaseError::InvalidFormat(
            "tablebase shard maximum key does not match payload".into(),
        ));
    }
    let first_key = PositionKey(u128::from_le_bytes(payload[..16].try_into().unwrap()));
    if first_key != header.min_key {
        return Err(TablebaseError::InvalidFormat(
            "tablebase shard minimum key does not match payload".into(),
        ));
    }
    if actual_max_distance != header.max_distance {
        return Err(TablebaseError::InvalidFormat(format!(
            "tablebase shard maximum distance is {}, expected {}",
            actual_max_distance, header.max_distance
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CURRENT_RULES_FINGERPRINT, PieceKind, Wdl};

    fn sample() -> Shard {
        Shard::from_records(
            CURRENT_RULES_FINGERPRINT,
            MaterialSignature::one_vs_one(PieceKind::Cat, PieceKind::Rat),
            true,
            [
                (PositionKey(2), TablebaseHit::loss(7)),
                (PositionKey(9), TablebaseHit::draw()),
                (PositionKey(40), TablebaseHit::win(3)),
            ],
        )
        .unwrap()
    }

    #[test]
    fn shard_round_trip_and_probe() {
        let shard = sample();
        let loaded = Shard::from_bytes(shard.as_bytes().to_vec()).unwrap();
        assert_eq!(loaded.header().entry_count, 3);
        assert_eq!(loaded.header().max_distance, 7);
        assert_eq!(loaded.probe(PositionKey(9)).unwrap().wdl, Wdl::Draw);
        assert_eq!(loaded.probe(PositionKey(40)), Some(TablebaseHit::win(3)));
        assert_eq!(loaded.probe(PositionKey(10)), None);
        assert_eq!(loaded.iter().collect::<Vec<_>>().len(), 3);
    }

    #[test]
    fn detects_payload_corruption() {
        let shard = sample();
        let mut bytes = shard.as_bytes().to_vec();
        *bytes.last_mut().unwrap() ^= 1;
        let error = Shard::from_bytes(bytes).unwrap_err();
        assert!(matches!(error, TablebaseError::ChecksumMismatch { .. }));
    }

    #[test]
    fn rejects_unsorted_records_at_generation_time() {
        let result = Shard::from_records(
            CURRENT_RULES_FINGERPRINT,
            MaterialSignature::one_vs_one(PieceKind::Rat, PieceKind::Rat),
            false,
            [
                (PositionKey(2), TablebaseHit::draw()),
                (PositionKey(1), TablebaseHit::draw()),
            ],
        );
        assert!(result.is_err());
    }

    #[test]
    fn streaming_writer_rejects_nonempty_output() {
        let result = StreamingShardWriter::new(
            Cursor::new(vec![1]),
            CURRENT_RULES_FINGERPRINT,
            MaterialSignature::one_vs_one(PieceKind::Rat, PieceKind::Rat),
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn set_handles_non_overlapping_chunks() {
        let signature = MaterialSignature::one_vs_one(PieceKind::Rat, PieceKind::Rat);
        let first = Shard::from_records(
            CURRENT_RULES_FINGERPRINT,
            signature,
            false,
            [(PositionKey(1), TablebaseHit::win(1))],
        )
        .unwrap();
        let second = Shard::from_records(
            CURRENT_RULES_FINGERPRINT,
            signature,
            false,
            [(PositionKey(5), TablebaseHit::loss(2))],
        )
        .unwrap();
        let mut set = TablebaseSet::new(CURRENT_RULES_FINGERPRINT);
        set.insert(second).unwrap();
        set.insert(first).unwrap();
        assert_eq!(
            set.probe_key(signature, PositionKey(5)).unwrap(),
            Some(TablebaseHit::loss(2))
        );
        assert_eq!(set.probe_key(signature, PositionKey(3)).unwrap(), None);
    }
}
