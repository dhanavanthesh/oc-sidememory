use serde_json::{Map, Value};

use super::diagnostic::CompileError;
use super::ir::{InstanceType, ScalarLiteral, SchemaIr, SchemaNode, SchemaNodeId};

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
            let sign = if value.is_negative() { "-" } else { "" };
            let text = format!("{}{}e{}", sign, value.coefficient(), value.exponent());
            serde_json::from_str(&text).map_err(|error| CompileError::InternalInvariant {
                context: format!("canonical number could not be lowered: {error}").into_boxed_str(),
            })
        }
    }
}

#[allow(dead_code)]
fn _location(node: &SchemaNode) -> &str {
    node.schema_location.as_str()
}
