use std::collections::VecDeque;

use crate::primitives::StateId;

use super::cursor::{CursorError, CursorJournal, CursorJournalMark};
use super::journal::{
    JournalError, RouterJournal, RouterJournalMark, SemanticJournal, SemanticJournalMark,
};
use super::state::Lifecycle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenMark {
    pub(crate) dfa_state: StateId,
    pub(crate) lifecycle: Lifecycle,
    pub(crate) cursor: CursorJournalMark,
    pub(crate) router: RouterJournalMark,
    pub(crate) semantic: SemanticJournalMark,
    pub(crate) trace_len: usize,
}

#[derive(Debug)]
pub struct RollbackWindow {
    checkpoints: VecDeque<TokenMark>,
    max_tokens: usize,
}

impl RollbackWindow {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            checkpoints: VecDeque::new(),
            max_tokens,
        }
    }

    pub fn preflight(&mut self) -> Result<(), JournalError> {
        if self.max_tokens == 0 {
            return Ok(());
        }
        self.checkpoints.try_reserve_exact(1).map_err(|_| {
            super::limits::ResourceError::Allocation {
                context: "rollback checkpoints",
                requested: 1,
            }
        })?;
        Ok(())
    }

    pub fn push(&mut self, mark: TokenMark) {
        if self.max_tokens == 0 {
            return;
        }
        self.checkpoints.push_back(mark);
    }

    pub fn pop(&mut self) -> Option<TokenMark> {
        self.checkpoints.pop_back()
    }

    pub fn len(&self) -> usize {
        self.checkpoints.len()
    }

    pub fn is_empty(&self) -> bool {
        self.checkpoints.is_empty()
    }

    pub fn clear(&mut self) {
        self.checkpoints.clear();
    }

    pub(crate) fn retained_len_after_push(&self) -> usize {
        self.checkpoints
            .len()
            .saturating_add(1)
            .min(self.max_tokens)
    }

    pub(crate) fn retained_start_after_push(&self, mark: TokenMark) -> Option<TokenMark> {
        if self.max_tokens == 0 {
            return None;
        }
        if self.checkpoints.len() < self.max_tokens {
            return self.checkpoints.front().copied().or(Some(mark));
        }
        self.checkpoints.get(1).copied().or(Some(mark))
    }

    pub fn compact(
        &mut self,
        cursor: &mut CursorJournal,
        router: &mut RouterJournal,
        semantic: &mut SemanticJournal,
    ) -> Result<(), RollbackError> {
        while self.checkpoints.len() > self.max_tokens {
            self.checkpoints.pop_front();
        }
        let cursor_prefix = self
            .checkpoints
            .front()
            .map_or_else(|| cursor.undo_len(), |mark| mark.cursor.undo_len());
        let router_prefix = self
            .checkpoints
            .front()
            .map_or_else(|| router.undo_len(), |mark| mark.router.undo_len());
        let semantic_prefix = self
            .checkpoints
            .front()
            .map_or_else(|| semantic.undo_len(), |mark| mark.semantic.undo_len());
        cursor.discard_prefix(cursor_prefix)?;
        router.discard_prefix(router_prefix)?;
        semantic.discard_prefix(semantic_prefix)?;
        for mark in &mut self.checkpoints {
            mark.cursor.shift(cursor_prefix)?;
            mark.router.shift(router_prefix)?;
            mark.semantic.shift(semantic_prefix)?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RollbackError {
    #[error(transparent)]
    Cursor(#[from] CursorError),
    #[error(transparent)]
    Journal(#[from] JournalError),
}
