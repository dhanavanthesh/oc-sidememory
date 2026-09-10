use serde_json::{json, Value};

use crate::args::Args;
use crate::input;
use crate::output::{CliError, FORMAT_VERSION};

pub fn run(args: &Args) -> Result<Value, CliError> {
    let inputs = input::load(args)?;
    let tokens = input::read_tokens(&args.path("--tokens")?)?;
    let mut guide = inputs.guide(args.usize("--max-rollback", 32)?)?;
    for (index, token) in tokens.iter().copied().enumerate() {
        guide
            .advance(token)
            .map_err(|error| CliError::guide(error, Some(index)))?;
    }
    if args.flag("--finish") {
        guide
            .advance(inputs.eos)
            .map_err(|error| CliError::guide(error, Some(tokens.len())))?;
    }
    Ok(json!({
        "formatVersion": FORMAT_VERSION,
        "accepted": !args.flag("--finish") || guide.is_terminated(),
        "terminated": guide.is_terminated(),
        "tokensCommitted": tokens.len() + usize::from(args.flag("--finish")),
        "rollbackAvailable": guide.rollback_available(),
    }))
}
