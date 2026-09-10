use thiserror::Error;

use crate::index::Index;
use crate::primitives::{StateId, TokenId};

use super::{
    ArenaError, CursorError, CursorSnapshot, Guide, GuideError, GuideOptions, JsonCursor,
    JsonEvent, RuntimeLimits, TokenTable,
};

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
    #[error(transparent)]
    Arena(#[from] ArenaError),
    #[error(transparent)]
    Guide(#[from] GuideError),
    #[error("semantic replay differs in {component}")]
    Mismatch { component: &'static str },
}

#[cfg(debug_assertions)]
pub fn verify_guide_replay(guide: &Guide) -> Result<(), ReplayError> {
    let options = GuideOptions {
        max_rollback_tokens: 0,
        limits: guide.limits.clone(),
    };
    let mut replay = match &guide.imports {
        Some(imports) => Guide::new_with_imports(guide.compiled.clone(), options, imports.clone())?,
        None => Guide::new(guide.compiled.clone(), options)?,
    };
    for token in guide.committed_trace.iter().copied() {
        replay.advance(token)?;
    }
    if replay.state.dfa_state != guide.state.dfa_state {
        return Err(ReplayError::Mismatch {
            component: "DFA state",
        });
    }
    if replay.state.cursor.snapshot() != guide.state.cursor.snapshot() {
        return Err(ReplayError::Mismatch {
            component: "JSON cursor",
        });
    }
    if replay.state.router != guide.state.router {
        return Err(ReplayError::Mismatch {
            component: "semantic router",
        });
    }
    if !replay.state.semantic.histories.logical_equal(
        replay.state.cursor.arena(),
        &guide.state.semantic.histories,
        guide.state.cursor.arena(),
    )? {
        return Err(ReplayError::Mismatch {
            component: "uniqueness histories",
        });
    }
    if replay.state.semantic.counters.logical_snapshot()
        != guide.state.semantic.counters.logical_snapshot()
    {
        return Err(ReplayError::Mismatch {
            component: "contains counters",
        });
    }
    if !replay.state.semantic.registers.logical_equal(
        replay.state.cursor.arena(),
        &guide.state.semantic.registers,
        guide.state.cursor.arena(),
    )? {
        return Err(ReplayError::Mismatch {
            component: "capture registers",
        });
    }
    if replay.state.lifecycle != guide.state.lifecycle {
        return Err(ReplayError::Mismatch {
            component: "lifecycle",
        });
    }
    if replay.committed_trace != guide.committed_trace {
        return Err(ReplayError::Mismatch {
            component: "committed trace",
        });
    }
    Ok(())
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
