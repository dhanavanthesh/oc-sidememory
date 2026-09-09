use thiserror::Error;

use crate::json_schema::ir::SchemaNodeId;
use crate::primitives::TokenId;

use super::canonical::{ArenaError, CanonicalArena};
use super::event::JsonEvent;
use super::limits::RuntimeLimits;
use super::number::{CanonicalNumber, NumberError};
use super::scope::{FrameId, FrameSlots, ScopeError};
use super::token_table::{TokenTable, VocabularyError};
use super::value::CanonicalId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringContext {
    Key,
    Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CursorMode {
    ExpectValue,
    InString(StringContext),
    AfterBackslash(StringContext),
    InUnicodeEscape {
        context: StringContext,
        digits: u8,
        value: u16,
    },
    WaitingLowSurrogate {
        context: StringContext,
        high: u16,
        matched: u8,
    },
    InNumber,
    InTrue {
        matched: u8,
    },
    InFalse {
        matched: u8,
    },
    InNull {
        matched: u8,
    },
    AfterValue,
    ExpectObjectKeyOrEnd,
    ExpectColon,
    ExpectObjectCommaOrEnd,
    ExpectArrayValueOrEnd,
    ExpectArrayCommaOrEnd,
    CompleteRoot,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Utf8State {
    pub expected_continuations: u8,
    pub codepoint: u32,
    pub minimum_codepoint: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorSnapshot {
    pub mode: CursorMode,
    pub frames: Vec<FrameId>,
    pub partial_scalar: Vec<u8>,
    pub root: Option<CanonicalId>,
    pub terminated: bool,
    pub byte_offset: usize,
}

pub struct JsonCursor {
    mode: CursorMode,
    frames: Vec<CursorFrame>,
    slots: FrameSlots,
    root: Option<CanonicalId>,
    arena: CanonicalArena,
    partial: Vec<u8>,
    utf8: Utf8State,
    terminated: bool,
    byte_offset: usize,
    limits: RuntimeLimits,
}

enum CursorFrame {
    Array {
        id: FrameId,
        values: Vec<CanonicalId>,
    },
    Object {
        id: FrameId,
        current_key: Option<Vec<u8>>,
        values: Vec<(Vec<u8>, CanonicalId)>,
    },
}

impl JsonCursor {
    pub fn new(limits: RuntimeLimits) -> Self {
        Self {
            mode: CursorMode::ExpectValue,
            frames: Vec::new(),
            slots: FrameSlots::default(),
            root: None,
            arena: CanonicalArena::new(limits.clone()),
            partial: Vec::new(),
            utf8: Utf8State::default(),
            terminated: false,
            byte_offset: 0,
            limits,
        }
    }

    pub fn feed_token(
        &mut self,
        token_id: TokenId,
        table: &TokenTable,
    ) -> Result<Vec<JsonEvent>, CursorError> {
        let bytes = table.get(token_id)?.bytes();
        let mut events = Vec::new();
        for byte in bytes {
            events.extend(self.feed_byte(*byte)?);
        }
        Ok(events)
    }

    pub fn feed_bytes(&mut self, bytes: &[u8]) -> Result<Vec<JsonEvent>, CursorError> {
        let mut events = Vec::new();
        for byte in bytes {
            events.extend(self.feed_byte(*byte)?);
        }
        Ok(events)
    }

    pub fn feed_byte(&mut self, byte: u8) -> Result<Vec<JsonEvent>, CursorError> {
        if self.terminated {
            return Err(CursorError::AlreadyTerminated);
        }
        let offset = self.byte_offset;
        self.byte_offset = self
            .byte_offset
            .checked_add(1)
            .ok_or(CursorError::OffsetOverflow)?;
        let mut events = Vec::new();
        let mut reprocess = true;
        while reprocess {
            reprocess = false;
            match self.mode.clone() {
                CursorMode::ExpectValue | CursorMode::ExpectArrayValueOrEnd => {
                    if is_whitespace(byte) {
                        continue;
                    }
                    if matches!(self.mode, CursorMode::ExpectArrayValueOrEnd) && byte == b']' {
                        self.close_array(&mut events)?;
                    } else {
                        self.start_value(byte, &mut events, offset)?;
                    }
                }
                CursorMode::ExpectObjectKeyOrEnd => {
                    if is_whitespace(byte) {
                        continue;
                    }
                    match byte {
                        b'}' => self.close_object(&mut events)?,
                        b'"' => self.begin_string(StringContext::Key)?,
                        _ => return Err(self.syntax(offset, "expected object key or '}'")),
                    }
                }
                CursorMode::ExpectColon => {
                    if is_whitespace(byte) {
                        continue;
                    }
                    if byte != b':' {
                        return Err(self.syntax(offset, "expected ':'"));
                    }
                    self.mode = CursorMode::ExpectValue;
                }
                CursorMode::ExpectArrayCommaOrEnd => {
                    if is_whitespace(byte) {
                        continue;
                    }
                    match byte {
                        b',' => self.mode = CursorMode::ExpectValue,
                        b']' => self.close_array(&mut events)?,
                        _ => return Err(self.syntax(offset, "expected ',' or ']'")),
                    }
                }
                CursorMode::ExpectObjectCommaOrEnd => {
                    if is_whitespace(byte) {
                        continue;
                    }
                    match byte {
                        b',' => self.mode = CursorMode::ExpectObjectKeyOrEnd,
                        b'}' => self.close_object(&mut events)?,
                        _ => return Err(self.syntax(offset, "expected ',' or '}'")),
                    }
                }
                CursorMode::CompleteRoot | CursorMode::AfterValue => {
                    if !is_whitespace(byte) {
                        return Err(self.syntax(offset, "trailing non-whitespace after root value"));
                    }
                    self.mode = CursorMode::CompleteRoot;
                }
                CursorMode::InString(context) => match byte {
                    b'"' => self.finish_string(context, &mut events)?,
                    b'\\' if self.utf8.expected_continuations == 0 => {
                        self.mode = CursorMode::AfterBackslash(context)
                    }
                    0x00..=0x1f => return Err(self.syntax(offset, "control byte in string")),
                    _ => {
                        feed_utf8(&mut self.utf8, byte).map_err(|reason| CursorError::Syntax {
                            byte_offset: offset,
                            reason,
                        })?;
                        self.push_partial(byte)?;
                    }
                },
                CursorMode::AfterBackslash(context) => match byte {
                    b'"' | b'\\' | b'/' => {
                        self.push_partial(byte)?;
                        self.mode = CursorMode::InString(context);
                    }
                    b'b' => self.finish_simple_escape(context, 0x08)?,
                    b'f' => self.finish_simple_escape(context, 0x0c)?,
                    b'n' => self.finish_simple_escape(context, b'\n')?,
                    b'r' => self.finish_simple_escape(context, b'\r')?,
                    b't' => self.finish_simple_escape(context, b'\t')?,
                    b'u' => {
                        self.mode = CursorMode::InUnicodeEscape {
                            context,
                            digits: 0,
                            value: 0,
                        }
                    }
                    _ => return Err(self.syntax(offset, "invalid string escape")),
                },
                CursorMode::InUnicodeEscape {
                    context,
                    digits,
                    value,
                } => {
                    let digit =
                        hex(byte).ok_or_else(|| self.syntax(offset, "invalid Unicode escape"))?;
                    let value = (value << 4) | u16::from(digit);
                    if digits == 3 {
                        if (0xd800..=0xdbff).contains(&value) {
                            self.mode = CursorMode::WaitingLowSurrogate {
                                context,
                                high: value,
                                matched: 0,
                            };
                        } else if (0xdc00..=0xdfff).contains(&value) {
                            return Err(self.syntax(offset, "isolated low surrogate"));
                        } else {
                            self.push_codepoint(u32::from(value))?;
                            self.mode = CursorMode::InString(context);
                        }
                    } else {
                        self.mode = CursorMode::InUnicodeEscape {
                            context,
                            digits: digits + 1,
                            value,
                        };
                    }
                }
                CursorMode::WaitingLowSurrogate {
                    context,
                    high,
                    matched,
                } => match matched {
                    0 if byte == b'\\' => {
                        self.mode = CursorMode::WaitingLowSurrogate {
                            context,
                            high,
                            matched: 1,
                        }
                    }
                    1 if byte == b'u' => {
                        self.mode = CursorMode::WaitingLowSurrogate {
                            context,
                            high,
                            matched: 2,
                        }
                    }
                    2..=5 => {
                        let digit = hex(byte)
                            .ok_or_else(|| self.syntax(offset, "invalid low surrogate"))?;
                        let accumulated = u16::from(digit) << (4 * (5 - matched));
                        let low = self.utf8.codepoint as u16 | accumulated;
                        self.utf8.codepoint = u32::from(low);
                        if matched == 5 {
                            if !(0xdc00..=0xdfff).contains(&low) {
                                return Err(self.syntax(offset, "expected low surrogate"));
                            }
                            let codepoint = 0x10000
                                + ((u32::from(high) - 0xd800) << 10)
                                + (u32::from(low) - 0xdc00);
                            self.utf8.codepoint = 0;
                            self.push_codepoint(codepoint)?;
                            self.mode = CursorMode::InString(context);
                        } else {
                            self.mode = CursorMode::WaitingLowSurrogate {
                                context,
                                high,
                                matched: matched + 1,
                            };
                        }
                    }
                    _ => return Err(self.syntax(offset, "expected low surrogate escape")),
                },
                CursorMode::InNumber => {
                    if is_number_byte(byte) {
                        self.push_partial(byte)?;
                        if !number_prefix_valid(&self.partial) {
                            return Err(self.syntax(offset, "invalid number"));
                        }
                    } else if is_delimiter(byte) && number_complete(&self.partial) {
                        self.seal_number(&mut events)?;
                        reprocess = true;
                    } else {
                        return Err(self.syntax(offset, "invalid number continuation"));
                    }
                }
                CursorMode::InTrue { matched } => {
                    reprocess = self.feed_literal(
                        byte,
                        b"true",
                        matched,
                        Literal::Bool(true),
                        &mut events,
                        offset,
                    )?;
                }
                CursorMode::InFalse { matched } => {
                    reprocess = self.feed_literal(
                        byte,
                        b"false",
                        matched,
                        Literal::Bool(false),
                        &mut events,
                        offset,
                    )?;
                }
                CursorMode::InNull { matched } => {
                    reprocess = self.feed_literal(
                        byte,
                        b"null",
                        matched,
                        Literal::Null,
                        &mut events,
                        offset,
                    )?;
                }
            }
            if events.len() > self.limits.max_events_per_byte {
                return Err(CursorError::EventLimit {
                    limit: self.limits.max_events_per_byte,
                    byte_offset: offset,
                });
            }
        }
        Ok(events)
    }

    pub fn finish_eos(&mut self) -> Result<Vec<JsonEvent>, CursorError> {
        if self.terminated {
            return Err(CursorError::AlreadyTerminated);
        }
        let mut events = Vec::new();
        match self.mode {
            CursorMode::InNumber if number_complete(&self.partial) => {
                self.seal_number(&mut events)?
            }
            CursorMode::InTrue { matched: 4 } => {
                self.seal_literal(Literal::Bool(true), &mut events)?
            }
            CursorMode::InFalse { matched: 5 } => {
                self.seal_literal(Literal::Bool(false), &mut events)?
            }
            CursorMode::InNull { matched: 4 } => self.seal_literal(Literal::Null, &mut events)?,
            CursorMode::CompleteRoot => {}
            _ => {
                return Err(CursorError::IncompleteDocument {
                    byte_offset: self.byte_offset,
                })
            }
        }
        if self.root.is_none()
            || !self.frames.is_empty()
            || !matches!(self.mode, CursorMode::CompleteRoot)
        {
            return Err(CursorError::IncompleteDocument {
                byte_offset: self.byte_offset,
            });
        }
        self.terminated = true;
        Ok(events)
    }

    pub fn snapshot(&self) -> CursorSnapshot {
        CursorSnapshot {
            mode: self.mode.clone(),
            frames: self.frames.iter().map(CursorFrame::id).collect(),
            partial_scalar: self.partial.clone(),
            root: self.root,
            terminated: self.terminated,
            byte_offset: self.byte_offset,
        }
    }

    pub fn root(&self) -> Option<CanonicalId> {
        self.root
    }

    pub fn arena(&self) -> &CanonicalArena {
        &self.arena
    }

    fn start_value(
        &mut self,
        byte: u8,
        events: &mut Vec<JsonEvent>,
        offset: usize,
    ) -> Result<(), CursorError> {
        match byte {
            b'[' => self.open_array(events),
            b'{' => self.open_object(events),
            b'"' => self.begin_string(StringContext::Value),
            b'-' | b'0'..=b'9' => {
                self.partial.clear();
                self.push_partial(byte)?;
                self.mode = CursorMode::InNumber;
                Ok(())
            }
            b't' => {
                self.mode = CursorMode::InTrue { matched: 1 };
                Ok(())
            }
            b'f' => {
                self.mode = CursorMode::InFalse { matched: 1 };
                Ok(())
            }
            b'n' => {
                self.mode = CursorMode::InNull { matched: 1 };
                Ok(())
            }
            _ => Err(self.syntax(offset, "expected JSON value")),
        }
    }

    fn open_array(&mut self, events: &mut Vec<JsonEvent>) -> Result<(), CursorError> {
        self.preflight_depth()?;
        let id = self.slots.allocate()?;
        self.frames.push(CursorFrame::Array {
            id,
            values: Vec::new(),
        });
        self.mode = CursorMode::ExpectArrayValueOrEnd;
        events.push(JsonEvent::ArrayStart(id));
        Ok(())
    }

    fn open_object(&mut self, events: &mut Vec<JsonEvent>) -> Result<(), CursorError> {
        self.preflight_depth()?;
        let id = self.slots.allocate()?;
        self.frames.push(CursorFrame::Object {
            id,
            current_key: None,
            values: Vec::new(),
        });
        self.mode = CursorMode::ExpectObjectKeyOrEnd;
        events.push(JsonEvent::ObjectStart(id));
        Ok(())
    }

    fn close_array(&mut self, events: &mut Vec<JsonEvent>) -> Result<(), CursorError> {
        let Some(CursorFrame::Array { id, values }) = self.frames.pop() else {
            return Err(self.syntax(self.byte_offset.saturating_sub(1), "mismatched ']'"));
        };
        let value = self.arena.add_array(&values)?;
        self.slots.release(id)?;
        events.push(JsonEvent::ArrayEnd { frame: id, value });
        self.publish_value(value, events)
    }

    fn close_object(&mut self, events: &mut Vec<JsonEvent>) -> Result<(), CursorError> {
        let Some(CursorFrame::Object {
            id,
            current_key: _,
            values,
        }) = self.frames.pop()
        else {
            return Err(self.syntax(self.byte_offset.saturating_sub(1), "mismatched '}'"));
        };
        let value = self.arena.add_object(values)?;
        self.slots.release(id)?;
        events.push(JsonEvent::ObjectEnd { frame: id, value });
        self.publish_value(value, events)
    }

    fn begin_string(&mut self, context: StringContext) -> Result<(), CursorError> {
        self.partial.clear();
        self.utf8 = Utf8State::default();
        self.mode = CursorMode::InString(context);
        Ok(())
    }

    fn finish_string(
        &mut self,
        context: StringContext,
        events: &mut Vec<JsonEvent>,
    ) -> Result<(), CursorError> {
        if self.utf8.expected_continuations != 0 {
            return Err(self.syntax(
                self.byte_offset.saturating_sub(1),
                "incomplete UTF-8 sequence",
            ));
        }
        if context == StringContext::Key {
            let key = std::mem::take(&mut self.partial);
            let Some(CursorFrame::Object {
                id,
                current_key,
                values,
            }) = self.frames.last_mut()
            else {
                return Err(CursorError::Internal("object key without object frame"));
            };
            if values.iter().any(|(existing, _)| *existing == key) {
                return Err(CursorError::DuplicateObjectKey {
                    byte_offset: self.byte_offset,
                });
            }
            *current_key = Some(key.clone());
            events.push(JsonEvent::PropertyKeySealed { object: *id, key });
            self.mode = CursorMode::ExpectColon;
            return Ok(());
        }
        let value = self.arena.add_string(&self.partial)?;
        self.partial.clear();
        events.push(JsonEvent::ScalarSealed(value));
        self.publish_value(value, events)
    }

    fn finish_simple_escape(
        &mut self,
        context: StringContext,
        byte: u8,
    ) -> Result<(), CursorError> {
        self.push_partial(byte)?;
        self.mode = CursorMode::InString(context);
        Ok(())
    }

    fn push_codepoint(&mut self, codepoint: u32) -> Result<(), CursorError> {
        let character =
            char::from_u32(codepoint).ok_or(CursorError::Internal("invalid Unicode scalar"))?;
        let mut bytes = [0; 4];
        let encoded = character.encode_utf8(&mut bytes);
        for byte in encoded.bytes() {
            self.push_partial(byte)?;
        }
        Ok(())
    }

    fn feed_literal(
        &mut self,
        byte: u8,
        expected: &[u8],
        matched: u8,
        literal: Literal,
        events: &mut Vec<JsonEvent>,
        offset: usize,
    ) -> Result<bool, CursorError> {
        let index = usize::from(matched);
        if index < expected.len() {
            if byte != expected[index] {
                return Err(self.syntax(offset, "invalid literal"));
            }
            let next = matched + 1;
            self.mode = match literal {
                Literal::Bool(true) => CursorMode::InTrue { matched: next },
                Literal::Bool(false) => CursorMode::InFalse { matched: next },
                Literal::Null => CursorMode::InNull { matched: next },
            };
            return Ok(false);
        }
        if !is_delimiter(byte) {
            return Err(self.syntax(offset, "invalid literal boundary"));
        }
        self.seal_literal(literal, events)?;
        Ok(true)
    }

    fn seal_literal(
        &mut self,
        literal: Literal,
        events: &mut Vec<JsonEvent>,
    ) -> Result<(), CursorError> {
        let value = match literal {
            Literal::Bool(value) => self.arena.add_bool(value)?,
            Literal::Null => self.arena.add_null()?,
        };
        events.push(JsonEvent::ScalarSealed(value));
        self.publish_value(value, events)
    }

    fn seal_number(&mut self, events: &mut Vec<JsonEvent>) -> Result<(), CursorError> {
        let number = CanonicalNumber::parse(&self.partial, &self.limits)?;
        let value = self.arena.add_number(number)?;
        self.partial.clear();
        events.push(JsonEvent::ScalarSealed(value));
        self.publish_value(value, events)
    }

    fn publish_value(
        &mut self,
        value: CanonicalId,
        events: &mut Vec<JsonEvent>,
    ) -> Result<(), CursorError> {
        match self.frames.last_mut() {
            Some(CursorFrame::Array { id, values }) => {
                if values.len() >= self.limits.max_array_items {
                    return Err(CursorError::ItemLimit {
                        limit: self.limits.max_array_items,
                    });
                }
                let index = u64::try_from(values.len()).map_err(|_| CursorError::OffsetOverflow)?;
                values.try_reserve(1).map_err(|_| CursorError::Allocation)?;
                values.push(value);
                events.push(JsonEvent::ItemSealed {
                    array: *id,
                    index,
                    value,
                });
                self.mode = CursorMode::ExpectArrayCommaOrEnd;
            }
            Some(CursorFrame::Object {
                id,
                current_key,
                values,
            }) => {
                if values.len() >= self.limits.max_object_members {
                    return Err(CursorError::MemberLimit {
                        limit: self.limits.max_object_members,
                    });
                }
                let key = current_key
                    .take()
                    .ok_or(CursorError::Internal("property value without key"))?;
                values.try_reserve(1).map_err(|_| CursorError::Allocation)?;
                values.push((key.clone(), value));
                events.push(JsonEvent::PropertyValueSealed {
                    object: *id,
                    key,
                    value,
                });
                self.mode = CursorMode::ExpectObjectCommaOrEnd;
            }
            None => {
                if self.root.replace(value).is_some() {
                    return Err(CursorError::Internal("root value published twice"));
                }
                self.mode = CursorMode::CompleteRoot;
                events.push(JsonEvent::RootComplete(value));
            }
        }
        Ok(())
    }

    fn push_partial(&mut self, byte: u8) -> Result<(), CursorError> {
        if self.partial.len() >= self.limits.max_value_bytes {
            return Err(CursorError::ValueLimit {
                limit: self.limits.max_value_bytes,
            });
        }
        self.partial
            .try_reserve(1)
            .map_err(|_| CursorError::Allocation)?;
        self.partial.push(byte);
        Ok(())
    }

    fn preflight_depth(&mut self) -> Result<(), CursorError> {
        if self.frames.len() >= self.limits.max_nesting_depth {
            return Err(CursorError::DepthLimit {
                limit: self.limits.max_nesting_depth,
            });
        }
        self.frames
            .try_reserve(1)
            .map_err(|_| CursorError::Allocation)
    }

    fn syntax(&self, byte_offset: usize, reason: &'static str) -> CursorError {
        CursorError::Syntax {
            byte_offset,
            reason,
        }
    }
}

impl CursorFrame {
    fn id(&self) -> FrameId {
        match self {
            Self::Array { id, .. } | Self::Object { id, .. } => *id,
        }
    }
}

#[derive(Clone, Copy)]
enum Literal {
    Bool(bool),
    Null,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CursorError {
    #[error("JSON syntax error at byte {byte_offset}: {reason}")]
    Syntax {
        byte_offset: usize,
        reason: &'static str,
    },
    #[error("JSON document is incomplete at byte {byte_offset}")]
    IncompleteDocument { byte_offset: usize },
    #[error("cursor is already terminated")]
    AlreadyTerminated,
    #[error("duplicate decoded object key at byte {byte_offset}")]
    DuplicateObjectKey { byte_offset: usize },
    #[error("nesting limit {limit} exceeded")]
    DepthLimit { limit: usize },
    #[error("value-byte limit {limit} exceeded")]
    ValueLimit { limit: usize },
    #[error("array-item limit {limit} exceeded")]
    ItemLimit { limit: usize },
    #[error("object-member limit {limit} exceeded")]
    MemberLimit { limit: usize },
    #[error("event limit {limit} exceeded at byte {byte_offset}")]
    EventLimit { limit: usize, byte_offset: usize },
    #[error("cursor offset overflow")]
    OffsetOverflow,
    #[error("cursor allocation failed")]
    Allocation,
    #[error("cursor internal invariant failed: {0}")]
    Internal(&'static str),
    #[error(transparent)]
    Arena(#[from] ArenaError),
    #[error(transparent)]
    Number(#[from] NumberError),
    #[error(transparent)]
    Vocabulary(#[from] VocabularyError),
    #[error(transparent)]
    Scope(#[from] ScopeError),
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\r' | b'\t')
}

fn is_delimiter(byte: u8) -> bool {
    is_whitespace(byte) || matches!(byte, b',' | b']' | b'}')
}

fn is_number_byte(byte: u8) -> bool {
    matches!(byte, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
}

fn number_complete(bytes: &[u8]) -> bool {
    bytes.last().is_some_and(u8::is_ascii_digit) && number_prefix_valid(bytes)
}

fn number_prefix_valid(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let mut chars = text.chars().peekable();
    if chars.peek() == Some(&'-') {
        chars.next();
    }
    match chars.next() {
        Some('0') if chars.peek().is_some_and(char::is_ascii_digit) => return false,
        Some('0'..='9') => {
            while chars.peek().is_some_and(char::is_ascii_digit) {
                chars.next();
            }
        }
        None => return text == "-",
        _ => return false,
    }
    if chars.peek() == Some(&'.') {
        chars.next();
        while chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
        }
    }
    if matches!(chars.peek(), Some('e' | 'E')) {
        chars.next();
        if matches!(chars.peek(), Some('+' | '-')) {
            chars.next();
        }
        while chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
        }
    }
    chars.next().is_none()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn feed_utf8(state: &mut Utf8State, byte: u8) -> Result<(), &'static str> {
    if state.expected_continuations == 0 {
        match byte {
            0x00..=0x7f => return Ok(()),
            0xc2..=0xdf => {
                state.expected_continuations = 1;
                state.codepoint = u32::from(byte & 0x1f);
                state.minimum_codepoint = 0x80;
            }
            0xe0..=0xef => {
                state.expected_continuations = 2;
                state.codepoint = u32::from(byte & 0x0f);
                state.minimum_codepoint = 0x800;
            }
            0xf0..=0xf4 => {
                state.expected_continuations = 3;
                state.codepoint = u32::from(byte & 0x07);
                state.minimum_codepoint = 0x10000;
            }
            _ => return Err("invalid UTF-8 leading byte"),
        }
        return Ok(());
    }
    if !(0x80..=0xbf).contains(&byte) {
        return Err("invalid UTF-8 continuation byte");
    }
    state.codepoint = (state.codepoint << 6) | u32::from(byte & 0x3f);
    state.expected_continuations -= 1;
    if state.expected_continuations == 0 {
        if state.codepoint < state.minimum_codepoint
            || (0xd800..=0xdfff).contains(&state.codepoint)
            || state.codepoint > 0x10ffff
        {
            return Err("invalid UTF-8 scalar value");
        }
        state.codepoint = 0;
        state.minimum_codepoint = 0;
    }
    Ok(())
}

#[allow(dead_code)]
fn _schema_node_marker(_: SchemaNodeId) {}
