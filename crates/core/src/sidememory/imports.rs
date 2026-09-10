use std::collections::HashMap;

use thiserror::Error;

use super::canonical::{ArenaError, CanonicalArena, EqualityScratch};
use super::history::{CanonicalSet, HistoryError, InsertResult};
use super::limits::{ResourceError, RuntimeLimits};
use super::number::CanonicalNumber;
use super::value::CanonicalId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ImportSetId(u32);

impl ImportSetId {
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Debug)]
pub struct ImportedMemory {
    identity: Box<str>,
    version: Box<str>,
    arena: CanonicalArena,
    sets: Box<[CanonicalSet]>,
    by_name: HashMap<Box<str>, ImportSetId>,
}

impl ImportedMemory {
    pub fn from_json(
        identity: impl Into<Box<str>>,
        version: impl Into<Box<str>>,
        json_text: &str,
    ) -> Result<Self, ImportError> {
        ImportedMemoryBuilder::new(identity, version).build_json(json_text)
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn set_count(&self) -> usize {
        self.sets.len()
    }

    pub fn contains(
        &self,
        name: &str,
        candidate_arena: &CanonicalArena,
        value: CanonicalId,
    ) -> Result<bool, ImportError> {
        let mut scratch = EqualityScratch::default();
        self.contains_with_scratch(name, candidate_arena, value, &mut scratch)
    }

    pub(crate) fn contains_with_scratch(
        &self,
        name: &str,
        candidate_arena: &CanonicalArena,
        value: CanonicalId,
        scratch: &mut EqualityScratch,
    ) -> Result<bool, ImportError> {
        let id = self
            .by_name
            .get(name)
            .copied()
            .ok_or_else(|| ImportError::UnknownSet(name.into()))?;
        self.sets[id.0 as usize]
            .contains_across_with_scratch(&self.arena, candidate_arena, value, scratch)
            .map_err(Into::into)
    }

    pub(crate) fn has_set(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }
}

pub struct ImportedMemoryBuilder {
    identity: Box<str>,
    version: Box<str>,
    runtime_limits: RuntimeLimits,
    max_sets: usize,
    max_values: usize,
}

impl ImportedMemoryBuilder {
    pub fn new(identity: impl Into<Box<str>>, version: impl Into<Box<str>>) -> Self {
        Self {
            identity: identity.into(),
            version: version.into(),
            runtime_limits: RuntimeLimits::default(),
            max_sets: 4096,
            max_values: 1_000_000,
        }
    }

    #[must_use]
    pub fn runtime_limits(mut self, limits: RuntimeLimits) -> Self {
        self.runtime_limits = limits;
        self
    }

    #[must_use]
    pub fn max_sets(mut self, limit: usize) -> Self {
        self.max_sets = limit;
        self
    }

    #[must_use]
    pub fn max_values(mut self, limit: usize) -> Self {
        self.max_values = limit;
        self
    }

    pub fn build_json(self, json_text: &str) -> Result<ImportedMemory, ImportError> {
        if self.identity.is_empty() || self.version.is_empty() {
            return Err(ImportError::InvalidMetadata);
        }
        let root: serde_json::Value = serde_json::from_str(json_text)
            .map_err(|error| ImportError::Malformed(error.to_string().into_boxed_str()))?;
        let object = root.as_object().ok_or(ImportError::ExpectedObject)?;
        if object.len() > self.max_sets {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_import_sets",
                limit_value: self.max_sets,
                requested: object.len(),
            }
            .into());
        }
        let runtime_limits = self.runtime_limits;
        let mut arena = CanonicalArena::new(runtime_limits.clone());
        let mut sets = Vec::new();
        sets.try_reserve(object.len())
            .map_err(|_| ResourceError::Allocation {
                context: "import sets",
                requested: object.len(),
            })?;
        let mut by_name = HashMap::new();
        by_name
            .try_reserve(object.len())
            .map_err(|_| ResourceError::Allocation {
                context: "import set dispatch",
                requested: object.len(),
            })?;
        let mut value_count = 0_usize;
        for (name, values) in object {
            if name.is_empty() {
                return Err(ImportError::EmptySetName);
            }
            let values = values
                .as_array()
                .ok_or_else(|| ImportError::ExpectedSetArray {
                    name: name.clone().into_boxed_str(),
                })?;
            value_count =
                value_count
                    .checked_add(values.len())
                    .ok_or(ResourceError::ArithmeticOverflow {
                        context: "imported values",
                    })?;
            if value_count > self.max_values {
                return Err(ResourceError::LimitExceeded {
                    limit_name: "max_import_values",
                    limit_value: self.max_values,
                    requested: value_count,
                }
                .into());
            }
            let mut set = CanonicalSet::default();
            for value in values {
                let id = canonicalize(value, &mut arena, &runtime_limits)?;
                if set.insert(&arena, id)? == InsertResult::Duplicate {
                    continue;
                }
            }
            let id = ImportSetId(u32::try_from(sets.len()).map_err(|_| {
                ResourceError::CompactIdOverflow {
                    context: "import set",
                }
            })?);
            sets.push(set);
            by_name.insert(name.clone().into_boxed_str(), id);
        }
        Ok(ImportedMemory {
            identity: self.identity,
            version: self.version,
            arena,
            sets: sets.into_boxed_slice(),
            by_name,
        })
    }
}

fn canonicalize(
    value: &serde_json::Value,
    arena: &mut CanonicalArena,
    limits: &RuntimeLimits,
) -> Result<CanonicalId, ImportError> {
    match value {
        serde_json::Value::Null => arena.add_null().map_err(Into::into),
        serde_json::Value::Bool(value) => arena.add_bool(*value).map_err(Into::into),
        serde_json::Value::Number(value) => {
            let number = CanonicalNumber::parse(value.to_string().as_bytes(), limits)
                .map_err(|error| ImportError::Malformed(error.to_string().into_boxed_str()))?;
            arena.add_number(number).map_err(Into::into)
        }
        serde_json::Value::String(value) => arena.add_string(value.as_bytes()).map_err(Into::into),
        serde_json::Value::Array(values) => {
            let mut children = Vec::new();
            children
                .try_reserve(values.len())
                .map_err(|_| ResourceError::Allocation {
                    context: "import array children",
                    requested: values.len(),
                })?;
            for value in values {
                children.push(canonicalize(value, arena, limits)?);
            }
            arena.add_array(&children).map_err(Into::into)
        }
        serde_json::Value::Object(values) => {
            let mut entries = Vec::new();
            entries
                .try_reserve(values.len())
                .map_err(|_| ResourceError::Allocation {
                    context: "import object entries",
                    requested: values.len(),
                })?;
            for (key, value) in values {
                entries.push((key.as_bytes().to_vec(), canonicalize(value, arena, limits)?));
            }
            arena.add_object(entries).map_err(Into::into)
        }
    }
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("import identity and version must be non-empty")]
    InvalidMetadata,
    #[error("malformed imported memory: {0}")]
    Malformed(Box<str>),
    #[error("imported memory must be a JSON object of named arrays")]
    ExpectedObject,
    #[error("import set name must be non-empty")]
    EmptySetName,
    #[error("import set {name} must be an array")]
    ExpectedSetArray { name: Box<str> },
    #[error("unknown imported set {0}")]
    UnknownSet(Box<str>),
    #[error(transparent)]
    Arena(#[from] ArenaError),
    #[error(transparent)]
    History(#[from] HistoryError),
    #[error(transparent)]
    Resource(#[from] ResourceError),
}
