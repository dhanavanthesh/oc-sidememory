use serde_json::{json, Value};

use crate::args::Args;
use crate::input;
use crate::output::{CliError, FORMAT_VERSION};

pub fn run(args: &Args) -> Result<Value, CliError> {
    let inputs = input::load(args)?;
    Ok(json!({
        "formatVersion": FORMAT_VERSION,
        "compiled": true,
        "modelWidth": inputs.compiled.token_table().model_width(),
        "schemaNodes": inputs.compiled.ir().nodes().len(),
        "semanticConstraints": inputs.compiled.memory_plan().unique_items().len()
            + inputs.compiled.memory_plan().contains_constraints().len()
            + inputs.compiled.memory_plan().object_extension_count(),
        "regularPlanBytes": inputs.compiled.regular_plan().len(),
    }))
}
