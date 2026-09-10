use std::collections::HashMap;
use std::hash::RandomState;
use std::mem::size_of;

use thiserror::Error;

use super::canonical::{ArenaError, CanonicalArena, EqualityScratch};
use super::journal::{JournalError, SemanticJournal, SemanticUndo};
use super::limits::ResourceError;
use super::plan::ConstraintId;
use super::scope::FrameId;
use super::value::CanonicalId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HistoryKey {
    pub frame: FrameId,
    pub constraint: ConstraintId,
}

#[derive(Debug)]
pub struct CanonicalSet<S = RandomState> {
    hash_builder: S,
    buckets: HashMap<u64, Vec<CanonicalId>>,
    len: usize,
}

#[derive(Debug, Default)]
pub struct HistoryStore {
    histories: HashMap<HistoryKey, CanonicalSet>,
    item_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InsertResult {
    Inserted,
    Duplicate,
}

impl Default for CanonicalSet<RandomState> {
    fn default() -> Self {
        Self {
            hash_builder: RandomState::new(),
            buckets: HashMap::new(),
            len: 0,
        }
    }
}

impl<S: std::hash::BuildHasher> CanonicalSet<S> {
    pub fn with_hash_builder(hash_builder: S) -> Self {
        Self {
            hash_builder,
            buckets: HashMap::new(),
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(crate) fn retained_bytes(&self) -> Result<usize, ResourceError> {
        let bucket_storage = self
            .buckets
            .capacity()
            .checked_mul(size_of::<(u64, Vec<CanonicalId>)>())
            .ok_or(ResourceError::ArithmeticOverflow {
                context: "retained history buckets",
            })?;
        self.buckets
            .values()
            .try_fold(bucket_storage, |total, bucket| {
                let values = bucket
                    .capacity()
                    .checked_mul(size_of::<CanonicalId>())
                    .ok_or(ResourceError::ArithmeticOverflow {
                        context: "retained history values",
                    })?;
                total
                    .checked_add(values)
                    .ok_or(ResourceError::ArithmeticOverflow {
                        context: "retained history bytes",
                    })
            })
    }

    pub fn contains(&self, arena: &CanonicalArena, value: CanonicalId) -> Result<bool, ArenaError> {
        let fingerprint = arena.fingerprint(value, &self.hash_builder)?;
        let Some(bucket) = self.buckets.get(&fingerprint) else {
            return Ok(false);
        };
        for candidate in bucket {
            if arena.equal(*candidate, value)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn contains_across_with_scratch(
        &self,
        stored_arena: &CanonicalArena,
        candidate_arena: &CanonicalArena,
        value: CanonicalId,
        scratch: &mut EqualityScratch,
    ) -> Result<bool, ArenaError> {
        let fingerprint = candidate_arena.fingerprint(value, &self.hash_builder)?;
        let Some(bucket) = self.buckets.get(&fingerprint) else {
            return Ok(false);
        };
        for candidate in bucket {
            if stored_arena.equal_across_with_scratch(
                *candidate,
                candidate_arena,
                value,
                scratch,
            )? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn contains_across(
        &self,
        stored_arena: &CanonicalArena,
        candidate_arena: &CanonicalArena,
        value: CanonicalId,
    ) -> Result<bool, ArenaError> {
        let mut scratch = EqualityScratch::default();
        self.contains_across_with_scratch(stored_arena, candidate_arena, value, &mut scratch)
    }

    pub fn insert(
        &mut self,
        arena: &CanonicalArena,
        value: CanonicalId,
    ) -> Result<InsertResult, HistoryError> {
        self.insert_limited(arena, value, usize::MAX)
    }

    pub fn insert_limited(
        &mut self,
        arena: &CanonicalArena,
        value: CanonicalId,
        max_bucket_entries: usize,
    ) -> Result<InsertResult, HistoryError> {
        let fingerprint = arena.fingerprint(value, &self.hash_builder)?;
        let next_len = self
            .len
            .checked_add(1)
            .ok_or(ResourceError::ArithmeticOverflow {
                context: "history length",
            })?;
        if let Some(bucket) = self.buckets.get_mut(&fingerprint) {
            for candidate in bucket.iter() {
                if arena.equal(*candidate, value)? {
                    return Ok(InsertResult::Duplicate);
                }
            }
            let requested =
                bucket
                    .len()
                    .checked_add(1)
                    .ok_or(ResourceError::ArithmeticOverflow {
                        context: "history bucket entries",
                    })?;
            if requested > max_bucket_entries {
                return Err(ResourceError::LimitExceeded {
                    limit_name: "max_history_bucket_entries",
                    limit_value: max_bucket_entries,
                    requested,
                }
                .into());
            }
            bucket
                .try_reserve_exact(1)
                .map_err(|_| ResourceError::Allocation {
                    context: "history bucket",
                    requested: 1,
                })?;
            bucket.push(value);
        } else {
            if max_bucket_entries == 0 {
                return Err(ResourceError::LimitExceeded {
                    limit_name: "max_history_bucket_entries",
                    limit_value: 0,
                    requested: 1,
                }
                .into());
            }
            self.buckets
                .try_reserve(1)
                .map_err(|_| ResourceError::Allocation {
                    context: "history buckets",
                    requested: 1,
                })?;
            let mut bucket = Vec::new();
            bucket
                .try_reserve_exact(1)
                .map_err(|_| ResourceError::Allocation {
                    context: "history bucket",
                    requested: 1,
                })?;
            bucket.push(value);
            self.buckets.insert(fingerprint, bucket);
        }
        self.len = next_len;
        Ok(InsertResult::Inserted)
    }
}

impl HistoryStore {
    #[cfg(debug_assertions)]
    pub(crate) fn logical_snapshot(&self) -> Vec<(HistoryKey, Vec<CanonicalId>)> {
        let mut result: Vec<_> = self
            .histories
            .iter()
            .map(|(key, history)| {
                let mut values: Vec<_> = history
                    .buckets
                    .values()
                    .flat_map(|bucket| bucket.iter().copied())
                    .collect();
                values.sort_unstable_by_key(|value| value.get());
                (*key, values)
            })
            .collect();
        result.sort_unstable_by_key(|(key, _)| {
            (key.frame.slot, key.frame.generation, key.constraint.get())
        });
        result
    }

    #[cfg(debug_assertions)]
    pub(crate) fn logical_equal(
        &self,
        arena: &CanonicalArena,
        other: &Self,
        other_arena: &CanonicalArena,
    ) -> Result<bool, ArenaError> {
        if self.item_count != other.item_count || self.histories.len() != other.histories.len() {
            return Ok(false);
        }
        for (key, history) in &self.histories {
            let Some(other_history) = other.histories.get(key) else {
                return Ok(false);
            };
            if history.len != other_history.len {
                return Ok(false);
            }
            for value in history.buckets.values().flatten() {
                let mut found = false;
                for candidate in other_history.buckets.values().flatten() {
                    if arena.equal_across(*value, other_arena, *candidate)? {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    pub fn create_limited(
        &mut self,
        key: HistoryKey,
        max_histories: usize,
    ) -> Result<bool, ResourceError> {
        if self.histories.contains_key(&key) {
            return Ok(false);
        }
        let requested =
            self.histories
                .len()
                .checked_add(1)
                .ok_or(ResourceError::ArithmeticOverflow {
                    context: "uniqueness histories",
                })?;
        if requested > max_histories {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_histories",
                limit_value: max_histories,
                requested,
            });
        }
        self.histories
            .try_reserve(1)
            .map_err(|_| ResourceError::Allocation {
                context: "uniqueness histories",
                requested: 1,
            })?;
        self.histories.insert(key, CanonicalSet::default());
        Ok(true)
    }

    pub fn create_journaled(
        &mut self,
        key: HistoryKey,
        max_histories: usize,
        journal: &mut SemanticJournal,
    ) -> Result<(), HistoryError> {
        if self.histories.contains_key(&key) {
            return Err(HistoryError::DuplicateHistory(key));
        }
        let requested =
            self.histories
                .len()
                .checked_add(1)
                .ok_or(ResourceError::ArithmeticOverflow {
                    context: "uniqueness histories",
                })?;
        if requested > max_histories {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_histories",
                limit_value: max_histories,
                requested,
            }
            .into());
        }
        self.histories
            .try_reserve(1)
            .map_err(|_| ResourceError::Allocation {
                context: "uniqueness histories",
                requested: 1,
            })?;
        journal.preflight(1)?;
        journal.push_preflighted(SemanticUndo::RemoveCreatedHistory { key });
        self.histories.insert(key, CanonicalSet::default());
        Ok(())
    }

    pub fn get(&self, key: HistoryKey) -> Option<&CanonicalSet> {
        self.histories.get(&key)
    }

    pub fn get_mut(&mut self, key: HistoryKey) -> Option<&mut CanonicalSet> {
        self.histories.get_mut(&key)
    }

    pub fn remove(&mut self, key: HistoryKey) -> Option<CanonicalSet> {
        let history = self.histories.remove(&key)?;
        self.item_count = self
            .item_count
            .checked_sub(history.len())
            .expect("history item count covers every live history");
        Some(history)
    }

    pub fn insert_limited(
        &mut self,
        key: HistoryKey,
        arena: &CanonicalArena,
        value: CanonicalId,
        max_history_items: usize,
        max_bucket_entries: usize,
    ) -> Result<InsertResult, HistoryError> {
        let history = self
            .histories
            .get(&key)
            .ok_or(HistoryError::MissingHistory(key))?;
        if history.contains(arena, value)? {
            return Ok(InsertResult::Duplicate);
        }
        let requested =
            self.item_count
                .checked_add(1)
                .ok_or(ResourceError::ArithmeticOverflow {
                    context: "history items",
                })?;
        if requested > max_history_items {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_history_items",
                limit_value: max_history_items,
                requested,
            }
            .into());
        }
        let history = self
            .histories
            .get_mut(&key)
            .ok_or(HistoryError::MissingHistory(key))?;
        let result = history.insert_limited(arena, value, max_bucket_entries)?;
        if result == InsertResult::Inserted {
            self.item_count = requested;
        }
        Ok(result)
    }

    pub fn insert_journaled(
        &mut self,
        key: HistoryKey,
        arena: &CanonicalArena,
        value: CanonicalId,
        max_history_items: usize,
        max_bucket_entries: usize,
        journal: &mut SemanticJournal,
    ) -> Result<InsertResult, HistoryError> {
        let mut scratch = EqualityScratch::default();
        let mut equality_checks = 0;
        self.insert_journaled_with_scratch(
            key,
            arena,
            value,
            max_history_items,
            max_bucket_entries,
            journal,
            &mut scratch,
            &mut equality_checks,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert_journaled_with_scratch(
        &mut self,
        key: HistoryKey,
        arena: &CanonicalArena,
        value: CanonicalId,
        max_history_items: usize,
        max_bucket_entries: usize,
        journal: &mut SemanticJournal,
        scratch: &mut EqualityScratch,
        equality_checks: &mut usize,
        equality_limit: Option<usize>,
    ) -> Result<InsertResult, HistoryError> {
        let history = self
            .histories
            .get(&key)
            .ok_or(HistoryError::MissingHistory(key))?;
        let fingerprint = arena.fingerprint(value, &history.hash_builder)?;
        if let Some(bucket) = history.buckets.get(&fingerprint) {
            for candidate in bucket {
                if let Some(limit) = equality_limit {
                    *equality_checks = equality_checks.checked_add(1).ok_or(
                        ResourceError::ArithmeticOverflow {
                            context: "mask equality checks",
                        },
                    )?;
                    if *equality_checks > limit {
                        return Err(ResourceError::LimitExceeded {
                            limit_name: "max_equality_checks_per_mask",
                            limit_value: limit,
                            requested: *equality_checks,
                        }
                        .into());
                    }
                }
                if arena.equal_with_scratch(*candidate, value, scratch)? {
                    return Ok(InsertResult::Duplicate);
                }
            }
        }
        let requested =
            self.item_count
                .checked_add(1)
                .ok_or(ResourceError::ArithmeticOverflow {
                    context: "history items",
                })?;
        if requested > max_history_items {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_history_items",
                limit_value: max_history_items,
                requested,
            }
            .into());
        }
        let history = self
            .histories
            .get_mut(&key)
            .ok_or(HistoryError::MissingHistory(key))?;
        let next_history_len =
            history
                .len
                .checked_add(1)
                .ok_or(ResourceError::ArithmeticOverflow {
                    context: "history length",
                })?;
        journal.preflight(1)?;
        if let Some(bucket) = history.buckets.get_mut(&fingerprint) {
            let bucket_len =
                bucket
                    .len()
                    .checked_add(1)
                    .ok_or(ResourceError::ArithmeticOverflow {
                        context: "history bucket entries",
                    })?;
            if bucket_len > max_bucket_entries {
                return Err(ResourceError::LimitExceeded {
                    limit_name: "max_history_bucket_entries",
                    limit_value: max_bucket_entries,
                    requested: bucket_len,
                }
                .into());
            }
            bucket
                .try_reserve_exact(1)
                .map_err(|_| ResourceError::Allocation {
                    context: "history bucket",
                    requested: 1,
                })?;
            journal.push_preflighted(SemanticUndo::RestoreBucket {
                key,
                fingerprint,
                previous_len: bucket.len(),
                bucket_was_created: false,
                previous_item_count: self.item_count,
            });
            bucket.push(value);
        } else {
            if max_bucket_entries == 0 {
                return Err(ResourceError::LimitExceeded {
                    limit_name: "max_history_bucket_entries",
                    limit_value: 0,
                    requested: 1,
                }
                .into());
            }
            history
                .buckets
                .try_reserve(1)
                .map_err(|_| ResourceError::Allocation {
                    context: "history buckets",
                    requested: 1,
                })?;
            let mut bucket = Vec::new();
            bucket
                .try_reserve_exact(1)
                .map_err(|_| ResourceError::Allocation {
                    context: "history bucket",
                    requested: 1,
                })?;
            journal.push_preflighted(SemanticUndo::RestoreBucket {
                key,
                fingerprint,
                previous_len: 0,
                bucket_was_created: true,
                previous_item_count: self.item_count,
            });
            bucket.push(value);
            history.buckets.insert(fingerprint, bucket);
        }
        history.len = next_history_len;
        self.item_count = requested;
        Ok(InsertResult::Inserted)
    }

    pub fn close_journaled(
        &mut self,
        key: HistoryKey,
        journal: &mut SemanticJournal,
    ) -> Result<(), HistoryError> {
        journal.preflight(1)?;
        let previous_item_count = self.item_count;
        let history_len = self
            .histories
            .get(&key)
            .ok_or(HistoryError::MissingHistory(key))?
            .len();
        let next_item_count = previous_item_count
            .checked_sub(history_len)
            .ok_or(HistoryError::Invariant)?;
        let history = self
            .histories
            .remove(&key)
            .ok_or(HistoryError::MissingHistory(key))?;
        journal.push_preflighted(SemanticUndo::RestoreClosedHistory {
            key,
            history,
            previous_item_count,
        });
        self.item_count = next_item_count;
        Ok(())
    }

    pub(crate) fn undo(&mut self, undo: SemanticUndo) -> Result<(), JournalError> {
        match undo {
            SemanticUndo::RemoveCreatedHistory { key } => {
                let history = self.histories.remove(&key).ok_or(JournalError::Invariant)?;
                if !history.is_empty() {
                    return Err(JournalError::Invariant);
                }
            }
            SemanticUndo::RestoreClosedHistory {
                key,
                history,
                previous_item_count,
            } => {
                if self.histories.insert(key, history).is_some() {
                    return Err(JournalError::Invariant);
                }
                self.item_count = previous_item_count;
            }
            SemanticUndo::RestoreBucket {
                key,
                fingerprint,
                previous_len,
                bucket_was_created,
                previous_item_count,
            } => {
                let history = self
                    .histories
                    .get_mut(&key)
                    .ok_or(JournalError::Invariant)?;
                let bucket = history
                    .buckets
                    .get_mut(&fingerprint)
                    .ok_or(JournalError::Invariant)?;
                if previous_len > bucket.len() {
                    return Err(JournalError::Invariant);
                }
                bucket.truncate(previous_len);
                history.len = history.len.checked_sub(1).ok_or(JournalError::Invariant)?;
                if bucket_was_created {
                    if previous_len != 0 {
                        return Err(JournalError::Invariant);
                    }
                    history.buckets.remove(&fingerprint);
                }
                self.item_count = previous_item_count;
            }
            _ => return Err(JournalError::Invariant),
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.histories.len()
    }

    pub fn is_empty(&self) -> bool {
        self.histories.is_empty()
    }

    pub fn item_count(&self) -> usize {
        self.item_count
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum HistoryError {
    #[error("missing uniqueness history {0:?}")]
    MissingHistory(HistoryKey),
    #[error("duplicate uniqueness history {0:?}")]
    DuplicateHistory(HistoryKey),
    #[error("history invariant failed")]
    Invariant,
    #[error(transparent)]
    Arena(#[from] ArenaError),
    #[error(transparent)]
    Resource(#[from] ResourceError),
    #[error(transparent)]
    Journal(#[from] JournalError),
}
