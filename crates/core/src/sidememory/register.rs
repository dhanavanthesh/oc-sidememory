use std::collections::HashMap;
use std::mem::size_of;

use thiserror::Error;

use crate::json_schema::extensions::RegisterId;

use super::journal::{JournalError, SemanticJournal, SemanticUndo};
use super::limits::ResourceError;
use super::{CanonicalId, FrameId};

pub type RegisterScope = HashMap<RegisterId, CanonicalId>;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RegisterKey {
    pub object: FrameId,
    pub register: RegisterId,
}

#[derive(Debug, Default)]
pub struct RegisterStore {
    scopes: HashMap<FrameId, RegisterScope>,
    value_count: usize,
}

impl RegisterStore {
    #[cfg(debug_assertions)]
    pub(crate) fn logical_equal(
        &self,
        arena: &super::CanonicalArena,
        other: &Self,
        other_arena: &super::CanonicalArena,
    ) -> Result<bool, super::ArenaError> {
        if self.value_count != other.value_count || self.scopes.len() != other.scopes.len() {
            return Ok(false);
        }
        for (object, scope) in &self.scopes {
            let Some(other_scope) = other.scopes.get(object) else {
                return Ok(false);
            };
            if scope.len() != other_scope.len() {
                return Ok(false);
            }
            for (register, value) in scope {
                let Some(other_value) = other_scope.get(register) else {
                    return Ok(false);
                };
                if !arena.equal_across(*value, other_arena, *other_value)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    pub(crate) fn open_journaled(
        &mut self,
        object: FrameId,
        max_scopes: usize,
        journal: &mut SemanticJournal,
    ) -> Result<(), RegisterError> {
        if self.scopes.contains_key(&object) {
            return Err(RegisterError::DuplicateScope(object));
        }
        let requested =
            self.scopes
                .len()
                .checked_add(1)
                .ok_or(ResourceError::ArithmeticOverflow {
                    context: "register scopes",
                })?;
        if requested > max_scopes {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_register_scopes",
                limit_value: max_scopes,
                requested,
            }
            .into());
        }
        self.scopes
            .try_reserve(1)
            .map_err(|_| ResourceError::Allocation {
                context: "register scopes",
                requested: 1,
            })?;
        journal.preflight(1)?;
        journal.push_preflighted(SemanticUndo::RemoveCreatedRegisterScope { object });
        self.scopes.insert(object, RegisterScope::new());
        Ok(())
    }

    pub(crate) fn set_journaled(
        &mut self,
        key: RegisterKey,
        value: CanonicalId,
        max_values: usize,
        journal: &mut SemanticJournal,
    ) -> Result<(), RegisterError> {
        let previous = self
            .scopes
            .get(&key.object)
            .ok_or(RegisterError::MissingScope(key.object))?
            .get(&key.register)
            .copied();
        let next_count = self
            .value_count
            .checked_add(usize::from(previous.is_none()))
            .ok_or(ResourceError::ArithmeticOverflow {
                context: "register values",
            })?;
        if next_count > max_values {
            return Err(ResourceError::LimitExceeded {
                limit_name: "max_register_values",
                limit_value: max_values,
                requested: next_count,
            }
            .into());
        }
        self.scopes
            .get_mut(&key.object)
            .ok_or(RegisterError::MissingScope(key.object))?
            .try_reserve(usize::from(previous.is_none()))
            .map_err(|_| ResourceError::Allocation {
                context: "register values",
                requested: 1,
            })?;
        journal.preflight(1)?;
        journal.push_preflighted(SemanticUndo::RestoreRegister { key, previous });
        self.scopes
            .get_mut(&key.object)
            .ok_or(RegisterError::MissingScope(key.object))?
            .insert(key.register, value);
        self.value_count = next_count;
        Ok(())
    }

    pub(crate) fn get(&self, key: RegisterKey) -> Option<CanonicalId> {
        self.scopes.get(&key.object)?.get(&key.register).copied()
    }

    pub(crate) fn close_journaled(
        &mut self,
        object: FrameId,
        journal: &mut SemanticJournal,
    ) -> Result<(), RegisterError> {
        journal.preflight(1)?;
        let scope = self
            .scopes
            .remove(&object)
            .ok_or(RegisterError::MissingScope(object))?;
        self.value_count = self
            .value_count
            .checked_sub(scope.len())
            .ok_or(RegisterError::Invariant)?;
        journal.push_preflighted(SemanticUndo::RestoreClosedRegisterScope { object, scope });
        Ok(())
    }

    pub(crate) fn undo(&mut self, undo: SemanticUndo) -> Result<(), JournalError> {
        match undo {
            SemanticUndo::RemoveCreatedRegisterScope { object } => {
                let scope = self.scopes.remove(&object).ok_or(JournalError::Invariant)?;
                if !scope.is_empty() {
                    return Err(JournalError::Invariant);
                }
            }
            SemanticUndo::RestoreRegister { key, previous } => {
                let scope = self
                    .scopes
                    .get_mut(&key.object)
                    .ok_or(JournalError::Invariant)?;
                match previous {
                    Some(value) => {
                        scope.insert(key.register, value);
                    }
                    None => {
                        scope.remove(&key.register).ok_or(JournalError::Invariant)?;
                        self.value_count = self
                            .value_count
                            .checked_sub(1)
                            .ok_or(JournalError::Invariant)?;
                    }
                }
            }
            SemanticUndo::RestoreClosedRegisterScope { object, scope } => {
                self.value_count = self
                    .value_count
                    .checked_add(scope.len())
                    .ok_or(JournalError::Invariant)?;
                if self.scopes.insert(object, scope).is_some() {
                    return Err(JournalError::Invariant);
                }
            }
            _ => return Err(JournalError::Invariant),
        }
        Ok(())
    }

    #[cfg(debug_assertions)]
    pub(crate) fn logical_snapshot(&self) -> Vec<(RegisterKey, CanonicalId)> {
        let mut values = Vec::with_capacity(self.value_count);
        for (object, scope) in &self.scopes {
            values.extend(scope.iter().map(|(register, value)| {
                (
                    RegisterKey {
                        object: *object,
                        register: *register,
                    },
                    *value,
                )
            }));
        }
        values.sort_unstable_by_key(|(key, _)| {
            (key.object.slot, key.object.generation, key.register.get())
        });
        values
    }
}

pub(crate) fn retained_scope_bytes(scope: &RegisterScope) -> Result<usize, ResourceError> {
    scope
        .capacity()
        .checked_mul(size_of::<(RegisterId, CanonicalId)>())
        .ok_or(ResourceError::ArithmeticOverflow {
            context: "retained register scope",
        })
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RegisterError {
    #[error("missing register scope {0:?}")]
    MissingScope(FrameId),
    #[error("duplicate register scope {0:?}")]
    DuplicateScope(FrameId),
    #[error("register invariant failed")]
    Invariant,
    #[error(transparent)]
    Resource(#[from] ResourceError),
    #[error(transparent)]
    Journal(#[from] JournalError),
}
