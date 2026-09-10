use crate::json_schema::ir::{InstanceType, ScalarLiteral, SchemaIr, SchemaNode, SchemaNodeId};

use super::canonical::{ArenaError, CanonicalArena, CanonicalNode, EqualityScratch};
use super::guide::GuideError;
use super::limits::{GuideLimits, ResourceError};
use super::value::CanonicalId;

#[derive(Debug, Default)]
pub struct PredicateScratch {
    equality: EqualityScratch,
    steps: usize,
    equality_checks: usize,
}

pub(crate) fn validate_predicate(
    predicate: SchemaNodeId,
    ir: &SchemaIr,
    arena: &CanonicalArena,
    value: CanonicalId,
    scratch: &mut PredicateScratch,
    limits: &GuideLimits,
) -> Result<bool, GuideError> {
    scratch.steps = 0;
    scratch.equality_checks = 0;
    validate_node(
        predicate,
        ir,
        arena,
        value,
        scratch,
        0,
        limits.max_predicate_steps,
        limits.max_predicate_depth,
        limits.max_predicate_equality_checks,
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_node(
    node_id: SchemaNodeId,
    ir: &SchemaIr,
    arena: &CanonicalArena,
    value: CanonicalId,
    scratch: &mut PredicateScratch,
    depth: usize,
    max_steps: usize,
    max_depth: usize,
    max_equality_checks: usize,
) -> Result<bool, GuideError> {
    consume(&mut scratch.steps, max_steps, "max_predicate_steps")?;
    if depth > max_depth {
        return Err(ResourceError::LimitExceeded {
            limit_name: "max_predicate_depth",
            limit_value: max_depth,
            requested: depth,
        }
        .into());
    }
    let node = ir.node(node_id).ok_or(GuideError::InternalInvariant {
        context: format!("missing predicate node {}", node_id.get()).into_boxed_str(),
    })?;
    if node.always_false {
        return Ok(false);
    }
    let canonical = arena.node(value)?;
    if !matches_type(node.instance_type, canonical) {
        return Ok(false);
    }
    if !matches_scalars(node, arena, canonical)? {
        return Ok(false);
    }
    if let Some(array) = &node.array {
        let CanonicalNode::Array(span) = canonical else {
            return Ok(false);
        };
        let values = arena.array_values(*span)?;
        let len = u64::try_from(values.len()).map_err(|_| ResourceError::CompactIdOverflow {
            context: "predicate array length",
        })?;
        if len < array.min_items || array.max_items.is_some_and(|maximum| len > maximum) {
            return Ok(false);
        }
        for child in values {
            if !validate_node(
                array.items,
                ir,
                arena,
                *child,
                scratch,
                depth + 1,
                max_steps,
                max_depth,
                max_equality_checks,
            )? {
                return Ok(false);
            }
        }
        if node.semantic.iter().any(|assertion| {
            matches!(
                assertion,
                crate::json_schema::ir::SemanticAssertion::UniqueItems
            )
        }) {
            for left in 0..values.len() {
                for right in left + 1..values.len() {
                    consume(
                        &mut scratch.equality_checks,
                        max_equality_checks,
                        "max_predicate_equality_checks",
                    )?;
                    if arena.equal_with_scratch(
                        values[left],
                        values[right],
                        &mut scratch.equality,
                    )? {
                        return Ok(false);
                    }
                }
            }
        }
    }
    if let Some(object) = &node.object {
        let CanonicalNode::Object(span) = canonical else {
            return Ok(false);
        };
        let entries = arena.object_entries(*span)?;
        for required in &object.required {
            let found = entries.iter().try_fold(false, |found, entry| {
                Ok::<_, ArenaError>(found || arena.object_key(*entry)? == required.as_bytes())
            })?;
            if !found {
                return Ok(false);
            }
        }
        for entry in entries {
            let key = arena.object_key(*entry)?;
            let Some((_, child)) = object
                .properties
                .iter()
                .find(|(name, _)| name.as_bytes() == key)
            else {
                return Ok(false);
            };
            if !validate_node(
                *child,
                ir,
                arena,
                arena.object_value(*entry),
                scratch,
                depth + 1,
                max_steps,
                max_depth,
                max_equality_checks,
            )? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn matches_type(expected: InstanceType, value: &CanonicalNode) -> bool {
    match (expected, value) {
        (InstanceType::Any, _) => true,
        (InstanceType::Null, CanonicalNode::Null)
        | (InstanceType::Boolean, CanonicalNode::Bool(_))
        | (InstanceType::Number, CanonicalNode::Number(_))
        | (InstanceType::String, CanonicalNode::String(_))
        | (InstanceType::Array, CanonicalNode::Array(_))
        | (InstanceType::Object, CanonicalNode::Object(_)) => true,
        (InstanceType::Integer, CanonicalNode::Number(number)) => number.is_integer(),
        _ => false,
    }
}

fn matches_scalars(
    node: &SchemaNode,
    arena: &CanonicalArena,
    value: &CanonicalNode,
) -> Result<bool, ArenaError> {
    if let Some(expected) = &node.scalar.const_value {
        return scalar_matches(expected, arena, value);
    }
    if node.scalar.enum_values.is_empty() {
        return Ok(true);
    }
    for expected in &node.scalar.enum_values {
        if scalar_matches(expected, arena, value)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn scalar_matches(
    expected: &ScalarLiteral,
    arena: &CanonicalArena,
    value: &CanonicalNode,
) -> Result<bool, ArenaError> {
    match (expected, value) {
        (ScalarLiteral::Null, CanonicalNode::Null) => Ok(true),
        (ScalarLiteral::Bool(left), CanonicalNode::Bool(right)) => Ok(left == right),
        (ScalarLiteral::Number(left), CanonicalNode::Number(right)) => Ok(left == right),
        (ScalarLiteral::String(left), CanonicalNode::String(right)) => {
            Ok(left.as_bytes() == arena.string_bytes(*right)?)
        }
        _ => Ok(false),
    }
}

fn consume(used: &mut usize, limit: usize, limit_name: &'static str) -> Result<(), ResourceError> {
    *used = used
        .checked_add(1)
        .ok_or(ResourceError::ArithmeticOverflow {
            context: limit_name,
        })?;
    if *used > limit {
        return Err(ResourceError::LimitExceeded {
            limit_name,
            limit_value: limit,
            requested: *used,
        });
    }
    Ok(())
}
