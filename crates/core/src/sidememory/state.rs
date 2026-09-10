use crate::primitives::StateId;

use super::counter::CounterStore;
use super::cursor::JsonCursor;
use super::history::HistoryStore;
use super::journal::{JournalError, SemanticUndo};
use super::register::RegisterStore;
use super::router::SemanticRouter;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lifecycle {
    Active,
    Terminated,
    Poisoned,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceLedger {
    pub retained_rollback_bytes: usize,
}

pub struct GuideState {
    pub dfa_state: StateId,
    pub cursor: JsonCursor,
    pub router: SemanticRouter,
    pub semantic: SemanticState,
    pub lifecycle: Lifecycle,
}

#[derive(Debug, Default)]
pub struct SemanticState {
    pub histories: HistoryStore,
    pub counters: CounterStore,
    pub registers: RegisterStore,
}

impl SemanticState {
    pub(crate) fn undo(&mut self, undo: SemanticUndo) -> Result<(), JournalError> {
        match undo {
            SemanticUndo::RemoveCreatedHistory { .. }
            | SemanticUndo::RestoreClosedHistory { .. }
            | SemanticUndo::RestoreBucket { .. } => self.histories.undo(undo),
            SemanticUndo::RemoveCreatedCounter { .. }
            | SemanticUndo::RestoreCounter { .. }
            | SemanticUndo::RestoreClosedCounter { .. } => self.counters.undo(undo),
            SemanticUndo::RemoveCreatedRegisterScope { .. }
            | SemanticUndo::RestoreRegister { .. }
            | SemanticUndo::RestoreClosedRegisterScope { .. } => self.registers.undo(undo),
        }
    }
}
