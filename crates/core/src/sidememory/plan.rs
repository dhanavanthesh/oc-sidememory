use crate::json_schema::ir::{SchemaIr, SchemaNodeId, SemanticAssertion};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConstraintId(u32);

#[derive(Clone, Debug, Default)]
pub struct MemoryPlan {
    unique_items: Vec<UniqueItemsConstraint>,
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
            }
        }
        Ok(Self { unique_items })
    }

    pub fn unique_items(&self) -> &[UniqueItemsConstraint] {
        &self.unique_items
    }

    pub fn is_empty(&self) -> bool {
        self.unique_items.is_empty()
    }
}
