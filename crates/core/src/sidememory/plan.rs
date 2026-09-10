use crate::json_schema::extensions::{ObjectExtensionPlan, RelationSource};
use crate::json_schema::ir::{SchemaIr, SchemaNodeId, SemanticAssertion};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConstraintId(u32);

#[derive(Clone, Debug, Default)]
pub struct MemoryPlan {
    unique_items: Box<[UniqueItemsConstraint]>,
    contains: Box<[ContainsConstraint]>,
    unique_by_node: Box<[Option<u32>]>,
    contains_by_node: Box<[Option<u32>]>,
    object_extensions: Box<[ObjectExtensionPlan]>,
    extension_by_node: Box<[Option<u32>]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UniqueItemsConstraint {
    pub constraint_id: ConstraintId,
    pub array_schema_node: SchemaNodeId,
    pub item_schema_node: SchemaNodeId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainsConstraint {
    pub constraint_id: ConstraintId,
    pub array_schema_node: SchemaNodeId,
    pub predicate: SchemaNodeId,
    pub lower: u64,
    pub upper: Option<u64>,
    pub array_max_items: Option<u64>,
}

impl MemoryPlan {
    pub fn from_ir(ir: &SchemaIr) -> Result<Self, &'static str> {
        let mut unique_items = Vec::new();
        let mut contains = Vec::new();
        let mut unique_by_node = vec![None; ir.nodes().len()];
        let mut contains_by_node = vec![None; ir.nodes().len()];
        let mut next_constraint = 0_u32;
        for node in ir.nodes() {
            for assertion in &node.semantic {
                let constraint_id = ConstraintId(next_constraint);
                next_constraint = next_constraint
                    .checked_add(1)
                    .ok_or("constraint ID overflow")?;
                match assertion {
                    SemanticAssertion::UniqueItems => {
                        let item_schema_node = node
                            .array
                            .as_ref()
                            .ok_or("uniqueItems without array IR")?
                            .items;
                        let index = u32::try_from(unique_items.len())
                            .map_err(|_| "uniqueItems index overflow")?;
                        unique_items.push(UniqueItemsConstraint {
                            constraint_id,
                            array_schema_node: node.id,
                            item_schema_node,
                        });
                        unique_by_node[node.id.get() as usize] = Some(index);
                    }
                    SemanticAssertion::Contains(assertion) => {
                        let index =
                            u32::try_from(contains.len()).map_err(|_| "contains index overflow")?;
                        contains.push(ContainsConstraint {
                            constraint_id,
                            array_schema_node: node.id,
                            predicate: assertion.predicate,
                            lower: assertion.lower,
                            upper: assertion.upper,
                            array_max_items: assertion.array_max_items,
                        });
                        contains_by_node[node.id.get() as usize] = Some(index);
                    }
                }
            }
        }
        Ok(Self {
            unique_items: unique_items.into_boxed_slice(),
            contains: contains.into_boxed_slice(),
            unique_by_node: unique_by_node.into_boxed_slice(),
            contains_by_node: contains_by_node.into_boxed_slice(),
            object_extensions: ir.extensions().objects.clone(),
            extension_by_node: ir.extensions().by_node.clone(),
        })
    }

    pub fn unique_items(&self) -> &[UniqueItemsConstraint] {
        &self.unique_items
    }

    pub fn is_empty(&self) -> bool {
        self.unique_items.is_empty()
            && self.contains.is_empty()
            && self.object_extensions.is_empty()
    }

    pub fn unique_for_node(&self, node: SchemaNodeId) -> Option<&UniqueItemsConstraint> {
        let index = *self.unique_by_node.get(node.get() as usize)?.as_ref()?;
        self.unique_items.get(index as usize)
    }

    pub fn contains_constraints(&self) -> &[ContainsConstraint] {
        &self.contains
    }

    pub fn contains_for_node(&self, node: SchemaNodeId) -> Option<&ContainsConstraint> {
        let index = *self.contains_by_node.get(node.get() as usize)?.as_ref()?;
        self.contains.get(index as usize)
    }

    pub(crate) fn object_extension_for_node(
        &self,
        node: SchemaNodeId,
    ) -> Option<&ObjectExtensionPlan> {
        let index = *self.extension_by_node.get(node.get() as usize)?.as_ref()?;
        self.object_extensions.get(index as usize)
    }

    pub fn object_extension_count(&self) -> usize {
        self.object_extensions.len()
    }

    pub(crate) fn required_imports(&self) -> impl Iterator<Item = &str> {
        self.object_extensions
            .iter()
            .flat_map(|object| object.actions.values())
            .flat_map(|actions| actions.relations.iter())
            .filter_map(|relation| match &relation.source {
                RelationSource::Import { name } => Some(name.as_ref()),
                RelationSource::Capture { .. } => None,
            })
    }
}

impl ConstraintId {
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    pub fn get(self) -> u32 {
        self.0
    }
}
