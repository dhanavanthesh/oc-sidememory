use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileLimits {
    pub max_schema_bytes: usize,
    pub max_schema_nodes: usize,
    pub max_schema_depth: usize,
    pub max_regex_bytes: usize,
    pub max_model_width: usize,
    pub max_total_token_bytes: usize,
    pub max_token_table_bytes: usize,
}

impl Default for CompileLimits {
    fn default() -> Self {
        Self {
            max_schema_bytes: 1 << 20,
            max_schema_nodes: 16_384,
            max_schema_depth: 128,
            max_regex_bytes: 8 << 20,
            max_model_width: 1_000_000,
            max_total_token_bytes: 64 << 20,
            max_token_table_bytes: 128 << 20,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeLimits {
    pub max_nesting_depth: usize,
    pub max_value_bytes: usize,
    pub max_arena_bytes: usize,
    pub max_number_digits: usize,
    pub max_exponent_digits: usize,
    pub max_arena_nodes: usize,
    pub max_array_items: usize,
    pub max_object_members: usize,
    pub max_events_per_byte: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuideLimits {
    pub max_rollback_tokens: usize,
    pub max_journal_records: usize,
    pub max_history_items: usize,
    pub max_histories: usize,
    pub max_history_bucket_entries: usize,
    pub max_retained_rollback_bytes: usize,
    pub max_mask_words: usize,
    pub max_candidates_per_mask: usize,
    pub max_candidate_bytes_per_mask: usize,
    pub max_equality_checks_per_mask: usize,
    pub max_debug_trace_tokens: usize,
    pub max_active_counters: usize,
    pub max_predicate_steps: usize,
    pub max_predicate_depth: usize,
    pub max_predicate_equality_checks: usize,
    pub max_register_scopes: usize,
    pub max_register_values: usize,
}

impl Default for GuideLimits {
    fn default() -> Self {
        Self {
            max_rollback_tokens: 32,
            max_journal_records: 1_000_000,
            max_history_items: 1_000_000,
            max_histories: 4096,
            max_history_bucket_entries: 1_000_000,
            max_retained_rollback_bytes: 64 << 20,
            max_mask_words: 1_000_000_usize.div_ceil(32),
            max_candidates_per_mask: 1_000_000,
            max_candidate_bytes_per_mask: 64 << 20,
            max_equality_checks_per_mask: 1_000_000,
            max_debug_trace_tokens: 1_000_000,
            max_active_counters: 4096,
            max_predicate_steps: 1_000_000,
            max_predicate_depth: 128,
            max_predicate_equality_checks: 1_000_000,
            max_register_scopes: 4096,
            max_register_values: 65_536,
        }
    }
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            max_nesting_depth: 128,
            max_value_bytes: 8 << 20,
            max_arena_bytes: 64 << 20,
            max_number_digits: 1 << 20,
            max_exponent_digits: 4096,
            max_arena_nodes: 1_000_000,
            max_array_items: 1_000_000,
            max_object_members: 1_000_000,
            max_events_per_byte: 64,
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ResourceError {
    #[error("resource limit {limit_name}={limit_value} exceeded; requested {requested}")]
    LimitExceeded {
        limit_name: &'static str,
        limit_value: usize,
        requested: usize,
    },
    #[error("arithmetic overflow while charging {context}")]
    ArithmeticOverflow { context: &'static str },
    #[error("allocation failed while reserving {requested} units for {context}")]
    Allocation {
        context: &'static str,
        requested: usize,
    },
    #[error("value cannot be represented by the compact arena: {context}")]
    CompactIdOverflow { context: &'static str },
}

pub(crate) fn checked_growth(
    current: usize,
    additional: usize,
    limit: usize,
    limit_name: &'static str,
) -> Result<usize, ResourceError> {
    let requested = current
        .checked_add(additional)
        .ok_or(ResourceError::ArithmeticOverflow {
            context: limit_name,
        })?;
    if requested > limit {
        return Err(ResourceError::LimitExceeded {
            limit_name,
            limit_value: limit,
            requested,
        });
    }
    Ok(requested)
}

pub(crate) fn reserve<T>(
    values: &mut Vec<T>,
    additional: usize,
    context: &'static str,
) -> Result<(), ResourceError> {
    values
        .try_reserve(additional)
        .map_err(|_| ResourceError::Allocation {
            context,
            requested: additional,
        })
}
