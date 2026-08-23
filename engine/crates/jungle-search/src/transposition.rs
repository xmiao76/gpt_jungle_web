//! A compact four-way bucketed transposition table.
//!
//! Search history can change the value assigned to a cycle.  Entries therefore
//! carry an explicit `history_safe` bit.  The search may still use an unsafe or
//! old-generation move for ordering, but it must never use its score as a bound.

use std::mem::size_of;

pub const TT_WAYS: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Bound {
    Exact = 1,
    Lower = 2,
    Upper = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Entry {
    pub key: u64,
    pub score: i32,
    pub best_move: u32,
    pub depth: i16,
    pub bound: Bound,
    pub generation: u8,
    pub history_safe: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Slot {
    key: u64,
    score: i32,
    best_move: u32,
    depth: i16,
    generation: u8,
    bound: u8,
    flags: u8,
}

impl Slot {
    const OCCUPIED: u8 = 1;
    const HISTORY_SAFE: u8 = 2;

    #[inline]
    fn occupied(self) -> bool {
        self.flags & Self::OCCUPIED != 0
    }

    #[inline]
    fn history_safe(self) -> bool {
        self.flags & Self::HISTORY_SAFE != 0
    }

    #[inline]
    fn as_entry(self) -> Option<Entry> {
        if !self.occupied() {
            return None;
        }
        let bound = match self.bound {
            1 => Bound::Exact,
            2 => Bound::Lower,
            3 => Bound::Upper,
            _ => return None,
        };
        Some(Entry {
            key: self.key,
            score: self.score,
            best_move: self.best_move,
            depth: self.depth,
            bound,
            generation: self.generation,
            history_safe: self.history_safe(),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TableStats {
    pub probes: u64,
    pub hits: u64,
    pub stores: u64,
    pub collisions: u64,
    pub replacements: u64,
}

#[derive(Debug)]
pub struct TranspositionTable {
    buckets: Box<[[Slot; TT_WAYS]]>,
    mask: usize,
    generation: u8,
    stats: TableStats,
}

impl TranspositionTable {
    /// Construct a table no larger than `bytes`, rounded down to a power-of-two
    /// bucket count.  At least one bucket is always allocated.
    pub fn with_memory_budget(bytes: usize) -> Self {
        let bucket_bytes = size_of::<[Slot; TT_WAYS]>().max(1);
        let requested = (bytes / bucket_bytes).max(1);
        let bucket_count = floor_power_of_two(requested);
        Self {
            buckets: vec![[Slot::default(); TT_WAYS]; bucket_count].into_boxed_slice(),
            mask: bucket_count - 1,
            generation: 1,
            stats: TableStats::default(),
        }
    }

    pub fn entry_capacity(&self) -> usize {
        self.buckets.len() * TT_WAYS
    }

    pub fn memory_bytes(&self) -> usize {
        self.buckets.len() * size_of::<[Slot; TT_WAYS]>()
    }

    pub fn generation(&self) -> u8 {
        self.generation
    }

    pub fn stats(&self) -> TableStats {
        self.stats
    }

    pub fn clear_stats(&mut self) {
        self.stats = TableStats::default();
    }

    pub fn clear(&mut self) {
        self.buckets.fill([Slot::default(); TT_WAYS]);
        self.generation = 1;
        self.clear_stats();
    }

    pub fn new_search(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            // Generation zero is reserved for never-written slots.  Clearing on
            // wrap prevents a 256-search-old entry looking current again.
            self.buckets.fill([Slot::default(); TT_WAYS]);
            self.generation = 1;
        }
        self.clear_stats();
    }

    #[inline]
    pub fn probe(&mut self, key: u64) -> Option<Entry> {
        self.stats.probes += 1;
        let bucket = &self.buckets[index_for(key, self.mask)];
        for slot in bucket {
            if slot.occupied() && slot.key == key {
                self.stats.hits += 1;
                return slot.as_entry();
            }
        }
        if bucket.iter().any(|slot| slot.occupied()) {
            self.stats.collisions += 1;
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    pub fn store(
        &mut self,
        key: u64,
        depth: i16,
        score: i32,
        bound: Bound,
        best_move: u32,
        history_safe: bool,
    ) {
        // A history-dependent value is deliberately not admitted.  Keeping the
        // argument in this API makes accidental unsafe storage visible at the
        // call site and protects future alternate search implementations.
        if !history_safe {
            return;
        }

        let generation = self.generation;
        let bucket = &mut self.buckets[index_for(key, self.mask)];

        let same = bucket
            .iter()
            .position(|slot| slot.occupied() && slot.key == key);
        let target = if let Some(index) = same {
            let old = bucket[index];
            let old_bound = old.as_entry().map(|entry| entry.bound);
            let should_replace = old.generation != generation
                || depth > old.depth
                || (depth == old.depth && bound == Bound::Exact && old_bound != Some(Bound::Exact))
                || (best_move != 0 && old.best_move == 0);
            if !should_replace {
                return;
            }
            index
        } else if let Some(index) = bucket.iter().position(|slot| !slot.occupied()) {
            index
        } else {
            self.stats.replacements += 1;
            replacement_index(bucket, generation)
        };

        bucket[target] = Slot {
            key,
            score,
            best_move,
            depth,
            generation,
            bound: bound as u8,
            flags: Slot::OCCUPIED | Slot::HISTORY_SAFE,
        };
        self.stats.stores += 1;
    }

    /// Approximate occupancy of the current generation in per-mille.
    pub fn hashfull_per_mille(&self) -> u16 {
        let sample_buckets = self.buckets.len().min(1_000);
        let sample_entries = sample_buckets * TT_WAYS;
        if sample_entries == 0 {
            return 0;
        }
        let occupied = self.buckets[..sample_buckets]
            .iter()
            .flatten()
            .filter(|slot| slot.occupied() && slot.generation == self.generation)
            .count();
        ((occupied * 1_000) / sample_entries) as u16
    }
}

#[inline]
fn floor_power_of_two(value: usize) -> usize {
    1usize << (usize::BITS - 1 - value.leading_zeros())
}

#[inline]
fn index_for(key: u64, mask: usize) -> usize {
    // Fold high bits before masking so small test tables do not depend only on
    // the low bits of a Zobrist key.
    ((key ^ (key >> 32)) as usize) & mask
}

fn replacement_index(bucket: &[Slot; TT_WAYS], generation: u8) -> usize {
    bucket
        .iter()
        .enumerate()
        .min_by_key(|(_, slot)| {
            let freshness = u32::from(slot.generation == generation) * 32_768;
            let exact = u32::from(slot.bound == Bound::Exact as u8) * 512;
            let depth = u32::from(slot.depth.max(0) as u16) * 16;
            freshness + exact + depth
        })
        .map_or(0, |(index, _)| index)
}

/// Normalize mate scores before storage so the distance remains correct when
/// the same position is reached at another ply.
#[inline]
pub fn score_to_table(score: i32, ply: u16, mate_threshold: i32) -> i32 {
    if score >= mate_threshold {
        score.saturating_add(i32::from(ply))
    } else if score <= -mate_threshold {
        score.saturating_sub(i32::from(ply))
    } else {
        score
    }
}

/// Reverse [`score_to_table`].
#[inline]
pub fn score_from_table(score: i32, ply: u16, mate_threshold: i32) -> i32 {
    if score >= mate_threshold {
        score.saturating_sub(i32::from(ply))
    } else if score <= -mate_threshold {
        score.saturating_add(i32::from(ply))
    } else {
        score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_budget_down_and_round_trips_entries() {
        let mut table = TranspositionTable::with_memory_budget(10_000);
        assert!(table.entry_capacity().is_power_of_two());
        table.store(0x1234, 7, 91, Bound::Exact, 42, true);
        let entry = table.probe(0x1234).expect("entry");
        assert_eq!(
            (entry.depth, entry.score, entry.bound, entry.best_move),
            (7, 91, Bound::Exact, 42)
        );
        assert!(entry.history_safe);
        assert_eq!(entry.generation, table.generation());
    }

    #[test]
    fn rejects_history_dependent_entries() {
        let mut table = TranspositionTable::with_memory_budget(1);
        table.store(9, 4, 12, Bound::Lower, 7, false);
        assert_eq!(table.probe(9), None);
        assert_eq!(table.stats().stores, 0);
    }

    #[test]
    fn bucket_keeps_deep_fresh_entries_over_shallow_collision() {
        let mut table = TranspositionTable::with_memory_budget(1);
        for key in 1..=4 {
            table.store(key, 12, key as i32, Bound::Exact, key as u32, true);
        }
        table.store(5, 1, 5, Bound::Upper, 5, true);
        let survivors = (1..=4).filter(|key| table.probe(*key).is_some()).count();
        assert_eq!(survivors, 3);
        assert!(table.probe(5).is_some());
    }

    #[test]
    fn old_generation_is_replaced_before_equally_deep_fresh_entry() {
        let mut table = TranspositionTable::with_memory_budget(1);
        for key in 1..=4 {
            table.store(key, 20, 0, Bound::Exact, key as u32, true);
        }
        table.new_search();
        table.store(1, 20, 0, Bound::Exact, 1, true);
        table.store(9, 2, 0, Bound::Upper, 9, true);
        assert!(table.probe(1).is_some());
        assert!(table.probe(9).is_some());
    }

    #[test]
    fn same_key_preserves_deeper_fresh_nonexact_entry() {
        let mut table = TranspositionTable::with_memory_budget(1);
        table.store(1, 10, 70, Bound::Lower, 8, true);
        table.store(1, 4, 20, Bound::Upper, 9, true);
        let entry = table.probe(1).expect("entry");
        assert_eq!((entry.depth, entry.score, entry.best_move), (10, 70, 8));
    }

    #[test]
    fn mate_scores_are_ply_independent_in_storage() {
        const THRESHOLD: i32 = 99_000;
        let win = score_to_table(99_995, 5, THRESHOLD);
        let loss = score_to_table(-99_995, 5, THRESHOLD);
        assert_eq!(score_from_table(win, 2, THRESHOLD), 99_998);
        assert_eq!(score_from_table(loss, 2, THRESHOLD), -99_998);
        assert_eq!(score_from_table(123, 40, THRESHOLD), 123);
    }

    #[test]
    fn generation_wrap_clears_old_entries() {
        let mut table = TranspositionTable::with_memory_budget(1);
        table.store(99, 1, 1, Bound::Exact, 1, true);
        for _ in 0..255 {
            table.new_search();
        }
        assert!(table.probe(99).is_none());
        assert_eq!(table.generation(), 1);
    }
}
