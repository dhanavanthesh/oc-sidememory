use serde_json::{json, Value};

use crate::args::Args;
use crate::output::{CliError, FORMAT_VERSION};

pub fn run(args: &Args) -> Result<Value, CliError> {
    let (mut guide, _) = super::guide_with_prefix(args)?;
    let tokens = guide
        .allowed_tokens()
        .map_err(|error| CliError::guide(error, None))?;
    Ok(json!({"formatVersion": FORMAT_VERSION, "tokens": tokens, "count": tokens.len()}))
}
