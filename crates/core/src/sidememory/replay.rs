use thiserror::Error;

use crate::index::Index;
use crate::primitives::{StateId, TokenId};

use super::{CursorError, CursorSnapshot, JsonCursor, JsonEvent, RuntimeLimits, TokenTable};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayState {
    pub dfa_state: StateId,
    pub cursor: CursorSnapshot,
    pub events: Vec<JsonEvent>,
}

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("token {token_id} has no transition from DFA state {state}")]
    MissingTransition { state: StateId, token_id: TokenId },
    #[error("EOS token {token_id} is not allowed from DFA state {state}")]
    EosNotAllowed { state: StateId, token_id: TokenId },
    #[error("token {token_id} appears after EOS")]
    TokenAfterEos { token_id: TokenId },
    #[error(transparent)]
    Cursor(#[from] CursorError),
}

pub fn replay_committed(
    index: &Index,
    token_table: &TokenTable,
    token_ids: &[TokenId],
    limits: RuntimeLimits,
) -> Result<ReplayState, ReplayError> {
    let mut dfa_state = index.initial_state();
    let mut cursor = JsonCursor::new(limits);
    let mut events = Vec::new();
    let mut finished = false;

    for &token_id in token_ids {
        if finished {
            return Err(ReplayError::TokenAfterEos { token_id });
        }
        if token_id == token_table.eos_token_id() {
            let eos_allowed = index
                .allowed_tokens_iter(&dfa_state)
                .is_some_and(|mut ids| ids.any(|id| *id == token_id));
            if !eos_allowed {
                return Err(ReplayError::EosNotAllowed {
                    state: dfa_state,
                    token_id,
                });
            }
            events.extend(cursor.finish_eos()?);
            finished = true;
            continue;
        }
        let next =
            index
                .next_state(&dfa_state, &token_id)
                .ok_or(ReplayError::MissingTransition {
                    state: dfa_state,
                    token_id,
                })?;
        events.extend(cursor.feed_token(token_id, token_table)?);
        dfa_state = next;
    }

    Ok(ReplayState {
        dfa_state,
        cursor: cursor.snapshot(),
        events,
    })
}
