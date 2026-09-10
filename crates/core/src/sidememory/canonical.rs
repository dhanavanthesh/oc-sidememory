use std::hash::{BuildHasher, Hasher};

use thiserror::Error;

use super::limits::{checked_growth, reserve, ResourceError, RuntimeLimits};
use super::number::CanonicalNumber;
use super::value::{ByteSpan, CanonicalId, ChildSpan, EntrySpan, ObjectEntry};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicalNode {
    Null,
    Bool(bool),
    Number(CanonicalNumber),
    String(ByteSpan),
    Array(ChildSpan),
    Object(EntrySpan),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArenaMark {
    nodes_len: usize,
    bytes_len: usize,
    children_len: usize,
    entries_len: usize,
}

#[derive(Debug)]
pub struct CanonicalArena {
    nodes: Vec<CanonicalNode>,
    bytes: Vec<u8>,
    children: Vec<CanonicalId>,
    entries: Vec<ObjectEntry>,
    limits: RuntimeLimits,
}

#[derive(Debug, Default)]
pub(crate) struct EqualityScratch {
    work: Vec<(CanonicalId, CanonicalId)>,
}

impl CanonicalArena {
    pub fn new(limits: RuntimeLimits) -> Self {
        Self {
            nodes: Vec::new(),
            bytes: Vec::new(),
            children: Vec::new(),
            entries: Vec::new(),
            limits,
        }
    }

    pub fn mark(&self) -> ArenaMark {
        ArenaMark {
            nodes_len: self.nodes.len(),
            bytes_len: self.bytes.len(),
            children_len: self.children.len(),
            entries_len: self.entries.len(),
        }
    }

    pub fn restore(&mut self, mark: ArenaMark) -> Result<(), ArenaError> {
        if mark.nodes_len > self.nodes.len()
            || mark.bytes_len > self.bytes.len()
            || mark.children_len > self.children.len()
            || mark.entries_len > self.entries.len()
        {
            return Err(ArenaError::InvalidMark);
        }
        self.nodes.truncate(mark.nodes_len);
        self.bytes.truncate(mark.bytes_len);
        self.children.truncate(mark.children_len);
        self.entries.truncate(mark.entries_len);
        Ok(())
    }

    pub fn add_null(&mut self) -> Result<CanonicalId, ArenaError> {
        self.publish(CanonicalNode::Null)
    }

    pub fn add_bool(&mut self, value: bool) -> Result<CanonicalId, ArenaError> {
        self.publish(CanonicalNode::Bool(value))
    }

    pub fn add_number(&mut self, value: CanonicalNumber) -> Result<CanonicalId, ArenaError> {
        self.publish(CanonicalNode::Number(value))
    }

    pub fn add_string(&mut self, value: &[u8]) -> Result<CanonicalId, ArenaError> {
        std::str::from_utf8(value).map_err(|_| ArenaError::InvalidUtf8)?;
        if value.len() > self.limits.max_value_bytes {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_value_bytes",
                limit_value: self.limits.max_value_bytes,
                requested: value.len(),
            }
            .into());
        }
        let end = checked_growth(
            self.bytes.len(),
            value.len(),
            self.limits.max_arena_bytes,
            "max_arena_bytes",
        )?;
        let span = span(self.bytes.len(), value.len(), "string bytes")?;
        reserve(&mut self.bytes, value.len(), "arena bytes")?;
        self.preflight_node()?;
        self.bytes.extend_from_slice(value);
        debug_assert_eq!(self.bytes.len(), end);
        self.publish_reserved(CanonicalNode::String(span))
    }

    pub fn add_array(&mut self, values: &[CanonicalId]) -> Result<CanonicalId, ArenaError> {
        if values.len() > self.limits.max_array_items {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_array_items",
                limit_value: self.limits.max_array_items,
                requested: values.len(),
            }
            .into());
        }
        self.validate_children(values)?;
        let span = child_span(self.children.len(), values.len())?;
        reserve(&mut self.children, values.len(), "arena children")?;
        self.preflight_node()?;
        self.children.extend_from_slice(values);
        self.publish_reserved(CanonicalNode::Array(span))
    }

    pub fn add_object(
        &mut self,
        mut values: Vec<(Vec<u8>, CanonicalId)>,
    ) -> Result<CanonicalId, ArenaError> {
        if values.len() > self.limits.max_object_members {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_object_members",
                limit_value: self.limits.max_object_members,
                requested: values.len(),
            }
            .into());
        }
        for (key, value) in &values {
            std::str::from_utf8(key).map_err(|_| ArenaError::InvalidUtf8)?;
            if key.len() > self.limits.max_value_bytes {
                return Err(ResourceError::LimitExceeded {
                    limit_name: "max_value_bytes",
                    limit_value: self.limits.max_value_bytes,
                    requested: key.len(),
                }
                .into());
            }
            self.validate_id(*value)?;
        }
        values.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        if values.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(ArenaError::DuplicateObjectKey);
        }
        let key_bytes = values.iter().try_fold(0_usize, |total, (key, _)| {
            total
                .checked_add(key.len())
                .ok_or(ResourceError::ArithmeticOverflow {
                    context: "object key bytes",
                })
        })?;
        checked_growth(
            self.bytes.len(),
            key_bytes,
            self.limits.max_arena_bytes,
            "max_arena_bytes",
        )?;
        let entries = entry_span(self.entries.len(), values.len())?;
        reserve(&mut self.bytes, key_bytes, "object key bytes")?;
        reserve(&mut self.entries, values.len(), "object entries")?;
        self.preflight_node()?;
        for (key, value) in values {
            let key_span = span(self.bytes.len(), key.len(), "object key")?;
            self.bytes.extend_from_slice(&key);
            self.entries.push(ObjectEntry {
                key: key_span,
                value,
            });
        }
        self.publish_reserved(CanonicalNode::Object(entries))
    }

    pub fn node(&self, id: CanonicalId) -> Result<&CanonicalNode, ArenaError> {
        self.nodes
            .get(id.0 as usize)
            .ok_or(ArenaError::InvalidId(id))
    }

    pub fn string_bytes(&self, span: ByteSpan) -> Result<&[u8], ArenaError> {
        slice(&self.bytes, span.start, span.len).ok_or(ArenaError::InvalidSpan)
    }

    pub(crate) fn array_values(&self, span: ChildSpan) -> Result<&[CanonicalId], ArenaError> {
        slice(&self.children, span.start, span.len).ok_or(ArenaError::InvalidSpan)
    }

    pub(crate) fn object_entries(&self, span: EntrySpan) -> Result<&[ObjectEntry], ArenaError> {
        slice(&self.entries, span.start, span.len).ok_or(ArenaError::InvalidSpan)
    }

    pub(crate) fn object_key(&self, entry: ObjectEntry) -> Result<&[u8], ArenaError> {
        self.string_bytes(entry.key)
    }

    pub(crate) fn object_value(&self, entry: ObjectEntry) -> CanonicalId {
        entry.value
    }

    pub fn equal(&self, left: CanonicalId, right: CanonicalId) -> Result<bool, ArenaError> {
        self.equal_across(left, self, right)
    }

    pub fn equal_across(
        &self,
        left: CanonicalId,
        other: &CanonicalArena,
        right: CanonicalId,
    ) -> Result<bool, ArenaError> {
        let mut scratch = EqualityScratch::default();
        self.equal_across_with_scratch(left, other, right, &mut scratch)
    }

    pub(crate) fn equal_with_scratch(
        &self,
        left: CanonicalId,
        right: CanonicalId,
        scratch: &mut EqualityScratch,
    ) -> Result<bool, ArenaError> {
        self.equal_across_with_scratch(left, self, right, scratch)
    }

    pub(crate) fn equal_across_with_scratch(
        &self,
        left: CanonicalId,
        other: &CanonicalArena,
        right: CanonicalId,
        scratch: &mut EqualityScratch,
    ) -> Result<bool, ArenaError> {
        scratch.work.clear();
        reserve(&mut scratch.work, 1, "canonical equality work")?;
        scratch.work.push((left, right));
        while let Some((left, right)) = scratch.work.pop() {
            match (self.node(left)?, other.node(right)?) {
                (CanonicalNode::Null, CanonicalNode::Null) => {}
                (CanonicalNode::Bool(a), CanonicalNode::Bool(b)) if a == b => {}
                (CanonicalNode::Number(a), CanonicalNode::Number(b)) if a == b => {}
                (CanonicalNode::String(a), CanonicalNode::String(b)) => {
                    if self.string_bytes(*a)? != other.string_bytes(*b)? {
                        return Ok(false);
                    }
                }
                (CanonicalNode::Array(a), CanonicalNode::Array(b)) => {
                    let left =
                        slice(&self.children, a.start, a.len).ok_or(ArenaError::InvalidSpan)?;
                    let right =
                        slice(&other.children, b.start, b.len).ok_or(ArenaError::InvalidSpan)?;
                    if left.len() != right.len() {
                        return Ok(false);
                    }
                    reserve(&mut scratch.work, left.len(), "canonical equality work")?;
                    scratch
                        .work
                        .extend(left.iter().zip(right).rev().map(|(a, b)| (*a, *b)));
                }
                (CanonicalNode::Object(a), CanonicalNode::Object(b)) => {
                    let left =
                        slice(&self.entries, a.start, a.len).ok_or(ArenaError::InvalidSpan)?;
                    let right =
                        slice(&other.entries, b.start, b.len).ok_or(ArenaError::InvalidSpan)?;
                    if left.len() != right.len() {
                        return Ok(false);
                    }
                    for (left, right) in left.iter().zip(right) {
                        if self.string_bytes(left.key)? != other.string_bytes(right.key)? {
                            return Ok(false);
                        }
                    }
                    reserve(&mut scratch.work, left.len(), "canonical equality work")?;
                    scratch.work.extend(
                        left.iter()
                            .zip(right)
                            .rev()
                            .map(|(a, b)| (a.value, b.value)),
                    );
                }
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    pub fn fingerprint<S: BuildHasher>(
        &self,
        root: CanonicalId,
        hash_builder: &S,
    ) -> Result<u64, ArenaError> {
        enum Work {
            Value(CanonicalId),
            Key(ByteSpan),
        }
        let mut hasher = hash_builder.build_hasher();
        let mut work = vec![Work::Value(root)];
        while let Some(item) = work.pop() {
            match item {
                Work::Key(key) => write_bytes(&mut hasher, 7, self.string_bytes(key)?)?,
                Work::Value(id) => match self.node(id)? {
                    CanonicalNode::Null => hasher.write(&[0]),
                    CanonicalNode::Bool(false) => hasher.write(&[1]),
                    CanonicalNode::Bool(true) => hasher.write(&[2]),
                    CanonicalNode::Number(number) => {
                        hasher.write(&[3]);
                        hasher.write(&[u8::from(number.is_negative())]);
                        write_field(&mut hasher, number.coefficient().as_bytes())?;
                        hasher.write(&[u8::from(number.exponent().starts_with('-'))]);
                        write_field(
                            &mut hasher,
                            number.exponent().trim_start_matches('-').as_bytes(),
                        )?;
                    }
                    CanonicalNode::String(value) => {
                        write_bytes(&mut hasher, 4, self.string_bytes(*value)?)?
                    }
                    CanonicalNode::Array(span) => {
                        hasher.write(&[5]);
                        write_count(&mut hasher, span.len as usize)?;
                        let values = slice(&self.children, span.start, span.len)
                            .ok_or(ArenaError::InvalidSpan)?;
                        work.extend(values.iter().rev().map(|id| Work::Value(*id)));
                    }
                    CanonicalNode::Object(span) => {
                        hasher.write(&[6]);
                        write_count(&mut hasher, span.len as usize)?;
                        let values = slice(&self.entries, span.start, span.len)
                            .ok_or(ArenaError::InvalidSpan)?;
                        for entry in values.iter().rev() {
                            work.push(Work::Value(entry.value));
                            work.push(Work::Key(entry.key));
                        }
                    }
                },
            }
        }
        Ok(hasher.finish())
    }

    pub fn lengths(&self) -> (usize, usize, usize, usize) {
        (
            self.nodes.len(),
            self.bytes.len(),
            self.children.len(),
            self.entries.len(),
        )
    }

    fn preflight_node(&mut self) -> Result<(), ArenaError> {
        checked_growth(
            self.nodes.len(),
            1,
            self.limits.max_arena_nodes,
            "max_arena_nodes",
        )?;
        reserve(&mut self.nodes, 1, "arena nodes")?;
        Ok(())
    }

    fn publish(&mut self, node: CanonicalNode) -> Result<CanonicalId, ArenaError> {
        self.preflight_node()?;
        self.publish_reserved(node)
    }

    fn publish_reserved(&mut self, node: CanonicalNode) -> Result<CanonicalId, ArenaError> {
        let id = CanonicalId(u32::try_from(self.nodes.len()).map_err(|_| {
            ResourceError::CompactIdOverflow {
                context: "canonical node",
            }
        })?);
        self.nodes.push(node);
        Ok(id)
    }

    fn validate_id(&self, id: CanonicalId) -> Result<(), ArenaError> {
        self.node(id).map(|_| ())
    }

    fn validate_children(&self, values: &[CanonicalId]) -> Result<(), ArenaError> {
        for value in values {
            self.validate_id(*value)?;
        }
        Ok(())
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ArenaError {
    #[error(transparent)]
    Resource(#[from] ResourceError),
    #[error("invalid canonical id {0:?}")]
    InvalidId(CanonicalId),
    #[error("invalid arena span")]
    InvalidSpan,
    #[error("invalid arena mark")]
    InvalidMark,
    #[error("canonical string is not UTF-8")]
    InvalidUtf8,
    #[error("duplicate decoded object key")]
    DuplicateObjectKey,
}

fn span(start: usize, len: usize, context: &'static str) -> Result<ByteSpan, ResourceError> {
    Ok(ByteSpan {
        start: u32::try_from(start).map_err(|_| ResourceError::CompactIdOverflow { context })?,
        len: u32::try_from(len).map_err(|_| ResourceError::CompactIdOverflow { context })?,
    })
}

fn child_span(start: usize, len: usize) -> Result<ChildSpan, ResourceError> {
    let span = span(start, len, "array children")?;
    Ok(ChildSpan {
        start: span.start,
        len: span.len,
    })
}

fn entry_span(start: usize, len: usize) -> Result<EntrySpan, ResourceError> {
    let span = span(start, len, "object entries")?;
    Ok(EntrySpan {
        start: span.start,
        len: span.len,
    })
}

fn slice<T>(values: &[T], start: u32, len: u32) -> Option<&[T]> {
    let start = start as usize;
    let end = start.checked_add(len as usize)?;
    values.get(start..end)
}

fn write_bytes(hasher: &mut impl Hasher, tag: u8, bytes: &[u8]) -> Result<(), ArenaError> {
    hasher.write(&[tag]);
    write_field(hasher, bytes)
}

fn write_field(hasher: &mut impl Hasher, bytes: &[u8]) -> Result<(), ArenaError> {
    write_count(hasher, bytes.len())?;
    hasher.write(bytes);
    Ok(())
}

fn write_count(hasher: &mut impl Hasher, count: usize) -> Result<(), ArenaError> {
    let count = u64::try_from(count).map_err(|_| ResourceError::CompactIdOverflow {
        context: "fingerprint length",
    })?;
    hasher.write(&count.to_le_bytes());
    Ok(())
}
