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
