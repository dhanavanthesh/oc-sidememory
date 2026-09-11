mod check;
mod compile;
mod mask;
mod tokens;

use serde_json::{json, Value};

use crate::args::Args;
use crate::input;
use crate::output::{CliError, FORMAT_VERSION};

pub fn run(args: Args) -> Result<Value, CliError> {
    match args.command.as_str() {
        "compile" => compile::run(&args),
        "tokens" => tokens::run(&args),
        "mask" => mask::run(&args),
        "check" => check::run(&args),
        "inspect-imports" => inspect_imports(&args),
        "capabilities" => Ok(capabilities()),
        _ => Err(CliError::usage("unknown command")),
    }
}

fn inspect_imports(args: &Args) -> Result<Value, CliError> {
    let path = args.path("--imports")?;
    let text = std::fs::read_to_string(&path)
        .map_err(|error| CliError::usage(format!("cannot read {}: {error}", path.display())))?;
    let identity = args.value("--import-identity").unwrap_or("local-imports");
    let version = args.value("--import-version").unwrap_or("1");
    let imports = oc_sidememory::ImportedMemory::from_json(identity, version, &text)
        .map_err(|error| CliError::compile(error.to_string()))?;
    Ok(json!({
        "formatVersion": FORMAT_VERSION,
        "identity": imports.identity(),
        "version": imports.version(),
        "setCount": imports.set_count(),
    }))
}

fn capabilities() -> Value {
    json!({
        "formatVersion": FORMAT_VERSION,
        "dialect": "https://json-schema.org/draft/2020-12/schema",
        "semantics": ["uniqueItems", "contains", "minContains", "maxContains", "capture", "equal", "notEqual", "memberOf", "notMemberOf"],
        "extensionVersions": [1],
        "liveGuideSerialization": false,
    })
}

pub(super) fn guide_with_prefix(
    args: &Args,
) -> Result<(oc_sidememory::Guide, input::Inputs), CliError> {
    let inputs = input::load(args)?;
    let mut guide = inputs.guide(args.usize("--max-rollback", 32)?)?;
    if let Some(path) = args.optional_path("--tokens") {
        for (index, token) in input::read_tokens(&path)?.into_iter().enumerate() {
            guide
                .advance(token)
                .map_err(|error| CliError::guide(error, Some(index)))?;
        }
    }
    Ok((guide, inputs))
}
