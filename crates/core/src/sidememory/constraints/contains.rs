use crate::json_schema::ir::SchemaIr;

use super::super::counter::{CounterKey, CounterStore};
use super::super::guide::{GuideError, SemanticViolation};
use super::super::journal::SemanticJournal;
use super::super::limits::GuideLimits;
use super::super::plan::ContainsConstraint;
use super::super::predicate::{validate_predicate, PredicateScratch};
use super::super::{CanonicalArena, CanonicalId, FrameId};

pub(crate) fn open(
    counters: &mut CounterStore,
    frame: FrameId,
    constraint: &ContainsConstraint,
    limits: &GuideLimits,
    journal: &mut SemanticJournal,
) -> Result<(), GuideError> {
    counters
        .create_journaled(
            CounterKey {
                frame,
                constraint: constraint.constraint_id,
            },
            limits.max_active_counters,
            journal,
        )
        .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn seal_item(
    counters: &mut CounterStore,
    ir: &SchemaIr,
    arena: &CanonicalArena,
    scratch: &mut PredicateScratch,
    frame: FrameId,
    item_index: u64,
    value: CanonicalId,
    constraint: &ContainsConstraint,
    limits: &GuideLimits,
    journal: &mut SemanticJournal,
) -> Result<Option<SemanticViolation>, GuideError> {
    let matched = validate_predicate(constraint.predicate, ir, arena, value, scratch, limits)?;
    let state = counters.update_journaled(
        CounterKey {
            frame,
            constraint: constraint.constraint_id,
        },
        matched,
        journal,
    )?;
    if constraint.upper.is_some_and(|upper| state.matched > upper) {
        return Ok(Some(SemanticViolation::ContainsMaximumExceeded {
            frame,
            constraint: constraint.constraint_id,
            item_index,
            processed: state.processed,
            matched: state.matched,
            upper: constraint.upper.unwrap_or(u64::MAX),
        }));
    }
    if let Some(max_items) = constraint.array_max_items {
        let remaining = max_items.saturating_sub(state.processed);
        if state.matched.saturating_add(remaining) < constraint.lower {
            return Ok(Some(SemanticViolation::ContainsMinimumUnreachable {
                frame,
                constraint: constraint.constraint_id,
                item_index,
                processed: state.processed,
                matched: state.matched,
                lower: constraint.lower,
            }));
        }
    }
    Ok(None)
}

pub(crate) fn close(
    counters: &mut CounterStore,
    frame: FrameId,
    constraint: &ContainsConstraint,
    journal: &mut SemanticJournal,
) -> Result<Option<SemanticViolation>, GuideError> {
    let key = CounterKey {
        frame,
        constraint: constraint.constraint_id,
    };
    let state = counters.get(key).ok_or(GuideError::InternalInvariant {
        context: "contains counter missing at array close".into(),
    })?;
    if state.matched < constraint.lower {
        return Ok(Some(SemanticViolation::ContainsMinimumNotMet {
            frame,
            constraint: constraint.constraint_id,
            processed: state.processed,
            matched: state.matched,
            lower: constraint.lower,
        }));
    }
    if constraint.upper.is_some_and(|upper| state.matched > upper) {
        return Ok(Some(SemanticViolation::ContainsMaximumExceeded {
            frame,
            constraint: constraint.constraint_id,
            item_index: state.processed.saturating_sub(1),
            processed: state.processed,
            matched: state.matched,
            upper: constraint.upper.unwrap_or(u64::MAX),
        }));
    }
    counters.close_journaled(key, journal)?;
    Ok(None)
}
