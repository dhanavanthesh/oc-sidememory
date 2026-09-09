use thiserror::Error;

use crate::sidememory::limits::ResourceError;
use crate::sidememory::token_table::VocabularyError;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SchemaLocation(Box<str>);

impl SchemaLocation {
    pub fn root() -> Self {
        Self("#".into())
    }

    pub fn child(&self, segment: &str) -> Self {
        let escaped = segment.replace('~', "~0").replace('/', "~1");
        Self(format!("{}/{}", self.0, escaped).into_boxed_str())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SchemaLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("malformed schema at {location}: {cause}")]
    MalformedSchema {
        location: SchemaLocation,
        cause: Box<str>,
    },
    #[error("unsupported JSON Schema dialect {dialect}")]
    UnsupportedDialect { dialect: Box<str> },
    #[error("unsupported keyword {keyword} at {location}: {reason}")]
    UnsupportedKeyword {
        location: SchemaLocation,
        keyword: Box<str>,
        reason: Box<str>,
    },
    #[error("invalid value for {keyword} at {location}; expected {expected}")]
    InvalidKeywordValue {
        location: SchemaLocation,
        keyword: Box<str>,
        expected: Box<str>,
    },
    #[error("unsatisfiable schema at {location}: {reason}")]
    UnsatisfiableSchema {
        location: SchemaLocation,
        reason: Box<str>,
    },
    #[error(transparent)]
    InvalidVocabulary(#[from] VocabularyError),
    #[error(transparent)]
    ResourceLimit(#[from] ResourceError),
    #[error("structural lowering failed at {location}: {cause}")]
    StructuralLowering {
        location: SchemaLocation,
        cause: Box<str>,
    },
    #[error("internal compiler invariant failed: {context}")]
    InternalInvariant { context: Box<str> },
}
