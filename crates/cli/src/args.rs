use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::output::CliError;

const COMMANDS: &[&str] = &[
    "compile",
    "tokens",
    "mask",
    "check",
    "inspect-imports",
    "capabilities",
];

#[derive(Debug)]
pub struct Args {
    pub command: String,
    values: HashMap<String, String>,
    flags: HashSet<String>,
}

impl Args {
    pub fn parse() -> Result<Self, CliError> {
        let mut arguments = std::env::args().skip(1);
        let Some(command) = arguments.next() else {
            return Err(CliError::usage(Self::help()));
        };
        if command == "--help" || command == "-h" {
            println!("{}", Self::help());
            std::process::exit(0);
        }
        if !COMMANDS.contains(&command.as_str()) {
            return Err(CliError::usage(format!(
                "unknown command {command}\n\n{}",
                Self::help()
            )));
        }
        let mut values = HashMap::new();
        let mut flags = HashSet::new();
        while let Some(option) = arguments.next() {
            if !option.starts_with("--") {
                return Err(CliError::usage(format!("unexpected argument {option}")));
            }
            if option == "--finish" {
                flags.insert(option);
                continue;
            }
            let value = arguments
                .next()
                .ok_or_else(|| CliError::usage(format!("missing value for {option}")))?;
            if value.starts_with("--") || values.insert(option.clone(), value).is_some() {
                return Err(CliError::usage(format!(
                    "invalid or repeated option {option}"
                )));
            }
        }
        Ok(Self {
            command,
            values,
            flags,
        })
    }

    pub fn path(&self, name: &str) -> Result<PathBuf, CliError> {
        self.values
            .get(name)
            .map(PathBuf::from)
            .ok_or_else(|| CliError::usage(format!("missing required option {name}")))
    }

    pub fn optional_path(&self, name: &str) -> Option<PathBuf> {
        self.values.get(name).map(PathBuf::from)
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    pub fn usize(&self, name: &str, default: usize) -> Result<usize, CliError> {
        self.value(name)
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| CliError::usage(format!("{name} must be an integer")))
            })
            .transpose()
            .map(|value| value.unwrap_or(default))
    }

    pub fn flag(&self, name: &str) -> bool {
        self.flags.contains(name)
    }

    fn help() -> &'static str {
        "OC-Sidememory semantic CLI\n\nCommands:\n  compile          Compile a schema and report its plans\n  tokens           List allowed tokens after an optional prefix\n  mask             Write the semantic token mask after an optional prefix\n  check            Advance a token sequence and optionally finish with EOS\n  inspect-imports  Validate imported memory metadata and contents\n  capabilities     Print the supported checked profile\n\nShared inputs:\n  --schema PATH --vocabulary PATH [--extensions PATH] [--imports PATH]\n  [--import-identity TEXT] [--import-version TEXT] [--max-rollback N]"
    }
}
