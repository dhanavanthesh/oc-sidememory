use crate::json_schema::ir::{SchemaIr, SchemaNodeId, SemanticAssertion};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConstraintId(u32);

#[derive(Clone, Debug, Default)]
pub struct MemoryPlan {
    unique_items: Box<[UniqueItemsConstraint]>,
    unique_by_node: Box<[Option<ConstraintId>]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UniqueItemsConstraint {
    pub constraint_id: ConstraintId,
    pub array_schema_node: SchemaNodeId,
    pub item_schema_node: SchemaNodeId,
}

impl MemoryPlan {
    pub fn from_ir(ir: &SchemaIr) -> Result<Self, &'static str> {
        let mut unique_items = Vec::new();
        let mut unique_by_node = vec![None; ir.nodes().len()];
        for node in ir.nodes() {
            if node.semantic.contains(&SemanticAssertion::UniqueItems) {
                let item_schema_node = node
                    .array
                    .as_ref()
                    .ok_or("uniqueItems without array IR")?
                    .items;
                let constraint_id = ConstraintId(
                    u32::try_from(unique_items.len()).map_err(|_| "constraint ID overflow")?,
                );
                unique_items.push(UniqueItemsConstraint {
                    constraint_id,
                    array_schema_node: node.id,
                    item_schema_node,
                });
                unique_by_node[node.id.get() as usize] = Some(constraint_id);
            }
        }
        Ok(Self {
            unique_items: unique_items.into_boxed_slice(),
            unique_by_node: unique_by_node.into_boxed_slice(),
        })
    }

    pub fn unique_items(&self) -> &[UniqueItemsConstraint] {
        &self.unique_items
    }

    pub fn is_empty(&self) -> bool {
        self.unique_items.is_empty()
    }

    pub fn unique_for_node(&self, node: SchemaNodeId) -> Option<&UniqueItemsConstraint> {
        let id = self.unique_by_node.get(node.get() as usize)?.as_ref()?;
        self.unique_items.get(id.get() as usize)
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
