use std::collections::HashSet;

use serde_json::Value;

use crate::index::Index;
use crate::sidememory::limits::{CompileLimits, ResourceError, RuntimeLimits};
use crate::sidememory::number::{CanonicalNumber, NumberError};
use crate::sidememory::plan::MemoryPlan;
use crate::sidememory::token_table::TokenTable;
use crate::vocabulary::Vocabulary;

use super::diagnostic::{CompileError, SchemaLocation};
use super::ir::{
    ArrayAssertions, InstanceType, ObjectAssertions, ScalarAssertions, ScalarLiteral, SchemaIr,
    SchemaNode, SchemaNodeId, SemanticAssertion,
};
use super::profile::{CheckedProfile, Dialect, UnknownKeywordPolicy};
use super::{legacy, memory_keywords, structural};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerializationPolicy {
    LegacyCompatible,
}

#[derive(Clone, Debug)]
pub struct CompileOptions {
    pub profile: CheckedProfile,
    pub compile_limits: CompileLimits,
    pub runtime_limits: RuntimeLimits,
    pub serialization_policy: SerializationPolicy,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            profile: CheckedProfile::default(),
            compile_limits: CompileLimits::default(),
            runtime_limits: RuntimeLimits::default(),
            serialization_policy: SerializationPolicy::LegacyCompatible,
        }
    }
}

#[derive(Debug)]
pub struct CompiledSchema {
    index: Index,
    memory_plan: MemoryPlan,
    token_table: TokenTable,
    ir: SchemaIr,
    regular_plan: Box<str>,
    serialization_policy: SerializationPolicy,
}

impl CompiledSchema {
    pub fn index(&self) -> &Index {
        &self.index
    }

    pub fn memory_plan(&self) -> &MemoryPlan {
        &self.memory_plan
    }

    pub fn token_table(&self) -> &TokenTable {
        &self.token_table
    }

    pub fn ir(&self) -> &SchemaIr {
        &self.ir
    }

    pub fn regular_plan(&self) -> &str {
        &self.regular_plan
    }

    pub fn serialization_policy(&self) -> SerializationPolicy {
        self.serialization_policy
    }
}

pub fn compile_schema(
    schema_bytes: &[u8],
    vocabulary: &Vocabulary,
    model_width: usize,
    options: &CompileOptions,
) -> Result<CompiledSchema, CompileError> {
    let ir = compile_ir(schema_bytes, options)?;
    let structural_schema = structural::lower(&ir)?;
    let mut parser = legacy::Parser::new(&structural_schema)
        .with_max_recursion_depth(options.profile.max_schema_depth as usize);
    let regular_plan =
        parser
            .to_regex(&structural_schema)
            .map_err(|error| CompileError::StructuralLowering {
                location: SchemaLocation::root(),
                cause: error.to_string().into_boxed_str(),
            })?;
    if regular_plan.len() > options.compile_limits.max_regex_bytes {
        return Err(ResourceError::LimitExceeded {
            limit_name: "max_regex_bytes",
            limit_value: options.compile_limits.max_regex_bytes,
            requested: regular_plan.len(),
        }
        .into());
    }
    let memory_plan =
        memory_keywords::lower(&ir).map_err(|context| CompileError::InternalInvariant {
            context: context.into(),
        })?;
    let token_table = TokenTable::build(vocabulary, model_width, &options.compile_limits)?;
    let index = Index::new(&regular_plan, vocabulary).map_err(|error| {
        CompileError::StructuralLowering {
            location: SchemaLocation::root(),
            cause: error.to_string().into_boxed_str(),
        }
    })?;
    Ok(CompiledSchema {
        index,
        memory_plan,
        token_table,
        ir,
        regular_plan: regular_plan.into_boxed_str(),
        serialization_policy: options.serialization_policy,
    })
}

pub fn compile_ir(schema_bytes: &[u8], options: &CompileOptions) -> Result<SchemaIr, CompileError> {
    if schema_bytes.len() > options.compile_limits.max_schema_bytes {
        return Err(ResourceError::LimitExceeded {
            limit_name: "max_schema_bytes",
            limit_value: options.compile_limits.max_schema_bytes,
            requested: schema_bytes.len(),
        }
        .into());
    }
    let raw_schema: Value =
        serde_json::from_slice(schema_bytes).map_err(|error| CompileError::MalformedSchema {
            location: SchemaLocation::root(),
            cause: error.to_string().into_boxed_str(),
        })?;
    check_dialect(&raw_schema, options.profile.dialect)?;
    let mut builder = IrBuilder::new(options);
    let root = builder.build_node(&raw_schema, SchemaLocation::root(), 0)?;
    let ir = SchemaIr {
        root,
        nodes: builder.nodes,
        dialect: options.profile.dialect,
    };
    Ok(ir)
}

struct IrBuilder<'a> {
    options: &'a CompileOptions,
    nodes: Vec<SchemaNode>,
}

impl<'a> IrBuilder<'a> {
    fn new(options: &'a CompileOptions) -> Self {
        Self {
            options,
            nodes: Vec::new(),
        }
    }

    fn build_node(
        &mut self,
        raw: &Value,
        location: SchemaLocation,
        depth: usize,
    ) -> Result<SchemaNodeId, CompileError> {
        let depth_limit = self
            .options
            .compile_limits
            .max_schema_depth
            .min(self.options.profile.max_schema_depth as usize);
        if depth > depth_limit {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_schema_depth",
                limit_value: depth_limit,
                requested: depth,
            }
            .into());
        }
        if raw == &Value::Bool(false) {
            return Err(CompileError::UnsatisfiableSchema {
                location,
                reason: "boolean false rejects every instance".into(),
            });
        }
        if raw == &Value::Bool(true) {
            return self.push_node(SchemaNode {
                id: SchemaNodeId(0),
                schema_location: location,
                instance_type: InstanceType::Any,
                scalar: ScalarAssertions::default(),
                array: None,
                object: None,
                semantic: Vec::new(),
            });
        }
        let object = raw
            .as_object()
            .ok_or_else(|| CompileError::MalformedSchema {
                location: location.clone(),
                cause: "schema must be a boolean or object".into(),
            })?;
        self.check_keywords(object, &location)?;
        validate_keyword_values(object, &location)?;
        let instance_type = parse_type(object.get("type"), &location)?;
        validate_applicability(object, instance_type, &location)?;
        let scalar = self.scalar_assertions(object, instance_type, &location)?;
        let mut semantic = Vec::new();
        let array = if instance_type == InstanceType::Array {
            let items = object.get("items").unwrap_or(&Value::Bool(true));
            let items = self.build_node(items, location.child("items"), depth + 1)?;
            let min_items = keyword_u64(object, "minItems", 0, &location)?;
            let max_items = optional_u64(object, "maxItems", &location)?;
            if max_items.is_some_and(|maximum| min_items > maximum) {
                return Err(CompileError::UnsatisfiableSchema {
                    location: location.clone(),
                    reason: "minItems exceeds maxItems".into(),
                });
            }
            if keyword_bool(object, "uniqueItems", false, &location)? {
                semantic.push(SemanticAssertion::UniqueItems);
            }
            Some(ArrayAssertions {
                items,
                min_items,
                max_items,
            })
        } else {
            if object.get("uniqueItems") == Some(&Value::Bool(true)) {
                return Err(unsupported(
                    &location,
                    "uniqueItems",
                    "checked profile requires explicit array applicability",
                ));
            }
            None
        };
        let object_assertions = if instance_type == InstanceType::Object {
            Some(self.object_assertions(object, &location, depth)?)
        } else {
            None
        };
        self.push_node(SchemaNode {
            id: SchemaNodeId(0),
            schema_location: location,
            instance_type,
            scalar,
            array,
            object: object_assertions,
            semantic,
        })
    }

    fn push_node(&mut self, mut node: SchemaNode) -> Result<SchemaNodeId, CompileError> {
        if self.nodes.len() >= self.options.compile_limits.max_schema_nodes {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_schema_nodes",
                limit_value: self.options.compile_limits.max_schema_nodes,
                requested: self.nodes.len().saturating_add(1),
            }
            .into());
        }
        self.nodes
            .try_reserve(1)
            .map_err(|_| ResourceError::Allocation {
                context: "schema IR nodes",
                requested: 1,
            })?;
        let id = SchemaNodeId(u32::try_from(self.nodes.len()).map_err(|_| {
            ResourceError::CompactIdOverflow {
                context: "schema node",
            }
        })?);
        node.id = id;
        self.nodes.push(node);
        Ok(id)
    }

    fn check_keywords(
        &self,
        object: &serde_json::Map<String, Value>,
        location: &SchemaLocation,
    ) -> Result<(), CompileError> {
        for keyword in object.keys() {
            if SUPPORTED.contains(&keyword.as_str()) {
                continue;
            }
            if ANNOTATIONS.contains(&keyword.as_str())
                && self.options.profile.allow_annotations
                && self.options.profile.unknown_keyword_policy
                    == UnknownKeywordPolicy::AllowKnownAnnotations
            {
                continue;
            }
            let reason = if UNSUPPORTED_STANDARD.contains(&keyword.as_str()) {
                "standard keyword is outside the checked profile"
            } else {
                "unknown extension keyword is rejected by profile"
            };
            return Err(unsupported(location, keyword, reason));
        }
        Ok(())
    }

    fn scalar_assertions(
        &self,
        object: &serde_json::Map<String, Value>,
        instance_type: InstanceType,
        location: &SchemaLocation,
    ) -> Result<ScalarAssertions, CompileError> {
        let const_value = object
            .get("const")
            .map(|value| scalar_literal(value, &self.options.runtime_limits, location, "const"))
            .transpose()?;
        let enum_values = if let Some(raw) = object.get("enum") {
            let values = raw
                .as_array()
                .ok_or_else(|| invalid(location, "enum", "a non-empty array"))?;
            if values.is_empty() {
                return Err(CompileError::UnsatisfiableSchema {
                    location: location.clone(),
                    reason: "enum is empty".into(),
                });
            }
            let mut result = Vec::with_capacity(values.len());
            for value in values {
                let value = scalar_literal(value, &self.options.runtime_limits, location, "enum")?;
                if result.contains(&value) {
                    return Err(invalid(
                        location,
                        "enum",
                        "mathematically distinct scalar values",
                    ));
                }
                result.push(value);
            }
            result
        } else {
            Vec::new()
        };
        if let Some(value) = &const_value {
            ensure_compatible(instance_type, value, location, "const")?;
        }
        for value in &enum_values {
            ensure_compatible(instance_type, value, location, "enum")?;
        }
        Ok(ScalarAssertions {
            const_value,
            enum_values,
        })
    }

    fn object_assertions(
        &mut self,
        object: &serde_json::Map<String, Value>,
        location: &SchemaLocation,
        depth: usize,
    ) -> Result<ObjectAssertions, CompileError> {
        let raw_properties = object
            .get("properties")
            .ok_or_else(|| {
                invalid(
                    location,
                    "properties",
                    "an object for explicit object shapes",
                )
            })?
            .as_object()
            .ok_or_else(|| invalid(location, "properties", "an object"))?;
        let mut properties = Vec::with_capacity(raw_properties.len());
        for (name, schema) in raw_properties {
            let child =
                self.build_node(schema, location.child("properties").child(name), depth + 1)?;
            properties.push((name.clone().into_boxed_str(), child));
        }
        let required = match object.get("required") {
            None => Vec::new(),
            Some(Value::Array(values)) => {
                let mut required: Vec<Box<str>> = Vec::with_capacity(values.len());
                for value in values {
                    let name = value.as_str().ok_or_else(|| {
                        invalid(location, "required", "an array of unique strings")
                    })?;
                    if required.iter().any(|existing| existing.as_ref() == name) {
                        return Err(invalid(location, "required", "an array of unique strings"));
                    }
                    if !raw_properties.contains_key(name) {
                        return Err(invalid(
                            location,
                            "required",
                            "names declared in properties",
                        ));
                    }
                    required.push(name.into());
                }
                required
            }
            Some(_) => return Err(invalid(location, "required", "an array of unique strings")),
        };
        match object.get("additionalProperties") {
            Some(Value::Bool(false)) => {}
            _ => return Err(invalid(location, "additionalProperties", "false")),
        }
        Ok(ObjectAssertions {
            properties,
            required,
            additional_properties: false,
        })
    }
}

fn check_dialect(raw: &Value, dialect: Dialect) -> Result<(), CompileError> {
    let Some(value) = raw.as_object().and_then(|object| object.get("$schema")) else {
        return Ok(());
    };
    let Some(value) = value.as_str() else {
        return Err(invalid(&SchemaLocation::root(), "$schema", "a URI string"));
    };
    if dialect == Dialect::Draft202012
        && !matches!(
            value,
            "https://json-schema.org/draft/2020-12/schema"
                | "https://json-schema.org/draft/2020-12/schema#"
        )
    {
        return Err(CompileError::UnsupportedDialect {
            dialect: value.into(),
        });
    }
    Ok(())
}

fn parse_type(
    value: Option<&Value>,
    location: &SchemaLocation,
) -> Result<InstanceType, CompileError> {
    let Some(value) = value else {
        return Ok(InstanceType::Any);
    };
    let Some(value) = value.as_str() else {
        return Err(invalid(location, "type", "one explicit type string"));
    };
    match value {
        "null" => Ok(InstanceType::Null),
        "boolean" => Ok(InstanceType::Boolean),
        "integer" => Ok(InstanceType::Integer),
        "number" => Ok(InstanceType::Number),
        "string" => Ok(InstanceType::String),
        "array" => Ok(InstanceType::Array),
        "object" => Ok(InstanceType::Object),
        _ => Err(invalid(location, "type", "a supported JSON type")),
    }
}

fn scalar_literal(
    value: &Value,
    limits: &RuntimeLimits,
    location: &SchemaLocation,
    keyword: &str,
) -> Result<ScalarLiteral, CompileError> {
    match value {
        Value::Null => Ok(ScalarLiteral::Null),
        Value::Bool(value) => Ok(ScalarLiteral::Bool(*value)),
        Value::String(value) => Ok(ScalarLiteral::String(value.clone().into_boxed_str())),
        Value::Number(value) => CanonicalNumber::parse(value.to_string().as_bytes(), limits)
            .map(ScalarLiteral::Number)
            .map_err(|error| number_error(location, keyword, error)),
        Value::Array(_) | Value::Object(_) => Err(unsupported(
            location,
            keyword,
            "composite const/enum is deferred until the checked compiler exposes its canonical literal arena",
        )),
    }
}

fn number_error(location: &SchemaLocation, keyword: &str, error: NumberError) -> CompileError {
    match error {
        NumberError::InvalidGrammar => invalid(location, keyword, "an exact JSON number"),
        NumberError::LimitExceeded {
            limit_name,
            limit_value,
            requested,
        } => CompileError::ResourceLimit(ResourceError::LimitExceeded {
            limit_name,
            limit_value,
            requested,
        }),
        NumberError::Allocation { context, requested } => {
            CompileError::ResourceLimit(ResourceError::Allocation { context, requested })
        }
    }
}

fn ensure_compatible(
    instance_type: InstanceType,
    value: &ScalarLiteral,
    location: &SchemaLocation,
    keyword: &str,
) -> Result<(), CompileError> {
    let compatible = match (instance_type, value) {
        (InstanceType::Any, _) => true,
        (InstanceType::Null, ScalarLiteral::Null)
        | (InstanceType::Boolean, ScalarLiteral::Bool(_))
        | (InstanceType::String, ScalarLiteral::String(_))
        | (InstanceType::Number, ScalarLiteral::Number(_)) => true,
        (InstanceType::Integer, ScalarLiteral::Number(number)) => {
            !number.exponent().starts_with('-')
        }
        _ => false,
    };
    if compatible {
        Ok(())
    } else {
        Err(invalid(
            location,
            keyword,
            "values compatible with the declared type",
        ))
    }
}

fn keyword_u64(
    object: &serde_json::Map<String, Value>,
    keyword: &'static str,
    default: u64,
    location: &SchemaLocation,
) -> Result<u64, CompileError> {
    optional_u64(object, keyword, location).map(|value| value.unwrap_or(default))
}

fn optional_u64(
    object: &serde_json::Map<String, Value>,
    keyword: &'static str,
    location: &SchemaLocation,
) -> Result<Option<u64>, CompileError> {
    object
        .get(keyword)
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| invalid(location, keyword, "a non-negative integer"))
        })
        .transpose()
}

fn keyword_bool(
    object: &serde_json::Map<String, Value>,
    keyword: &'static str,
    default: bool,
    location: &SchemaLocation,
) -> Result<bool, CompileError> {
    object
        .get(keyword)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| invalid(location, keyword, "a boolean"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn validate_keyword_values(
    object: &serde_json::Map<String, Value>,
    location: &SchemaLocation,
) -> Result<(), CompileError> {
    if let Some(items) = object.get("items") {
        if !items.is_boolean() && !items.is_object() {
            return Err(invalid(location, "items", "a boolean or schema object"));
        }
    }
    optional_u64(object, "minItems", location)?;
    optional_u64(object, "maxItems", location)?;
    keyword_bool(object, "uniqueItems", false, location)?;
    if object
        .get("properties")
        .is_some_and(|value| !value.is_object())
    {
        return Err(invalid(location, "properties", "an object"));
    }
    if let Some(required) = object.get("required") {
        let values = required
            .as_array()
            .ok_or_else(|| invalid(location, "required", "an array of unique strings"))?;
        let mut names = HashSet::with_capacity(values.len());
        for value in values {
            let name = value
                .as_str()
                .ok_or_else(|| invalid(location, "required", "an array of unique strings"))?;
            if !names.insert(name) {
                return Err(invalid(location, "required", "an array of unique strings"));
            }
        }
    }
    if object
        .get("additionalProperties")
        .is_some_and(|value| !value.is_boolean())
    {
        return Err(invalid(location, "additionalProperties", "a boolean"));
    }
    Ok(())
}

fn validate_applicability(
    object: &serde_json::Map<String, Value>,
    instance_type: InstanceType,
    location: &SchemaLocation,
) -> Result<(), CompileError> {
    for keyword in ["items", "minItems", "maxItems"] {
        if object.contains_key(keyword) && instance_type != InstanceType::Array {
            return Err(unsupported(
                location,
                keyword,
                "checked profile requires explicit array applicability",
            ));
        }
    }
    if object.get("uniqueItems") == Some(&Value::Bool(true)) && instance_type != InstanceType::Array
    {
        return Err(unsupported(
            location,
            "uniqueItems",
            "checked profile requires explicit array applicability",
        ));
    }
    for keyword in ["properties", "required", "additionalProperties"] {
        if object.contains_key(keyword) && instance_type != InstanceType::Object {
            return Err(unsupported(
                location,
                keyword,
                "checked profile requires explicit object applicability",
            ));
        }
    }
    Ok(())
}

fn invalid(location: &SchemaLocation, keyword: &str, expected: &str) -> CompileError {
    CompileError::InvalidKeywordValue {
        location: location.clone(),
        keyword: keyword.into(),
        expected: expected.into(),
    }
}

fn unsupported(location: &SchemaLocation, keyword: &str, reason: &str) -> CompileError {
    CompileError::UnsupportedKeyword {
        location: location.clone(),
        keyword: keyword.into(),
        reason: reason.into(),
    }
}

const SUPPORTED: &[&str] = &[
    "$schema",
    "type",
    "const",
    "enum",
    "items",
    "minItems",
    "maxItems",
    "uniqueItems",
    "properties",
    "required",
    "additionalProperties",
];

const ANNOTATIONS: &[&str] = &[
    "title",
    "description",
    "$comment",
    "default",
    "examples",
    "deprecated",
    "readOnly",
    "writeOnly",
];

const UNSUPPORTED_STANDARD: &[&str] = &[
    "$ref",
    "$dynamicRef",
    "$defs",
    "allOf",
    "anyOf",
    "oneOf",
    "not",
    "if",
    "then",
    "else",
    "prefixItems",
    "contains",
    "minContains",
    "maxContains",
    "unevaluatedItems",
    "unevaluatedProperties",
    "dependentSchemas",
    "dependentRequired",
    "patternProperties",
    "propertyNames",
    "format",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "minLength",
    "maxLength",
    "pattern",
    "minProperties",
    "maxProperties",
];

#[allow(dead_code)]
fn _keyword_uniqueness() -> HashSet<&'static str> {
    SUPPORTED
        .iter()
        .chain(ANNOTATIONS)
        .chain(UNSUPPORTED_STANDARD)
        .copied()
        .collect()
}
