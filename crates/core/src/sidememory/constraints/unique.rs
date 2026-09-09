use super::super::canonical::{CanonicalArena, EqualityScratch};
use super::super::guide::{GuideError, SemanticViolation};
use super::super::history::{HistoryKey, HistoryStore, InsertResult};
use super::super::journal::SemanticJournal;
use super::super::{CanonicalId, ConstraintId, FrameId, GuideLimits};

pub(crate) struct ItemContext<'a> {
    pub histories: &'a mut HistoryStore,
    pub arena: &'a CanonicalArena,
    pub journal: &'a mut SemanticJournal,
    pub equality: &'a mut EqualityScratch,
    pub equality_checks: &'a mut usize,
    pub equality_limit: Option<usize>,
    pub limits: &'a GuideLimits,
}

pub(crate) fn open(
    histories: &mut HistoryStore,
    key: HistoryKey,
    limits: &GuideLimits,
    journal: &mut SemanticJournal,
) -> Result<(), GuideError> {
    histories.create_journaled(key, limits.max_histories, journal)?;
    Ok(())
}

pub(crate) fn seal_item(
    context: ItemContext<'_>,
    frame: FrameId,
    constraint: ConstraintId,
    index: u64,
    value: CanonicalId,
) -> Result<Option<SemanticViolation>, GuideError> {
    let key = HistoryKey { frame, constraint };
    let result = context.histories.insert_journaled_with_scratch(
        key,
        context.arena,
        value,
        context.limits.max_history_items,
        context.limits.max_history_bucket_entries,
        context.journal,
        context.equality,
        context.equality_checks,
        context.equality_limit,
    )?;
    Ok(
        (result == InsertResult::Duplicate).then_some(SemanticViolation::DuplicateArrayItem {
            frame,
            constraint,
            item_index: index,
        }),
    )
}

pub(crate) fn close(
    histories: &mut HistoryStore,
    key: HistoryKey,
    journal: &mut SemanticJournal,
) -> Result<(), GuideError> {
    histories.close_journaled(key, journal)?;
    Ok(())
}
