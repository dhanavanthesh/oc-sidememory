use super::diagnostic::SchemaLocation;
use super::extensions::CompiledExtensions;
use super::profile::Dialect;
use crate::sidememory::number::CanonicalNumber;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SchemaNodeId(pub(crate) u32);

impl SchemaNodeId {
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug)]
pub struct SchemaIr {
    pub(crate) root: SchemaNodeId,
    pub(crate) nodes: Vec<SchemaNode>,
    pub(crate) dialect: Dialect,
    pub(crate) extensions: CompiledExtensions,
}

impl SchemaIr {
    pub fn root(&self) -> SchemaNodeId {
        self.root
    }

    pub fn nodes(&self) -> &[SchemaNode] {
        &self.nodes
    }

    pub fn node(&self, id: SchemaNodeId) -> Option<&SchemaNode> {
        self.nodes.get(id.0 as usize)
    }

    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    pub(crate) fn extensions(&self) -> &CompiledExtensions {
        &self.extensions
    }
}

#[derive(Clone, Debug)]
pub struct SchemaNode {
    pub id: SchemaNodeId,
    pub schema_location: SchemaLocation,
    pub instance_type: InstanceType,
    pub scalar: ScalarAssertions,
    pub array: Option<ArrayAssertions>,
    pub object: Option<ObjectAssertions>,
    pub semantic: Vec<SemanticAssertion>,
    pub always_false: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstanceType {
    Any,
    Null,
    Boolean,
    Integer,
    Number,
    String,
    Array,
    Object,
}

#[derive(Clone, Debug, Default)]
pub struct ScalarAssertions {
    pub const_value: Option<ScalarLiteral>,
    pub enum_values: Vec<ScalarLiteral>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ScalarLiteral {
    Null,
    Bool(bool),
    Number(CanonicalNumber),
    String(Box<str>),
}

#[derive(Clone, Debug)]
pub struct ArrayAssertions {
    pub items: SchemaNodeId,
    pub min_items: u64,
    pub max_items: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct ObjectAssertions {
    pub properties: Vec<(Box<str>, SchemaNodeId)>,
    pub required: Vec<Box<str>>,
    pub additional_properties: bool,
    pub fixed_property_order: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticAssertion {
    UniqueItems,
    Contains(ContainsAssertion),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContainsAssertion {
    pub predicate: SchemaNodeId,
    pub lower: u64,
    pub upper: Option<u64>,
    pub array_max_items: Option<u64>,
}
