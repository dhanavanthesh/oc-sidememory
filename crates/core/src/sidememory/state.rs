use crate::primitives::StateId;

use super::cursor::JsonCursor;
use super::history::HistoryStore;
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
    pub histories: HistoryStore,
    pub lifecycle: Lifecycle,
}
