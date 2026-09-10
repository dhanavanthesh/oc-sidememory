use std::sync::Arc;

use thiserror::Error;

use crate::json_schema::compile::CompiledSchema;
use crate::primitives::{StateId, TokenId};

use super::canonical::ArenaError;
use super::canonical::EqualityScratch;
use super::counter::CounterError;
use super::cursor::{CursorError, CursorJournal};
use super::history::HistoryError;
use super::imports::{ImportError, ImportedMemory};
use super::journal::{JournalError, RouterJournal, SemanticJournal};
use super::limits::{GuideLimits, ResourceError};
use super::register::RegisterError;
use super::rollback::{RollbackError, RollbackWindow, TokenMark};
use super::router::{RouterError, SemanticRouter};
use super::state::{GuideState, Lifecycle, ResourceLedger, SemanticState};
use super::token_table::VocabularyError;
use super::JsonEvent;

#[derive(Clone, Debug)]
pub struct GuideOptions {
    pub max_rollback_tokens: usize,
    pub limits: GuideLimits,
}

impl Default for GuideOptions {
    fn default() -> Self {
        let limits = GuideLimits::default();
        Self {
            max_rollback_tokens: limits.max_rollback_tokens,
            limits,
        }
    }
}

pub struct Guide {
    pub(crate) compiled: Arc<CompiledSchema>,
    pub(crate) imports: Option<Arc<ImportedMemory>>,
    pub(crate) state: GuideState,
    pub(crate) cursor_journal: CursorJournal,
    pub(crate) semantic_journal: SemanticJournal,
    pub(crate) router_journal: RouterJournal,
    pub(crate) rollback: RollbackWindow,
    pub(crate) revision: u64,
    pub(crate) limits: GuideLimits,
    pub(crate) ledger: ResourceLedger,
    pub(crate) scratch: GuideScratch,
    #[cfg(debug_assertions)]
    pub(crate) committed_trace: Vec<TokenId>,
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailurePoint {
    BeforeCursorMutation,
    AfterCursorMutation,
    AfterFrameAllocation,
    AfterHistoryCreation,
    BeforeHistoryInsert,
    AfterHistoryInsert,
    BeforeArrayClose,
    AfterHistoryClose,
    AfterCounterCreation,
    BeforeContainsEvaluation,
    AfterCounterUpdate,
    AfterRegisterScopeCreation,
    BeforeRelationEvaluation,
    AfterCapturePublication,
    AfterRegisterScopeClose,
    BeforeCheckpointInsert,
    DuringMask,
    DuringRestore,
}

#[cfg(debug_assertions)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuideDebugSnapshot {
    pub dfa_state: StateId,
    pub cursor: super::CursorSnapshot,
    pub arena_lengths: (usize, usize, usize, usize),
    pub router: SemanticRouter,
    pub histories: Vec<(super::HistoryKey, Vec<super::CanonicalId>)>,
    pub counters: Vec<(super::CounterKey, super::ContainsState)>,
    pub registers: Vec<(super::RegisterKey, super::CanonicalId)>,
    pub lifecycle: Lifecycle,
    pub rollback_available: usize,
    pub retained_rollback_bytes: usize,
    pub committed_trace: Vec<TokenId>,
}

#[derive(Default)]
pub(crate) struct GuideScratch {
    pub(crate) events: Vec<JsonEvent>,
    pub(crate) equality: EqualityScratch,
    pub(crate) equality_checks: usize,
    pub(crate) equality_limit: Option<usize>,
    pub(crate) predicate: super::predicate::PredicateScratch,
    #[cfg(debug_assertions)]
    pub(crate) failure: Option<FailurePoint>,
}

impl Guide {
    pub fn new(compiled: Arc<CompiledSchema>, options: GuideOptions) -> Result<Self, GuideError> {
        Self::build(compiled, options, None)
    }

    pub fn new_with_imports(
        compiled: Arc<CompiledSchema>,
        options: GuideOptions,
        imports: Arc<ImportedMemory>,
    ) -> Result<Self, GuideError> {
        Self::build(compiled, options, Some(imports))
    }

    fn build(
        compiled: Arc<CompiledSchema>,
        mut options: GuideOptions,
        imports: Option<Arc<ImportedMemory>>,
    ) -> Result<Self, GuideError> {
        options.limits.max_rollback_tokens = options.max_rollback_tokens;
        let words = super::mask::required_words(compiled.token_table().model_width());
        if words > options.limits.max_mask_words {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_mask_words",
                limit_value: options.limits.max_mask_words,
                requested: words,
            }
            .into());
        }
        for name in compiled.memory_plan().required_imports() {
            if imports.as_ref().is_none_or(|memory| !memory.has_set(name)) {
                return Err(GuideError::MissingImport { name: name.into() });
            }
        }
        let root = compiled.ir().root();
        let state = GuideState {
            dfa_state: compiled.index().initial_state(),
            cursor: super::JsonCursor::new(compiled.runtime_limits().clone()),
            router: SemanticRouter::new(root),
            semantic: SemanticState::default(),
            lifecycle: Lifecycle::Active,
        };
        Ok(Self {
            compiled,
            imports,
            state,
            cursor_journal: CursorJournal::new(options.limits.max_journal_records),
            semantic_journal: SemanticJournal::new(options.limits.max_journal_records),
            router_journal: RouterJournal::new(options.limits.max_journal_records),
            rollback: RollbackWindow::new(options.max_rollback_tokens),
            revision: 0,
            limits: options.limits,
            ledger: ResourceLedger::default(),
            scratch: GuideScratch::default(),
            #[cfg(debug_assertions)]
            committed_trace: Vec::new(),
        })
    }

    pub fn state_id(&self) -> StateId {
        self.state.dfa_state
    }

    pub fn model_width(&self) -> usize {
        self.compiled.token_table().model_width()
    }

    pub fn probe(&mut self, token: TokenId) -> Result<ProbeDecision, GuideError> {
        super::simulation::transition(self, token, super::simulation::TransitionMode::Probe)
    }

    pub fn advance(&mut self, token: TokenId) -> Result<(), GuideError> {
        match super::simulation::transition(self, token, super::simulation::TransitionMode::Commit)?
        {
            ProbeDecision::Allow => Ok(()),
            ProbeDecision::Deny(violation) => Err(GuideError::SemanticRejection(violation)),
        }
    }

    pub fn allowed_tokens(&mut self) -> Result<Vec<TokenId>, GuideError> {
        if self.state.lifecycle == Lifecycle::Terminated {
            return Ok(Vec::new());
        }
        self.require_active("allowed_tokens")?;
        let candidates = self
            .compiled
            .index()
            .allowed_tokens(&self.state.dfa_state)
            .ok_or(GuideError::InternalInvariant {
                context: "DFA state has no candidate row".into(),
            })?;
        if candidates.len() > self.limits.max_candidates_per_mask {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_candidates_per_mask",
                limit_value: self.limits.max_candidates_per_mask,
                requested: candidates.len(),
            }
            .into());
        }
        if self.compiled.memory_plan().is_empty() {
            return Ok(candidates);
        }
        self.scratch.equality_checks = 0;
        self.scratch.equality_limit = Some(self.limits.max_equality_checks_per_mask);
        let result = self.filter_candidates(candidates);
        self.scratch.equality_limit = None;
        result
    }

    fn filter_candidates(&mut self, candidates: Vec<TokenId>) -> Result<Vec<TokenId>, GuideError> {
        let mut allowed = Vec::new();
        allowed
            .try_reserve(candidates.len())
            .map_err(|_| ResourceError::Allocation {
                context: "allowed tokens",
                requested: candidates.len(),
            })?;
        let mut bytes = 0_usize;
        for token in candidates {
            #[cfg(debug_assertions)]
            self.fail_if(FailurePoint::DuringMask)?;
            if token != self.compiled.token_table().eos_token_id() {
                bytes = bytes
                    .checked_add(self.compiled.token_table().get(token)?.bytes().len())
                    .ok_or(ResourceError::ArithmeticOverflow {
                        context: "candidate bytes",
                    })?;
                if bytes > self.limits.max_candidate_bytes_per_mask {
                    return Err(ResourceError::LimitExceeded {
                        limit_name: "max_candidate_bytes_per_mask",
                        limit_value: self.limits.max_candidate_bytes_per_mask,
                        requested: bytes,
                    }
                    .into());
                }
            }
            if self.probe(token)? == ProbeDecision::Allow {
                allowed.push(token);
            }
        }
        Ok(allowed)
    }

    pub fn write_mask(&mut self, mask: &mut [u32]) -> Result<MaskSummary, GuideError> {
        let model_width = self.compiled.token_table().model_width();
        let words = super::mask::required_words(model_width);
        if mask.len() != words {
            return Err(GuideError::InvalidMaskBuffer {
                expected_words: words,
                actual_words: mask.len(),
            });
        }
        mask.fill(0);
        let result = self.allowed_tokens();
        let allowed = match result {
            Ok(allowed) => allowed,
            Err(error) => {
                mask.fill(0);
                return Err(error);
            }
        };
        for token in &allowed {
            if let Err(error) = super::mask::set_token(mask, *token) {
                mask.fill(0);
                return Err(error);
            }
        }
        super::mask::clear_padding(mask, model_width);
        Ok(MaskSummary {
            allowed: allowed.len(),
            words,
        })
    }

    pub fn rollback(&mut self, count: usize) -> Result<(), GuideError> {
        if self.state.lifecycle == Lifecycle::Poisoned {
            return Err(GuideError::Poisoned {
                cause: "guide restoration failed".into(),
            });
        }
        if count > self.rollback.len() {
            return Err(GuideError::RollbackUnavailable {
                requested: count,
                available: self.rollback.len(),
            });
        }
        for _ in 0..count {
            let mark = self.rollback.pop().ok_or(GuideError::InternalInvariant {
                context: "rollback window shortened during rollback".into(),
            })?;
            self.restore(mark)?;
        }
        self.bump_revision()?;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), GuideError> {
        let root = self.compiled.ir().root();
        self.state = GuideState {
            dfa_state: self.compiled.index().initial_state(),
            cursor: super::JsonCursor::new(self.compiled.runtime_limits().clone()),
            router: SemanticRouter::new(root),
            semantic: SemanticState::default(),
            lifecycle: Lifecycle::Active,
        };
        self.cursor_journal = CursorJournal::new(self.limits.max_journal_records);
        self.semantic_journal = SemanticJournal::new(self.limits.max_journal_records);
        self.router_journal = RouterJournal::new(self.limits.max_journal_records);
        self.rollback.clear();
        self.ledger = ResourceLedger::default();
        #[cfg(debug_assertions)]
        self.committed_trace.clear();
        self.bump_revision()
    }

    pub fn is_accepting(&mut self) -> Result<bool, GuideError> {
        match self.state.lifecycle {
            Lifecycle::Terminated => Ok(true),
            Lifecycle::Poisoned => Err(GuideError::Poisoned {
                cause: "guide restoration failed".into(),
            }),
            Lifecycle::Active if !self.compiled.index().is_final_state(&self.state.dfa_state) => {
                Ok(false)
            }
            Lifecycle::Active => {
                Ok(self.probe(self.compiled.token_table().eos_token_id())? == ProbeDecision::Allow)
            }
        }
    }

    pub fn is_terminated(&self) -> bool {
        self.state.lifecycle == Lifecycle::Terminated
    }

    pub fn rollback_available(&self) -> usize {
        self.rollback.len()
    }

    pub fn retained_rollback_bytes(&self) -> usize {
        self.ledger.retained_rollback_bytes
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn inject_failure_once(&mut self, point: FailurePoint) {
        self.scratch.failure = Some(point);
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn debug_snapshot(&self) -> GuideDebugSnapshot {
        GuideDebugSnapshot {
            dfa_state: self.state.dfa_state,
            cursor: self.state.cursor.snapshot(),
            arena_lengths: self.state.cursor.arena().lengths(),
            router: self.state.router.clone(),
            histories: self.state.semantic.histories.logical_snapshot(),
            counters: self.state.semantic.counters.logical_snapshot(),
            registers: self.state.semantic.registers.logical_snapshot(),
            lifecycle: self.state.lifecycle,
            rollback_available: self.rollback.len(),
            retained_rollback_bytes: self.ledger.retained_rollback_bytes,
            committed_trace: self.committed_trace.clone(),
        }
    }

    #[cfg(debug_assertions)]
    pub(crate) fn fail_if(&mut self, point: FailurePoint) -> Result<(), GuideError> {
        if self.scratch.failure == Some(point) {
            self.scratch.failure = None;
            return Err(ResourceError::Allocation {
                context: "injected failure",
                requested: 1,
            }
            .into());
        }
        Ok(())
    }

    pub fn probe_sequence(
        &mut self,
        tokens: &[TokenId],
        finish: bool,
    ) -> Result<SequenceDecision, GuideError> {
        self.require_active("probe_sequence")?;
        let mark = self.mark();
        let result = self.probe_sequence_inner(tokens, finish);
        self.restore(mark)?;
        self.bump_revision()?;
        result
    }

    fn probe_sequence_inner(
        &mut self,
        tokens: &[TokenId],
        finish: bool,
    ) -> Result<SequenceDecision, GuideError> {
        for (token_index, token) in tokens.iter().copied().enumerate() {
            if let ProbeDecision::Deny(violation) =
                super::simulation::transition_uncommitted(self, token)?
            {
                return Ok(SequenceDecision::Deny {
                    token_index,
                    violation,
                });
            }
        }
        if finish && self.state.lifecycle == Lifecycle::Active {
            let eos = self.compiled.token_table().eos_token_id();
            if let ProbeDecision::Deny(violation) =
                super::simulation::transition_uncommitted(self, eos)?
            {
                return Ok(SequenceDecision::Deny {
                    token_index: tokens.len(),
                    violation,
                });
            }
        }
        Ok(SequenceDecision::Allow)
    }

    pub(crate) fn mark(&self) -> TokenMark {
        TokenMark {
            dfa_state: self.state.dfa_state,
            lifecycle: self.state.lifecycle,
            cursor: self.cursor_journal.mark(&self.state.cursor),
            router: self.router_journal.mark(),
            semantic: self.semantic_journal.mark(),
            trace_len: self.trace_len(),
        }
    }

    pub(crate) fn restore(&mut self, mark: TokenMark) -> Result<(), GuideError> {
        #[cfg(debug_assertions)]
        if self.scratch.failure == Some(FailurePoint::DuringRestore) {
            self.scratch.failure = None;
            self.state.lifecycle = Lifecycle::Poisoned;
            return Err(GuideError::Poisoned {
                cause: "injected restoration failure".into(),
            });
        }
        if let Err(error) = self
            .semantic_journal
            .restore_state(mark.semantic, &mut self.state.semantic)
        {
            self.state.lifecycle = Lifecycle::Poisoned;
            return Err(error.into());
        }
        if let Err(error) = self
            .router_journal
            .restore(mark.router, &mut self.state.router)
        {
            self.state.lifecycle = Lifecycle::Poisoned;
            return Err(error.into());
        }
        if let Err(error) = self.cursor_journal_restore(mark.cursor) {
            self.state.lifecycle = Lifecycle::Poisoned;
            return Err(error);
        }
        self.state.dfa_state = mark.dfa_state;
        self.state.lifecycle = mark.lifecycle;
        #[cfg(debug_assertions)]
        self.committed_trace.truncate(mark.trace_len);
        if let Err(error) = self.refresh_retained_rollback_bytes() {
            self.state.lifecycle = Lifecycle::Poisoned;
            return Err(error);
        }
        Ok(())
    }

    fn cursor_journal_restore(&mut self, mark: super::CursorJournalMark) -> Result<(), GuideError> {
        self.state.cursor.restore(mark, &mut self.cursor_journal)?;
        Ok(())
    }

    pub(crate) fn bump_revision(&mut self) -> Result<(), GuideError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(GuideError::InternalInvariant {
                context: "guide revision exhausted".into(),
            })?;
        Ok(())
    }

    pub(crate) fn require_active(&self, operation: &'static str) -> Result<(), GuideError> {
        if self.state.lifecycle != Lifecycle::Active {
            return Err(GuideError::InvalidLifecycle {
                state: self.state.lifecycle,
                operation,
            });
        }
        Ok(())
    }

    fn trace_len(&self) -> usize {
        #[cfg(debug_assertions)]
        return self.committed_trace.len();
        #[cfg(not(debug_assertions))]
        return 0;
    }

    pub(crate) fn retained_bytes_after_push(&self, mark: TokenMark) -> Result<usize, GuideError> {
        let Some(start) = self.rollback.retained_start_after_push(mark) else {
            return Ok(0);
        };
        self.retained_bytes_from(
            start.cursor.undo_len(),
            start.router.undo_len(),
            start.semantic.undo_len(),
            self.rollback.retained_len_after_push(),
        )
    }

    pub(crate) fn refresh_retained_rollback_bytes(&mut self) -> Result<(), GuideError> {
        self.ledger.retained_rollback_bytes = if self.rollback.is_empty() {
            0
        } else {
            self.retained_bytes_from(0, 0, 0, self.rollback.len())?
        };
        Ok(())
    }

    fn retained_bytes_from(
        &self,
        cursor_start: usize,
        router_start: usize,
        semantic_start: usize,
        checkpoints: usize,
    ) -> Result<usize, GuideError> {
        let cursor = self.cursor_journal.retained_bytes_from(cursor_start)?;
        let router = self.router_journal.retained_bytes_from(router_start)?;
        let semantic = self.semantic_journal.retained_bytes_from(semantic_start)?;
        let checkpoint_bytes = checkpoints
            .checked_mul(std::mem::size_of::<TokenMark>())
            .ok_or(ResourceError::ArithmeticOverflow {
                context: "rollback checkpoint bytes",
            })?;
        cursor
            .checked_add(router)
            .and_then(|bytes| bytes.checked_add(semantic))
            .and_then(|bytes| bytes.checked_add(checkpoint_bytes))
            .ok_or(ResourceError::ArithmeticOverflow {
                context: "retained rollback bytes",
            })
            .map_err(Into::into)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProbeDecision {
    Allow,
    Deny(SemanticViolation),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticViolation {
    DuplicateArrayItem {
        frame: super::FrameId,
        constraint: super::ConstraintId,
        item_index: u64,
    },
    ContainsMinimumNotMet {
        frame: super::FrameId,
        constraint: super::ConstraintId,
        processed: u64,
        matched: u64,
        lower: u64,
    },
    ContainsMaximumExceeded {
        frame: super::FrameId,
        constraint: super::ConstraintId,
        item_index: u64,
        processed: u64,
        matched: u64,
        upper: u64,
    },
    ContainsMinimumUnreachable {
        frame: super::FrameId,
        constraint: super::ConstraintId,
        item_index: u64,
        processed: u64,
        matched: u64,
        lower: u64,
    },
    MissingCapture {
        object: super::FrameId,
        property: Box<str>,
        capture: Box<str>,
    },
    EqualityMismatch {
        object: super::FrameId,
        property: Box<str>,
        capture: Box<str>,
    },
    InequalityMismatch {
        object: super::FrameId,
        property: Box<str>,
        capture: Box<str>,
    },
    ImportedMemberRequired {
        object: super::FrameId,
        property: Box<str>,
        import_set: Box<str>,
    },
    ImportedMemberForbidden {
        object: super::FrameId,
        property: Box<str>,
        import_set: Box<str>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SequenceDecision {
    Allow,
    Deny {
        token_index: usize,
        violation: SemanticViolation,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaskSummary {
    pub allowed: usize,
    pub words: usize,
}

#[derive(Debug, Error)]
pub enum GuideError {
    #[error("operation {operation} is invalid while guide is {state:?}")]
    InvalidLifecycle {
        state: Lifecycle,
        operation: &'static str,
    },
    #[error("unknown token {token_id}")]
    UnknownToken { token_id: TokenId },
    #[error("token {token_id} is structurally rejected from state {state}")]
    StructuralRejection { state: StateId, token_id: TokenId },
    #[error("semantic rejection: {0:?}")]
    SemanticRejection(SemanticViolation),
    #[error("invalid mask buffer: expected {expected_words} words, got {actual_words}")]
    InvalidMaskBuffer {
        expected_words: usize,
        actual_words: usize,
    },
    #[error("cannot roll back {requested} tokens; {available} available")]
    RollbackUnavailable { requested: usize, available: usize },
    #[error(transparent)]
    Cursor(#[from] CursorError),
    #[error(transparent)]
    Arena(#[from] ArenaError),
    #[error(transparent)]
    Vocabulary(#[from] VocabularyError),
    #[error(transparent)]
    History(#[from] HistoryError),
    #[error(transparent)]
    Counter(#[from] CounterError),
    #[error(transparent)]
    Register(#[from] RegisterError),
    #[error(transparent)]
    Import(#[from] ImportError),
    #[error("required imported set {name} was not supplied")]
    MissingImport { name: Box<str> },
    #[error(transparent)]
    Router(#[from] RouterError),
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Rollback(#[from] RollbackError),
    #[error(transparent)]
    Resource(#[from] ResourceError),
    #[error("internal guide invariant failed: {context}")]
    InternalInvariant { context: Box<str> },
    #[error("guide is poisoned: {cause}")]
    Poisoned { cause: Box<str> },
}
