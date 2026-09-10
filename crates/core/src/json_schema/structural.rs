use serde_json::{Map, Value};

use crate::sidememory::number::CanonicalNumber;

use super::diagnostic::CompileError;
use super::ir::{InstanceType, ScalarLiteral, SchemaIr, SchemaNodeId};

/// Digit ceiling for expanding a numeric literal to plain decimal; past this the
/// grammar falls back to the exact scientific spelling.
const MAX_LITERAL_DIGITS: usize = 4096;

pub(crate) fn lower(ir: &SchemaIr) -> Result<Value, CompileError> {
    build(ir, ir.root())
}

fn build(ir: &SchemaIr, id: SchemaNodeId) -> Result<Value, CompileError> {
    let node = ir.node(id).ok_or_else(|| CompileError::InternalInvariant {
        context: format!("missing schema node {}", id.get()).into_boxed_str(),
    })?;
    if node.instance_type == InstanceType::Any
        && node.scalar.const_value.is_none()
        && node.scalar.enum_values.is_empty()
    {
        return Ok(Value::Object(Map::new()));
    }
    let mut object = Map::new();
    if node.instance_type != InstanceType::Any {
        object.insert(
            "type".into(),
            Value::String(type_name(node.instance_type).into()),
        );
    }
    if let Some(value) = &node.scalar.const_value {
        object.insert("const".into(), literal(value)?);
    }
    if !node.scalar.enum_values.is_empty() {
        object.insert(
            "enum".into(),
            Value::Array(
                node.scalar
                    .enum_values
                    .iter()
                    .map(literal)
                    .collect::<Result<_, _>>()?,
            ),
        );
    }
    if let Some(array) = &node.array {
        object.insert("items".into(), build(ir, array.items)?);
        object.insert("minItems".into(), Value::from(array.min_items));
        if let Some(max_items) = array.max_items {
            object.insert("maxItems".into(), Value::from(max_items));
        }
    }
    if let Some(shape) = &node.object {
        let mut properties = Map::new();
        for (name, child) in &shape.properties {
            properties.insert(name.to_string(), build(ir, *child)?);
        }
        object.insert("properties".into(), Value::Object(properties));
        object.insert(
            "required".into(),
            Value::Array(
                shape
                    .required
                    .iter()
                    .map(|name| Value::String(name.to_string()))
                    .collect(),
            ),
        );
        object.insert(
            "additionalProperties".into(),
            Value::Bool(shape.additional_properties),
        );
        if !shape.fixed_property_order {
            object.insert("__oc_unordered_required".into(), Value::Bool(true));
        }
    }
    Ok(Value::Object(object))
}

fn type_name(value: InstanceType) -> &'static str {
    match value {
        InstanceType::Any => unreachable!("Any handled before type lowering"),
        InstanceType::Null => "null",
        InstanceType::Boolean => "boolean",
        InstanceType::Integer => "integer",
        InstanceType::Number => "number",
        InstanceType::String => "string",
        InstanceType::Array => "array",
        InstanceType::Object => "object",
    }
}

fn literal(value: &ScalarLiteral) -> Result<Value, CompileError> {
    match value {
        ScalarLiteral::Null => Ok(Value::Null),
        ScalarLiteral::Bool(value) => Ok(Value::Bool(*value)),
        ScalarLiteral::String(value) => Ok(Value::String(value.to_string())),
        ScalarLiteral::Number(value) => {
            let text = number_to_json(value);
            serde_json::from_str(&text).map_err(|error| CompileError::InternalInvariant {
                context: format!("canonical number could not be lowered: {error}").into_boxed_str(),
            })
        }
    }
}

/// Render a canonical number as the exact JSON spelling the grammar accepts:
/// plain decimal when the expansion is bounded, exact scientific otherwise.
fn number_to_json(number: &CanonicalNumber) -> String {
    let coefficient = number.coefficient();
    let sign = if number.is_negative() { "-" } else { "" };
    if coefficient == "0" {
        return "0".to_string();
    }
    let digits = coefficient.len();
    let Ok(exponent) = number.exponent().parse::<i64>() else {
        return format!("{sign}{coefficient}e{}", number.exponent());
    };
    if exponent >= 0 {
        let Ok(zeros) = usize::try_from(exponent) else {
            return format!("{sign}{coefficient}e{exponent}");
        };
        if digits
            .checked_add(zeros)
            .is_none_or(|total| total > MAX_LITERAL_DIGITS)
        {
            return format!("{sign}{coefficient}e{exponent}");
        }
        format!("{sign}{coefficient}{}", "0".repeat(zeros))
    } else {
        let Ok(shift) = usize::try_from(exponent.unsigned_abs()) else {
            return format!("{sign}{coefficient}e{exponent}");
        };
        if shift < digits {
            let point = digits - shift;
            format!("{sign}{}.{}", &coefficient[..point], &coefficient[point..])
        } else if shift > MAX_LITERAL_DIGITS {
            format!("{sign}{coefficient}e{exponent}")
        } else {
            format!("{sign}0.{}{coefficient}", "0".repeat(shift - digits))
        }
    }
}
