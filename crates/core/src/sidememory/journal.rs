use std::mem::size_of;
use thiserror::Error;

use crate::json_schema::ir::SchemaNodeId;

use super::history::{CanonicalSet, HistoryKey, HistoryStore};
use super::limits::ResourceError;
use super::router::{RuntimeFrame, SemanticRouter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SemanticJournalMark(pub(crate) usize);

#[derive(Debug)]
pub struct SemanticJournal {
    undo: Vec<SemanticUndo>,
    max_records: usize,
}

#[derive(Debug)]
pub(crate) enum SemanticUndo {
    RemoveCreatedHistory {
        key: HistoryKey,
    },
    RestoreClosedHistory {
        key: HistoryKey,
        history: CanonicalSet,
        previous_item_count: usize,
    },
    RestoreBucket {
        key: HistoryKey,
        fingerprint: u64,
        previous_len: usize,
        bucket_was_created: bool,
        previous_item_count: usize,
    },
}

impl SemanticJournal {
    pub fn new(max_records: usize) -> Self {
        Self {
            undo: Vec::new(),
            max_records,
        }
    }

    pub fn mark(&self) -> SemanticJournalMark {
        SemanticJournalMark(self.undo.len())
    }

    pub(crate) fn preflight(&mut self, additional: usize) -> Result<(), JournalError> {
        preflight_records(
            &mut self.undo,
            self.max_records,
            additional,
            "semantic journal",
        )
    }

    pub(crate) fn push_preflighted(&mut self, undo: SemanticUndo) {
        debug_assert!(self.undo.len() < self.max_records);
        debug_assert!(self.undo.len() < self.undo.capacity());
        self.undo.push(undo);
    }

    pub fn restore(
        &mut self,
        mark: SemanticJournalMark,
        histories: &mut HistoryStore,
    ) -> Result<(), JournalError> {
        if mark.0 > self.undo.len() {
            return Err(JournalError::InvalidMark);
        }
        while self.undo.len() > mark.0 {
            let undo = self.undo.pop().ok_or(JournalError::InvalidMark)?;
            histories.undo(undo)?;
        }
        Ok(())
    }

    pub(crate) fn discard_prefix(&mut self, count: usize) -> Result<(), JournalError> {
        if count > self.undo.len() {
            return Err(JournalError::InvalidMark);
        }
        self.undo.drain(..count);
        Ok(())
    }

    pub(crate) fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub(crate) fn retained_bytes_from(&self, start: usize) -> Result<usize, ResourceError> {
        let records = self
            .undo
            .get(start..)
            .ok_or(ResourceError::ArithmeticOverflow {
                context: "semantic journal retained range",
            })?;
        let base = records.len().checked_mul(size_of::<SemanticUndo>()).ok_or(
            ResourceError::ArithmeticOverflow {
                context: "semantic journal retained bytes",
            },
        )?;
        records.iter().try_fold(base, |total, undo| {
            let dynamic = match undo {
                SemanticUndo::RestoreClosedHistory { history, .. } => history.retained_bytes()?,
                _ => 0,
            };
            total
                .checked_add(dynamic)
                .ok_or(ResourceError::ArithmeticOverflow {
                    context: "semantic journal retained bytes",
                })
        })
    }
}

impl SemanticJournalMark {
    pub(crate) fn undo_len(self) -> usize {
        self.0
    }

    pub(crate) fn shift(&mut self, count: usize) -> Result<(), JournalError> {
        self.0 = self.0.checked_sub(count).ok_or(JournalError::InvalidMark)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouterJournalMark(pub(crate) usize);

#[derive(Debug)]
pub struct RouterJournal {
    undo: Vec<RouterUndo>,
    max_records: usize,
}

#[derive(Debug)]
pub(crate) enum RouterUndo {
    PopOpenedFrame {
        previous_pending: SchemaNodeId,
    },
    RestoreClosedFrame {
        frame: RuntimeFrame,
        previous_pending: SchemaNodeId,
    },
    RestorePending {
        frame_index: usize,
        previous_property: Option<SchemaNodeId>,
        previous_pending: SchemaNodeId,
    },
    RestoreRootComplete(bool),
}

impl RouterJournal {
    pub fn new(max_records: usize) -> Self {
        Self {
            undo: Vec::new(),
            max_records,
        }
    }

    pub fn mark(&self) -> RouterJournalMark {
        RouterJournalMark(self.undo.len())
    }

    pub(crate) fn preflight(&mut self, additional: usize) -> Result<(), JournalError> {
        preflight_records(
            &mut self.undo,
            self.max_records,
            additional,
            "router journal",
        )
    }

    pub(crate) fn push_preflighted(&mut self, undo: RouterUndo) {
        debug_assert!(self.undo.len() < self.max_records);
        debug_assert!(self.undo.len() < self.undo.capacity());
        self.undo.push(undo);
    }

    pub fn restore(
        &mut self,
        mark: RouterJournalMark,
        router: &mut SemanticRouter,
    ) -> Result<(), JournalError> {
        if mark.0 > self.undo.len() {
            return Err(JournalError::InvalidMark);
        }
        while self.undo.len() > mark.0 {
            let undo = self.undo.pop().ok_or(JournalError::InvalidMark)?;
            router.undo(undo)?;
        }
        Ok(())
    }

    pub(crate) fn discard_prefix(&mut self, count: usize) -> Result<(), JournalError> {
        if count > self.undo.len() {
            return Err(JournalError::InvalidMark);
        }
        self.undo.drain(..count);
        Ok(())
    }

    pub(crate) fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub(crate) fn retained_bytes_from(&self, start: usize) -> Result<usize, ResourceError> {
        let count = self
            .undo
            .get(start..)
            .ok_or(ResourceError::ArithmeticOverflow {
                context: "router journal retained range",
            })?
            .len();
        count
            .checked_mul(size_of::<RouterUndo>())
            .ok_or(ResourceError::ArithmeticOverflow {
                context: "router journal retained bytes",
            })
    }
}

impl RouterJournalMark {
    pub(crate) fn undo_len(self) -> usize {
        self.0
    }

    pub(crate) fn shift(&mut self, count: usize) -> Result<(), JournalError> {
        self.0 = self.0.checked_sub(count).ok_or(JournalError::InvalidMark)?;
        Ok(())
    }
}

fn preflight_records<T>(
    records: &mut Vec<T>,
    limit: usize,
    additional: usize,
    context: &'static str,
) -> Result<(), JournalError> {
    let requested = records
        .len()
        .checked_add(additional)
        .ok_or(ResourceError::ArithmeticOverflow { context })?;
    if requested > limit {
        return Err(ResourceError::LimitExceeded {
            limit_name: "max_journal_records",
            limit_value: limit,
            requested,
        }
        .into());
    }
    records
        .try_reserve_exact(additional)
        .map_err(|_| ResourceError::Allocation {
            context,
            requested: additional,
        })?;
    Ok(())
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum JournalError {
    #[error(transparent)]
    Resource(#[from] ResourceError),
    #[error("invalid journal mark")]
    InvalidMark,
    #[error("journal restoration invariant failed")]
    Invariant,
}
