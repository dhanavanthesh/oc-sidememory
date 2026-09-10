use serde_json::{json, Value};

use crate::args::Args;
use crate::output::{CliError, FORMAT_VERSION};

pub fn run(args: &Args) -> Result<Value, CliError> {
    let (mut guide, _) = super::guide_with_prefix(args)?;
    let mut words = vec![0_u32; guide.model_width().div_ceil(32)];
    let summary = guide
        .write_mask(&mut words)
        .map_err(|error| CliError::guide(error, None))?;
    Ok(json!({
        "formatVersion": FORMAT_VERSION,
        "words": words,
        "allowed": summary.allowed,
        "modelWidth": guide.model_width(),
    }))
}
