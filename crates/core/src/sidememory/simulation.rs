use crate::json_schema::compile::CompiledSchema;
use crate::json_schema::extensions::RelationOperator;
use crate::primitives::TokenId;

#[cfg(debug_assertions)]
use super::guide::FailurePoint;
use super::guide::{Guide, GuideError, GuideScratch, ProbeDecision, SemanticViolation};
use super::history::HistoryKey;
use super::journal::{RouterJournal, SemanticJournal};
use super::limits::GuideLimits;
use super::state::{GuideState, Lifecycle};
use super::JsonEvent;

#[derive(Clone, Copy)]
pub(crate) enum TransitionMode {
    Probe,
    Commit,
}

pub(crate) fn transition(
    guide: &mut Guide,
    token_id: TokenId,
    mode: TransitionMode,
) -> Result<ProbeDecision, GuideError> {
    let next_state = validate_token(guide, token_id)?;
    let eos = guide.compiled.token_table().eos_token_id();
    if matches!(mode, TransitionMode::Commit) {
        guide.rollback.preflight()?;
        preflight_trace(guide)?;
    }
    let mark = guide.mark();
    let result = run_transition(guide, token_id, eos, next_state);
    match result {
        Ok(ProbeDecision::Allow) if matches!(mode, TransitionMode::Commit) => {
            let retained = match guide.retained_bytes_after_push(mark) {
                Ok(retained) => retained,
                Err(error) => {
                    guide.restore(mark)?;
                    guide.bump_revision()?;
                    return Err(error);
                }
            };
            if retained > guide.limits.max_retained_rollback_bytes {
                guide.restore(mark)?;
                guide.bump_revision()?;
                return Err(super::limits::ResourceError::LimitExceeded {
                    limit_name: "max_retained_rollback_bytes",
                    limit_value: guide.limits.max_retained_rollback_bytes,
                    requested: retained,
                }
                .into());
            }
            #[cfg(debug_assertions)]
            if let Err(error) = guide.fail_if(FailurePoint::BeforeCheckpointInsert) {
                guide.restore(mark)?;
                guide.bump_revision()?;
                return Err(error);
            }
            guide.rollback.push(mark);
            #[cfg(debug_assertions)]
            guide.committed_trace.push(token_id);
            if let Err(error) = guide.rollback.compact(
                &mut guide.cursor_journal,
                &mut guide.router_journal,
                &mut guide.semantic_journal,
            ) {
                guide.state.lifecycle = Lifecycle::Poisoned;
                return Err(error.into());
            }
            if let Err(error) = guide.refresh_retained_rollback_bytes() {
                guide.state.lifecycle = Lifecycle::Poisoned;
                return Err(error);
            }
            guide.bump_revision()?;
            Ok(ProbeDecision::Allow)
        }
        Ok(decision) => {
            guide.restore(mark)?;
            guide.bump_revision()?;
            Ok(decision)
        }
        Err(error) => {
            guide.restore(mark)?;
            guide.bump_revision()?;
            Err(error)
        }
    }
}

pub(crate) fn transition_uncommitted(
    guide: &mut Guide,
    token_id: TokenId,
) -> Result<ProbeDecision, GuideError> {
    let next_state = validate_token(guide, token_id)?;
    let eos = guide.compiled.token_table().eos_token_id();
    run_transition(guide, token_id, eos, next_state)
}

fn validate_token(guide: &Guide, token_id: TokenId) -> Result<u32, GuideError> {
    guide.require_active("transition")?;
    let eos = guide.compiled.token_table().eos_token_id();
    if token_id == eos {
        if guide
            .compiled
            .index()
            .is_final_state(&guide.state.dfa_state)
        {
            return Ok(guide.state.dfa_state);
        }
        return Err(GuideError::StructuralRejection {
            state: guide.state.dfa_state,
            token_id,
        });
    }
    match guide.compiled.token_table().get(token_id) {
        Ok(_) => {}
        Err(super::token_table::VocabularyError::UnknownToken { .. }) => {
            return Err(GuideError::UnknownToken { token_id });
        }
        Err(error) => return Err(error.into()),
    }
    guide
        .compiled
        .index()
        .next_state(&guide.state.dfa_state, &token_id)
        .ok_or(GuideError::StructuralRejection {
            state: guide.state.dfa_state,
            token_id,
        })
}

fn run_transition(
    guide: &mut Guide,
    token_id: TokenId,
    eos: TokenId,
    next_state: u32,
) -> Result<ProbeDecision, GuideError> {
    let Guide {
        compiled,
        imports,
        state,
        cursor_journal,
        semantic_journal,
        router_journal,
        limits,
        scratch,
        ..
    } = guide;
    let GuideScratch {
        events,
        equality,
        equality_checks,
        equality_limit,
        predicate,
        #[cfg(debug_assertions)]
        failure,
        ..
    } = scratch;
    if token_id == eos {
        events.clear();
        state.cursor.finish_eos_into(events, cursor_journal)?;
        let mut context = EventContext {
            compiled: compiled.as_ref(),
            imports: imports.as_deref(),
            state,
            router_journal,
            semantic_journal,
            limits,
            equality,
            equality_checks,
            equality_limit: *equality_limit,
            predicate,
            #[cfg(debug_assertions)]
            failure,
        };
        process_pending_events(&mut context, events)?;
        state.lifecycle = Lifecycle::Terminated;
        return Ok(ProbeDecision::Allow);
    }
    let bytes = compiled.token_table().get(token_id)?.bytes();
    for &byte in bytes {
        events.clear();
        #[cfg(debug_assertions)]
        inject_failure(failure, FailurePoint::BeforeCursorMutation)?;
        state.cursor.feed_byte_into(byte, events, cursor_journal)?;
        #[cfg(debug_assertions)]
        inject_failure(failure, FailurePoint::AfterCursorMutation)?;
        let mut context = EventContext {
            compiled: compiled.as_ref(),
            imports: imports.as_deref(),
            state,
            router_journal,
            semantic_journal,
            limits,
            equality,
            equality_checks,
            equality_limit: *equality_limit,
            predicate,
            #[cfg(debug_assertions)]
            failure,
        };
        if let Some(violation) = process_pending_events(&mut context, events)? {
            return Ok(ProbeDecision::Deny(violation));
        }
    }
    state.dfa_state = next_state;
    Ok(ProbeDecision::Allow)
}

struct EventContext<'a> {
    compiled: &'a CompiledSchema,
    imports: Option<&'a super::ImportedMemory>,
    state: &'a mut GuideState,
    router_journal: &'a mut RouterJournal,
    semantic_journal: &'a mut SemanticJournal,
    limits: &'a GuideLimits,
    equality: &'a mut super::canonical::EqualityScratch,
    equality_checks: &'a mut usize,
    equality_limit: Option<usize>,
    predicate: &'a mut super::predicate::PredicateScratch,
    #[cfg(debug_assertions)]
    failure: &'a mut Option<FailurePoint>,
}

fn process_pending_events(
    context: &mut EventContext<'_>,
    events: &mut Vec<JsonEvent>,
) -> Result<Option<SemanticViolation>, GuideError> {
    events.reverse();
    while let Some(event) = events.pop() {
        if let Some(violation) = process_event(context, event)? {
            events.clear();
            return Ok(Some(violation));
        }
    }
    Ok(None)
}

fn process_event(
    context: &mut EventContext<'_>,
    event: JsonEvent,
) -> Result<Option<SemanticViolation>, GuideError> {
    let EventContext {
        compiled,
        imports,
        state,
        router_journal,
        semantic_journal,
        limits,
        equality,
        equality_checks,
        equality_limit,
        predicate,
        #[cfg(debug_assertions)]
        failure,
    } = context;
    match event {
        JsonEvent::ArrayStart(frame) => {
            let node = state
                .router
                .open_array_journaled(frame, compiled.ir(), router_journal)?;
            #[cfg(debug_assertions)]
            inject_failure(failure, FailurePoint::AfterFrameAllocation)?;
            if let Some(constraint) = compiled.memory_plan().unique_for_node(node) {
                super::constraints::unique::open(
                    &mut state.semantic.histories,
                    HistoryKey {
                        frame,
                        constraint: constraint.constraint_id,
                    },
                    limits,
                    semantic_journal,
                )?;
                #[cfg(debug_assertions)]
                inject_failure(failure, FailurePoint::AfterHistoryCreation)?;
            }
            if let Some(constraint) = compiled.memory_plan().contains_for_node(node) {
                super::constraints::contains::open(
                    &mut state.semantic.counters,
                    frame,
                    constraint,
                    limits,
                    semantic_journal,
                )?;
                #[cfg(debug_assertions)]
                inject_failure(failure, FailurePoint::AfterCounterCreation)?;
            }
        }
        JsonEvent::ObjectStart(frame) => {
            let node = state
                .router
                .open_object_journaled(frame, compiled.ir(), router_journal)?;
            if compiled
                .memory_plan()
                .object_extension_for_node(node)
                .is_some()
            {
                state.semantic.registers.open_journaled(
                    frame,
                    limits.max_register_scopes,
                    semantic_journal,
                )?;
                #[cfg(debug_assertions)]
                inject_failure(failure, FailurePoint::AfterRegisterScopeCreation)?;
            }
            #[cfg(debug_assertions)]
            inject_failure(failure, FailurePoint::AfterFrameAllocation)?;
        }
        JsonEvent::PropertyKeySealed { object, key } => {
            state.router.set_pending_property_journaled(
                object,
                &key,
                compiled.ir(),
                router_journal,
            )?;
        }
        JsonEvent::ScalarSealed(_) => {}
        JsonEvent::ItemSealed {
            array,
            index,
            value,
        } => {
            let node = state.router.array_schema_node(array)?;
            if let Some(constraint) = compiled.memory_plan().unique_for_node(node) {
                #[cfg(debug_assertions)]
                inject_failure(failure, FailurePoint::BeforeHistoryInsert)?;
                let violation = super::constraints::unique::seal_item(
                    super::constraints::unique::ItemContext {
                        histories: &mut state.semantic.histories,
                        arena: state.cursor.arena(),
                        journal: semantic_journal,
                        equality,
                        equality_checks,
                        equality_limit: *equality_limit,
                        limits,
                    },
                    array,
                    constraint.constraint_id,
                    index,
                    value,
                )?;
                #[cfg(debug_assertions)]
                inject_failure(failure, FailurePoint::AfterHistoryInsert)?;
                if violation.is_some() {
                    return Ok(violation);
                }
            }
            if let Some(constraint) = compiled.memory_plan().contains_for_node(node) {
                #[cfg(debug_assertions)]
                inject_failure(failure, FailurePoint::BeforeContainsEvaluation)?;
                let violation = super::constraints::contains::seal_item(
                    &mut state.semantic.counters,
                    compiled.ir(),
                    state.cursor.arena(),
                    predicate,
                    array,
                    index,
                    value,
                    constraint,
                    limits,
                    semantic_journal,
                )?;
                #[cfg(debug_assertions)]
                inject_failure(failure, FailurePoint::AfterCounterUpdate)?;
                if violation.is_some() {
                    return Ok(violation);
                }
            }
            state
                .router
                .complete_array_item_journaled(array, router_journal)?;
        }
        JsonEvent::PropertyValueSealed { object, key, value } => {
            let node = state.router.object_schema_node(object)?;
            if let Some(plan) = compiled.memory_plan().object_extension_for_node(node) {
                let key_text =
                    std::str::from_utf8(&key).map_err(|_| GuideError::InternalInvariant {
                        context: "sealed property key is not UTF-8".into(),
                    })?;
                if let Some(actions) = plan.actions.get(key_text) {
                    for relation in &actions.relations {
                        #[cfg(debug_assertions)]
                        inject_failure(failure, FailurePoint::BeforeRelationEvaluation)?;
                        let violation = match relation.operator {
                            RelationOperator::Equal | RelationOperator::NotEqual => {
                                super::constraints::equality::evaluate(
                                    relation,
                                    &state.semantic.registers,
                                    state.cursor.arena(),
                                    object,
                                    value,
                                    equality,
                                )?
                            }
                            RelationOperator::MemberOf | RelationOperator::NotMemberOf => {
                                let imports = imports.ok_or(GuideError::InternalInvariant {
                                    context: "validated imported memory is absent".into(),
                                })?;
                                super::constraints::membership::evaluate(
                                    relation,
                                    imports,
                                    state.cursor.arena(),
                                    object,
                                    value,
                                    equality,
                                )?
                            }
                        };
                        if violation.is_some() {
                            return Ok(violation);
                        }
                    }
                    for capture in &actions.captures {
                        state.semantic.registers.set_journaled(
                            super::RegisterKey {
                                object,
                                register: capture.register,
                            },
                            value,
                            limits.max_register_values,
                            semantic_journal,
                        )?;
                        #[cfg(debug_assertions)]
                        inject_failure(failure, FailurePoint::AfterCapturePublication)?;
                    }
                }
            }
            state
                .router
                .complete_property_journaled(object, router_journal)?;
        }
        JsonEvent::ArrayEnd { frame, .. } => {
            #[cfg(debug_assertions)]
            inject_failure(failure, FailurePoint::BeforeArrayClose)?;
            let node = state.router.array_schema_node(frame)?;
            if let Some(constraint) = compiled.memory_plan().contains_for_node(node) {
                let violation = super::constraints::contains::close(
                    &mut state.semantic.counters,
                    frame,
                    constraint,
                    semantic_journal,
                )?;
                if violation.is_some() {
                    return Ok(violation);
                }
            }
            if let Some(constraint) = compiled.memory_plan().unique_for_node(node) {
                super::constraints::unique::close(
                    &mut state.semantic.histories,
                    HistoryKey {
                        frame,
                        constraint: constraint.constraint_id,
                    },
                    semantic_journal,
                )?;
                #[cfg(debug_assertions)]
                inject_failure(failure, FailurePoint::AfterHistoryClose)?;
            }
            state.router.close_journaled(frame, router_journal)?;
        }
        JsonEvent::ObjectEnd { frame, .. } => {
            let node = state.router.object_schema_node(frame)?;
            if compiled
                .memory_plan()
                .object_extension_for_node(node)
                .is_some()
            {
                state
                    .semantic
                    .registers
                    .close_journaled(frame, semantic_journal)?;
                #[cfg(debug_assertions)]
                inject_failure(failure, FailurePoint::AfterRegisterScopeClose)?;
            }
            state.router.close_journaled(frame, router_journal)?;
        }
        JsonEvent::RootComplete(_) => {
            state.router.complete_root_journaled(router_journal)?;
        }
    }
    Ok(None)
}

#[cfg(debug_assertions)]
fn inject_failure(
    failure: &mut Option<FailurePoint>,
    point: FailurePoint,
) -> Result<(), GuideError> {
    if *failure == Some(point) {
        *failure = None;
        return Err(super::limits::ResourceError::Allocation {
            context: "injected failure",
            requested: 1,
        }
        .into());
    }
    Ok(())
}

fn preflight_trace(_guide: &mut Guide) -> Result<(), GuideError> {
    #[cfg(debug_assertions)]
    {
        let requested = _guide.committed_trace.len().checked_add(1).ok_or(
            super::limits::ResourceError::ArithmeticOverflow {
                context: "committed trace",
            },
        )?;
        if requested > _guide.limits.max_debug_trace_tokens {
            return Err(super::limits::ResourceError::LimitExceeded {
                limit_name: "max_debug_trace_tokens",
                limit_value: _guide.limits.max_debug_trace_tokens,
                requested,
            }
            .into());
        }
        _guide.committed_trace.try_reserve_exact(1).map_err(|_| {
            super::limits::ResourceError::Allocation {
                context: "committed trace",
                requested: 1,
            }
        })?;
    }
    Ok(())
}
