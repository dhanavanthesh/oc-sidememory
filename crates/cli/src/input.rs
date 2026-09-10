use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use oc_sidememory::{
    compile_schema, CompileOptions, CompiledSchema, ExtensionPlanV1, Guide, GuideOptions,
    ImportedMemory, Vocabulary,
};
use serde::Deserialize;

use crate::args::Args;
use crate::output::CliError;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VocabularyFile {
    format_version: u32,
    model_width: usize,
    eos_token_id: u32,
    tokens: Vec<TokenFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TokenFile {
    id: u32,
    bytes_base64: String,
}

pub struct Inputs {
    pub compiled: Arc<CompiledSchema>,
    pub imports: Option<Arc<ImportedMemory>>,
    pub eos: u32,
}

impl Inputs {
    pub fn guide(&self, max_rollback: usize) -> Result<Guide, CliError> {
        let options = GuideOptions {
            max_rollback_tokens: max_rollback,
            ..GuideOptions::default()
        };
        match &self.imports {
            Some(imports) => {
                Guide::new_with_imports(self.compiled.clone(), options, imports.clone())
            }
            None => Guide::new(self.compiled.clone(), options),
        }
        .map_err(|error| CliError::guide(error, None))
    }
}

pub fn load(args: &Args) -> Result<Inputs, CliError> {
    let schema = read(&args.path("--schema")?)?;
    let vocabulary_text = read(&args.path("--vocabulary")?)?;
    let vocabulary_file: VocabularyFile = serde_json::from_str(&vocabulary_text)
        .map_err(|error| CliError::usage(format!("invalid vocabulary file: {error}")))?;
    let model_width = vocabulary_file.model_width;
    let (vocabulary, eos) = parse_vocabulary(vocabulary_file)?;
    let extensions = args
        .optional_path("--extensions")
        .map(|path| read(&path))
        .transpose()?
        .map(|text| {
            ExtensionPlanV1::from_json(&text).map_err(|error| CliError::compile(error.to_string()))
        })
        .transpose()?;
    let options = CompileOptions {
        extension_plan: extensions,
        ..CompileOptions::default()
    };
    let compiled = Arc::new(
        compile_schema(schema.as_bytes(), &vocabulary, model_width, &options)
            .map_err(|error| CliError::compile(error.to_string()))?,
    );
    let imports = args
        .optional_path("--imports")
        .map(|path| read(&path))
        .transpose()?
        .map(|text| {
            let identity = args.value("--import-identity").unwrap_or("local-imports");
            let version = args.value("--import-version").unwrap_or("1");
            ImportedMemory::from_json(identity, version, &text)
                .map(Arc::new)
                .map_err(|error| CliError::compile(error.to_string()))
        })
        .transpose()?;
    Ok(Inputs {
        compiled,
        imports,
        eos,
    })
}

pub fn read_tokens(path: &Path) -> Result<Vec<u32>, CliError> {
    let text = read(path)?;
    serde_json::from_str(&text)
        .map_err(|error| CliError::usage(format!("invalid token list: {error}")))
}

fn read(path: &Path) -> Result<String, CliError> {
    fs::read_to_string(path)
        .map_err(|error| CliError::usage(format!("cannot read {}: {error}", path.display())))
}

fn parse_vocabulary(file: VocabularyFile) -> Result<(Vocabulary, u32), CliError> {
    if file.format_version != 1 {
        return Err(CliError::usage("unsupported vocabulary formatVersion"));
    }
    if file.model_width == 0 || file.eos_token_id as usize >= file.model_width {
        return Err(CliError::usage("eosTokenId must be inside modelWidth"));
    }
    let mut by_id: HashMap<u32, Vec<u8>> = HashMap::new();
    let mut vocabulary = Vocabulary::new(file.eos_token_id);
    for token in file.tokens {
        if token.id as usize >= file.model_width || token.id == file.eos_token_id {
            return Err(CliError::usage(format!(
                "token id {} is outside the vocabulary range",
                token.id
            )));
        }
        let bytes = decode_base64(&token.bytes_base64)?;
        if let Some(previous) = by_id.insert(token.id, bytes.clone()) {
            let detail = if previous == bytes {
                "is repeated"
            } else {
                "has conflicting byte mappings"
            };
            return Err(CliError::usage(format!("token id {} {detail}", token.id)));
        }
        vocabulary
            .try_insert(bytes, token.id)
            .map_err(|error| CliError::usage(error.to_string()))?;
    }
    Ok((vocabulary, file.eos_token_id))
}

fn decode_base64(text: &str) -> Result<Vec<u8>, CliError> {
    if text.len() % 4 != 0 {
        return Err(CliError::usage("bytesBase64 has invalid length"));
    }
    let mut output = Vec::with_capacity(text.len() / 4 * 3);
    for (chunk_index, chunk) in text.as_bytes().chunks_exact(4).enumerate() {
        let a = digit(chunk[0])?;
        let b = digit(chunk[1])?;
        let c = if chunk[2] == b'=' {
            64
        } else {
            digit(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            64
        } else {
            digit(chunk[3])?
        };
        let is_last = chunk_index + 1 == text.len() / 4;
        if !is_last && (c == 64 || d == 64) || c == 64 && d != 64 {
            return Err(CliError::usage("bytesBase64 has invalid padding"));
        }
        output.push((a << 2) | (b >> 4));
        if c != 64 {
            output.push((b << 4) | (c >> 2));
        }
        if d != 64 {
            output.push((c << 6) | d);
        }
    }
    Ok(output)
}

fn digit(byte: u8) -> Result<u8, CliError> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(CliError::usage("bytesBase64 contains an invalid character")),
    }
}

#[cfg(test)]
mod tests {
    use super::decode_base64;

    #[test]
    fn base64_decoder_handles_binary_and_rejects_bad_padding() {
        assert_eq!(decode_base64("AP+A").expect("valid"), [0, 255, 128]);
        assert_eq!(decode_base64("Ww==").expect("valid"), b"[");
        assert!(decode_base64("W===").is_err());
        assert!(decode_base64("Ww=a").is_err());
    }
}
