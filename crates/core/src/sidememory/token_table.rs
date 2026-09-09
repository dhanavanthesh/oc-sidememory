use std::mem::size_of;

use bitflags::bitflags;
use thiserror::Error;

use crate::primitives::TokenId;
use crate::vocabulary::Vocabulary;

use super::limits::{CompileLimits, ResourceError};

#[derive(Clone, Debug)]
pub struct TokenTable {
    entries: Box<[Option<TokenInfo>]>,
    eos: TokenId,
    model_width: usize,
    total_token_bytes: usize,
}

#[derive(Clone, Debug)]
pub struct TokenInfo {
    bytes: Box<[u8]>,
    flags: TokenFlags,
}

bitflags! {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct TokenFlags: u16 {
        const HAS_QUOTE = 1 << 0;
        const HAS_BACKSLASH = 1 << 1;
        const HAS_COMMA = 1 << 2;
        const HAS_COLON = 1 << 3;
        const HAS_LBRACKET = 1 << 4;
        const HAS_RBRACKET = 1 << 5;
        const HAS_LBRACE = 1 << 6;
        const HAS_RBRACE = 1 << 7;
        const HAS_DIGIT = 1 << 8;
        const MAY_SEAL_VALUE = 1 << 9;
    }
}

impl TokenTable {
    pub fn build(
        vocabulary: &Vocabulary,
        model_width: usize,
        limits: &CompileLimits,
    ) -> Result<Self, VocabularyError> {
        if model_width == 0 {
            return Err(VocabularyError::InvalidModelWidth);
        }
        if model_width > limits.max_model_width {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_model_width",
                limit_value: limits.max_model_width,
                requested: model_width,
            }
            .into());
        }
        let eos = vocabulary.eos_token_id();
        if eos as usize >= model_width {
            return Err(VocabularyError::TokenOutsideModelWidth {
                id: eos,
                model_width,
            });
        }
        let allocation = model_width
            .checked_mul(size_of::<Option<TokenInfo>>())
            .ok_or(ResourceError::ArithmeticOverflow {
                context: "token table",
            })?;
        if allocation > limits.max_token_table_bytes {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_token_table_bytes",
                limit_value: limits.max_token_table_bytes,
                requested: allocation,
            }
            .into());
        }
        let mut entries: Vec<Option<TokenInfo>> = Vec::new();
        entries
            .try_reserve_exact(model_width)
            .map_err(|_| ResourceError::Allocation {
                context: "token table",
                requested: model_width,
            })?;
        entries.resize_with(model_width, || None);
        let mut total_token_bytes = 0_usize;

        for (bytes, ids) in vocabulary.tokens() {
            if bytes.is_empty() {
                return Err(VocabularyError::EmptyOrdinaryToken);
            }
            for id in ids {
                if *id == eos {
                    return Err(VocabularyError::EosUsedAsOrdinaryToken { id: *id });
                }
                let index = *id as usize;
                if index >= model_width {
                    return Err(VocabularyError::TokenOutsideModelWidth {
                        id: *id,
                        model_width,
                    });
                }
                match &entries[index] {
                    Some(existing) if existing.bytes.as_ref() == bytes.as_slice() => continue,
                    Some(existing) => {
                        return Err(VocabularyError::ConflictingTokenBytes {
                            id: *id,
                            first: existing.bytes.clone(),
                            second: bytes.clone().into_boxed_slice(),
                        });
                    }
                    None => {}
                }
                total_token_bytes = total_token_bytes.checked_add(bytes.len()).ok_or(
                    ResourceError::ArithmeticOverflow {
                        context: "token bytes",
                    },
                )?;
                if total_token_bytes > limits.max_total_token_bytes {
                    return Err(ResourceError::LimitExceeded {
                        limit_name: "max_total_token_bytes",
                        limit_value: limits.max_total_token_bytes,
                        requested: total_token_bytes,
                    }
                    .into());
                }
                entries[index] = Some(TokenInfo {
                    bytes: bytes.clone().into_boxed_slice(),
                    flags: flags(bytes),
                });
            }
        }
        Ok(Self {
            entries: entries.into_boxed_slice(),
            eos,
            model_width,
            total_token_bytes,
        })
    }

    pub fn get(&self, id: TokenId) -> Result<&TokenInfo, VocabularyError> {
        if id == self.eos {
            return Err(VocabularyError::EosIsControl { id });
        }
        self.entries
            .get(id as usize)
            .and_then(Option::as_ref)
            .ok_or(VocabularyError::UnknownToken { id })
    }

    pub fn eos_token_id(&self) -> TokenId {
        self.eos
    }

    pub fn model_width(&self) -> usize {
        self.model_width
    }

    pub fn total_token_bytes(&self) -> usize {
        self.total_token_bytes
    }
}

impl TokenInfo {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn flags(&self) -> TokenFlags {
        self.flags
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum VocabularyError {
    #[error("model width must be greater than zero")]
    InvalidModelWidth,
    #[error("token {id} is outside model width {model_width}")]
    TokenOutsideModelWidth { id: TokenId, model_width: usize },
    #[error("EOS token {id} appears as an ordinary token")]
    EosUsedAsOrdinaryToken { id: TokenId },
    #[error("ordinary token is empty")]
    EmptyOrdinaryToken,
    #[error("token {id} has conflicting processed bytes")]
    ConflictingTokenBytes {
        id: TokenId,
        first: Box<[u8]>,
        second: Box<[u8]>,
    },
    #[error("unknown token {id}")]
    UnknownToken { id: TokenId },
    #[error("EOS token {id} is a control operation")]
    EosIsControl { id: TokenId },
    #[error(transparent)]
    Resource(#[from] ResourceError),
}

fn flags(bytes: &[u8]) -> TokenFlags {
    let mut result = TokenFlags::empty();
    for byte in bytes {
        result |= match byte {
            b'"' => TokenFlags::HAS_QUOTE | TokenFlags::MAY_SEAL_VALUE,
            b'\\' => TokenFlags::HAS_BACKSLASH,
            b',' => TokenFlags::HAS_COMMA | TokenFlags::MAY_SEAL_VALUE,
            b':' => TokenFlags::HAS_COLON,
            b'[' => TokenFlags::HAS_LBRACKET,
            b']' => TokenFlags::HAS_RBRACKET | TokenFlags::MAY_SEAL_VALUE,
            b'{' => TokenFlags::HAS_LBRACE,
            b'}' => TokenFlags::HAS_RBRACE | TokenFlags::MAY_SEAL_VALUE,
            b'0'..=b'9' => TokenFlags::HAS_DIGIT,
            _ => TokenFlags::empty(),
        };
    }
    result
}
