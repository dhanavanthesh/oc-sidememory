use crate::sidememory::plan::MemoryPlan;

use super::ir::SchemaIr;

pub(crate) fn lower(ir: &SchemaIr) -> Result<MemoryPlan, &'static str> {
    MemoryPlan::from_ir(ir)
}
