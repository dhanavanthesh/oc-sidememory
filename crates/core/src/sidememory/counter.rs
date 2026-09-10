use std::collections::HashMap;

use thiserror::Error;

use super::journal::{JournalError, SemanticJournal, SemanticUndo};
use super::limits::ResourceError;
use super::plan::ConstraintId;
use super::scope::FrameId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CounterKey {
    pub frame: FrameId,
    pub constraint: ConstraintId,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContainsState {
    pub processed: u64,
    pub matched: u64,
}

#[derive(Debug, Default)]
pub struct CounterStore {
    counters: HashMap<CounterKey, ContainsState>,
}

impl CounterStore {
    pub(crate) fn create_journaled(
        &mut self,
        key: CounterKey,
        limit: usize,
        journal: &mut SemanticJournal,
    ) -> Result<(), CounterError> {
        if self.counters.contains_key(&key) {
            return Err(CounterError::DuplicateCounter(key));
        }
        let requested =
            self.counters
                .len()
                .checked_add(1)
                .ok_or(ResourceError::ArithmeticOverflow {
                    context: "contains counters",
                })?;
        if requested > limit {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_active_counters",
                limit_value: limit,
                requested,
            }
            .into());
        }
        self.counters
            .try_reserve(1)
            .map_err(|_| ResourceError::Allocation {
                context: "contains counters",
                requested: 1,
            })?;
        journal.preflight(1)?;
        journal.push_preflighted(SemanticUndo::RemoveCreatedCounter { key });
        self.counters.insert(key, ContainsState::default());
        Ok(())
    }

    pub(crate) fn update_journaled(
        &mut self,
        key: CounterKey,
        matched: bool,
        journal: &mut SemanticJournal,
    ) -> Result<ContainsState, CounterError> {
        let previous = *self
            .counters
            .get(&key)
            .ok_or(CounterError::MissingCounter(key))?;
        let next = ContainsState {
            processed: previous.processed.checked_add(1).ok_or(
                ResourceError::ArithmeticOverflow {
                    context: "contains processed count",
                },
            )?,
            matched: previous.matched.checked_add(u64::from(matched)).ok_or(
                ResourceError::ArithmeticOverflow {
                    context: "contains match count",
                },
            )?,
        };
        journal.preflight(1)?;
        journal.push_preflighted(SemanticUndo::RestoreCounter { key, previous });
        *self
            .counters
            .get_mut(&key)
            .ok_or(CounterError::MissingCounter(key))? = next;
        Ok(next)
    }

    pub(crate) fn close_journaled(
        &mut self,
        key: CounterKey,
        journal: &mut SemanticJournal,
    ) -> Result<ContainsState, CounterError> {
        journal.preflight(1)?;
        let state = self
            .counters
            .remove(&key)
            .ok_or(CounterError::MissingCounter(key))?;
        journal.push_preflighted(SemanticUndo::RestoreClosedCounter { key, state });
        Ok(state)
    }

    pub(crate) fn get(&self, key: CounterKey) -> Option<ContainsState> {
        self.counters.get(&key).copied()
    }

    #[cfg(debug_assertions)]
    pub(crate) fn logical_snapshot(&self) -> Vec<(CounterKey, ContainsState)> {
        let mut counters: Vec<_> = self
            .counters
            .iter()
            .map(|(key, state)| (*key, *state))
            .collect();
        counters.sort_unstable_by_key(|(key, _)| {
            (key.frame.slot, key.frame.generation, key.constraint.get())
        });
        counters
    }

    pub(crate) fn undo(&mut self, undo: SemanticUndo) -> Result<(), JournalError> {
        match undo {
            SemanticUndo::RemoveCreatedCounter { key } => {
                let state = self.counters.remove(&key).ok_or(JournalError::Invariant)?;
                if state != ContainsState::default() {
                    return Err(JournalError::Invariant);
                }
            }
            SemanticUndo::RestoreCounter { key, previous } => {
                *self.counters.get_mut(&key).ok_or(JournalError::Invariant)? = previous;
            }
            SemanticUndo::RestoreClosedCounter { key, state } => {
                if self.counters.insert(key, state).is_some() {
                    return Err(JournalError::Invariant);
                }
            }
            _ => return Err(JournalError::Invariant),
        }
        Ok(())
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CounterError {
    #[error("missing contains counter {0:?}")]
    MissingCounter(CounterKey),
    #[error("duplicate contains counter {0:?}")]
    DuplicateCounter(CounterKey),
    #[error(transparent)]
    Resource(#[from] ResourceError),
    #[error(transparent)]
    Journal(#[from] JournalError),
}
