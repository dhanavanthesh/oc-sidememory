use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use super::diagnostic::{CompileError, SchemaLocation};
use super::ir::{InstanceType, SchemaIr, SchemaNodeId};
use crate::sidememory::limits::ResourceError;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionPlanV1 {
    pub version: u32,
    #[serde(default)]
    pub objects: Vec<ObjectExtensionV1>,
}

impl ExtensionPlanV1 {
    pub fn from_json(json: &str) -> Result<Self, CompileError> {
        serde_json::from_str(json).map_err(|error| CompileError::MalformedSchema {
            location: SchemaLocation::root(),
            cause: format!("invalid extension plan: {error}").into_boxed_str(),
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectExtensionV1 {
    pub schema_path: Box<str>,
    pub property_order: Vec<Box<str>>,
    #[serde(default)]
    pub captures: Vec<CaptureV1>,
    #[serde(default)]
    pub relations: Vec<RelationV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureV1 {
    pub name: Box<str>,
    pub source_property: Box<str>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum RelationOperator {
    #[serde(rename = "equal")]
    Equal,
    #[serde(rename = "notEqual")]
    NotEqual,
    #[serde(rename = "memberOf")]
    MemberOf,
    #[serde(rename = "notMemberOf")]
    NotMemberOf,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelationV1 {
    pub target_property: Box<str>,
    pub operator: RelationOperator,
    pub capture: Option<Box<str>>,
    #[serde(rename = "import")]
    pub import_set: Option<Box<str>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RegisterId(pub(crate) u32);

impl RegisterId {
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CompiledExtensions {
    pub objects: Box<[ObjectExtensionPlan]>,
    pub by_node: Box<[Option<u32>]>,
}

#[derive(Clone, Debug)]
pub(crate) struct ObjectExtensionPlan {
    pub actions: HashMap<Box<str>, PropertyActions>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PropertyActions {
    pub captures: Box<[CapturePlan]>,
    pub relations: Box<[RelationPlan]>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CapturePlan {
    pub register: RegisterId,
}

#[derive(Clone, Debug)]
pub(crate) struct RelationPlan {
    pub operator: RelationOperator,
    pub source: RelationSource,
    pub target_property: Box<str>,
}

#[derive(Clone, Debug)]
pub(crate) enum RelationSource {
    Capture {
        register: RegisterId,
        name: Box<str>,
    },
    Import {
        name: Box<str>,
    },
}

pub(crate) fn apply(ir: &mut SchemaIr, plan: Option<&ExtensionPlanV1>) -> Result<(), CompileError> {
    let Some(plan) = plan else {
        ir.extensions = CompiledExtensions {
            objects: Box::new([]),
            by_node: empty_dispatch(ir.nodes.len())?,
        };
        return Ok(());
    };
    if plan.version != 1 {
        return Err(invalid("version", "the supported extension version 1"));
    }
    let mut objects = Vec::new();
    objects
        .try_reserve(plan.objects.len())
        .map_err(|_| allocation("extension objects", plan.objects.len()))?;
    let mut by_node = Vec::new();
    by_node
        .try_reserve_exact(ir.nodes.len())
        .map_err(|_| allocation("extension node dispatch", ir.nodes.len()))?;
    by_node.resize(ir.nodes.len(), None);
    let mut next_register = 0_u32;
    for object in &plan.objects {
        let node_id = resolve_object(ir, &object.schema_path)?;
        if by_node[node_id.get() as usize].is_some() {
            return Err(invalid(
                "schemaPath",
                "one extension object per schema path",
            ));
        }
        let shape = ir
            .node(node_id)
            .and_then(|node| node.object.as_ref())
            .ok_or_else(|| invalid("schemaPath", "a path to an explicit closed object"))?;
        validate_order(
            shape.properties.iter().map(|(name, _)| name.as_ref()),
            object,
        )?;

        let mut positions = HashMap::new();
        positions
            .try_reserve(object.property_order.len())
            .map_err(|_| allocation("property positions", object.property_order.len()))?;
        positions.extend(
            object
                .property_order
                .iter()
                .enumerate()
                .map(|(index, name)| (name.as_ref(), index)),
        );
        let mut registers = HashMap::new();
        registers
            .try_reserve(object.captures.len())
            .map_err(|_| allocation("capture register dispatch", object.captures.len()))?;
        let mut capture_sources = HashMap::new();
        capture_sources
            .try_reserve(object.captures.len())
            .map_err(|_| allocation("capture source dispatch", object.captures.len()))?;
        for capture in &object.captures {
            if capture.name.is_empty() || registers.contains_key(capture.name.as_ref()) {
                return Err(invalid("captures", "unique non-empty capture names"));
            }
            let source = property_node(ir, node_id, &capture.source_property)?;
            let register = RegisterId(next_register);
            next_register = next_register
                .checked_add(1)
                .ok_or_else(|| invalid("captures", "representable register identifiers"))?;
            registers.insert(capture.name.as_ref(), register);
            capture_sources.insert(
                capture.name.as_ref(),
                (capture.source_property.as_ref(), source),
            );
        }

        let action_capacity = object
            .captures
            .len()
            .checked_add(object.relations.len())
            .ok_or_else(|| allocation("property actions", usize::MAX))?;
        let mut actions: HashMap<Box<str>, MutableActions> = HashMap::new();
        actions
            .try_reserve(action_capacity)
            .map_err(|_| allocation("property actions", action_capacity))?;
        for capture in &object.captures {
            let register = *registers
                .get(capture.name.as_ref())
                .ok_or_else(|| invalid("capture", "a known capture name"))?;
            let action = actions.entry(capture.source_property.clone()).or_default();
            action
                .captures
                .try_reserve(1)
                .map_err(|_| allocation("capture actions", 1))?;
            action.captures.push(CapturePlan { register });
        }
        for relation in &object.relations {
            let target = property_node(ir, node_id, &relation.target_property)?;
            let source = match relation.operator {
                RelationOperator::Equal | RelationOperator::NotEqual => {
                    if relation.import_set.is_some() {
                        return Err(invalid(
                            "relations",
                            "equality relations using a capture only",
                        ));
                    }
                    let name = relation
                        .capture
                        .as_deref()
                        .ok_or_else(|| invalid("relations", "a known capture"))?;
                    let (source_property, source_node) = capture_sources
                        .get(name)
                        .copied()
                        .ok_or_else(|| invalid("capture", "a known capture name"))?;
                    let source_position = positions
                        .get(source_property)
                        .copied()
                        .ok_or_else(|| invalid("relations", "a declared capture source"))?;
                    let target_position = positions
                        .get(relation.target_property.as_ref())
                        .copied()
                        .ok_or_else(|| invalid("relations", "a declared target property"))?;
                    if source_position >= target_position {
                        return Err(invalid("relations", "capture sources before their targets"));
                    }
                    if !compatible_types(ir, source_node, target) {
                        return Err(invalid("relations", "compatible source and target types"));
                    }
                    RelationSource::Capture {
                        register: *registers
                            .get(name)
                            .ok_or_else(|| invalid("capture", "a known capture name"))?,
                        name: name.into(),
                    }
                }
                RelationOperator::MemberOf | RelationOperator::NotMemberOf => {
                    if relation.capture.is_some() {
                        return Err(invalid(
                            "relations",
                            "membership relations using an import only",
                        ));
                    }
                    let name = relation
                        .import_set
                        .as_deref()
                        .filter(|name| !name.is_empty())
                        .ok_or_else(|| invalid("import", "a non-empty imported set name"))?;
                    RelationSource::Import { name: name.into() }
                }
            };
            let action = actions.entry(relation.target_property.clone()).or_default();
            action
                .relations
                .try_reserve(1)
                .map_err(|_| allocation("relation actions", 1))?;
            action.relations.push(RelationPlan {
                operator: relation.operator,
                source,
                target_property: relation.target_property.clone(),
            });
        }

        let mut compiled_actions = HashMap::new();
        compiled_actions
            .try_reserve(actions.len())
            .map_err(|_| allocation("compiled property actions", actions.len()))?;
        for (property, actions) in actions {
            compiled_actions.insert(
                property,
                PropertyActions {
                    captures: actions.captures.into_boxed_slice(),
                    relations: actions.relations.into_boxed_slice(),
                },
            );
        }
        let index = u32::try_from(objects.len())
            .map_err(|_| invalid("objects", "representable object plan identifiers"))?;
        objects.push(ObjectExtensionPlan {
            actions: compiled_actions,
        });
        by_node[node_id.get() as usize] = Some(index);

        let shape = ir
            .nodes
            .get_mut(node_id.get() as usize)
            .and_then(|node| node.object.as_mut())
            .ok_or_else(|| invalid("schemaPath", "a path to an explicit closed object"))?;
        let property_count = shape.properties.len();
        let mut properties = HashMap::new();
        properties
            .try_reserve(property_count)
            .map_err(|_| allocation("ordered properties", property_count))?;
        properties.extend(shape.properties.drain(..));
        for name in &object.property_order {
            let node = properties
                .remove(name.as_ref())
                .ok_or_else(|| invalid("propertyOrder", "every declared property exactly once"))?;
            shape.properties.push((name.clone(), node));
        }
        if !properties.is_empty() {
            return Err(invalid(
                "propertyOrder",
                "every declared property exactly once",
            ));
        }
        shape.fixed_property_order = true;
    }
    ir.extensions = CompiledExtensions {
        objects: objects.into_boxed_slice(),
        by_node: by_node.into_boxed_slice(),
    };
    Ok(())
}

#[derive(Default)]
struct MutableActions {
    captures: Vec<CapturePlan>,
    relations: Vec<RelationPlan>,
}

fn resolve_object(ir: &SchemaIr, path: &str) -> Result<SchemaNodeId, CompileError> {
    if path == "$" {
        return Ok(ir.root);
    }
    let rest = path
        .strip_prefix("$.")
        .ok_or_else(|| invalid("schemaPath", "'$' or a direct-property path beginning '$.'"))?;
    let mut current = ir.root;
    for property in rest.split('.') {
        current = property_node(ir, current, property)?;
    }
    Ok(current)
}

fn property_node(
    ir: &SchemaIr,
    object: SchemaNodeId,
    property: &str,
) -> Result<SchemaNodeId, CompileError> {
    ir.node(object)
        .and_then(|node| node.object.as_ref())
        .and_then(|shape| {
            shape
                .properties
                .iter()
                .find_map(|(name, node)| (name.as_ref() == property).then_some(*node))
        })
        .ok_or_else(|| invalid("property", "a declared direct property"))
}

fn validate_order<'a>(
    properties: impl Iterator<Item = &'a str>,
    object: &ObjectExtensionV1,
) -> Result<(), CompileError> {
    if object.property_order.is_empty() {
        return Err(invalid(
            "propertyOrder",
            "every declared property in generation order",
        ));
    }
    let expected: HashSet<&str> = properties.collect();
    let actual: HashSet<&str> = object.property_order.iter().map(AsRef::as_ref).collect();
    if actual.len() != object.property_order.len() || actual != expected {
        return Err(invalid(
            "propertyOrder",
            "every declared property exactly once",
        ));
    }
    Ok(())
}

fn compatible_types(ir: &SchemaIr, source: SchemaNodeId, target: SchemaNodeId) -> bool {
    let Some(source) = ir.node(source) else {
        return false;
    };
    let Some(target) = ir.node(target) else {
        return false;
    };
    source.instance_type == target.instance_type
        || matches!(
            (source.instance_type, target.instance_type),
            (InstanceType::Integer, InstanceType::Number)
                | (InstanceType::Number, InstanceType::Integer)
                | (InstanceType::Any, _)
                | (_, InstanceType::Any)
        )
}

fn invalid(keyword: &str, expected: &str) -> CompileError {
    CompileError::InvalidKeywordValue {
        location: SchemaLocation::root(),
        keyword: keyword.into(),
        expected: expected.into(),
    }
}

fn empty_dispatch(len: usize) -> Result<Box<[Option<u32>]>, CompileError> {
    let mut dispatch = Vec::new();
    dispatch
        .try_reserve_exact(len)
        .map_err(|_| allocation("extension node dispatch", len))?;
    dispatch.resize(len, None);
    Ok(dispatch.into_boxed_slice())
}

fn allocation(context: &'static str, requested: usize) -> CompileError {
    ResourceError::Allocation { context, requested }.into()
}
