use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FrameId {
    pub slot: u32,
    pub generation: u32,
}

#[derive(Default)]
pub struct FrameSlots {
    slots: Vec<Slot>,
    free: Vec<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FrameSlotUndo {
    RemoveNew { id: FrameId },
    RestoreReused { id: FrameId, generation: u32 },
    RestoreReleased { id: FrameId },
}

#[derive(Default)]
struct Slot {
    generation: u32,
    occupied: bool,
}

impl FrameSlots {
    pub fn allocate(&mut self) -> Result<FrameId, ScopeError> {
        self.allocate_with_undo().map(|(id, _)| id)
    }

    pub(crate) fn allocate_with_undo(&mut self) -> Result<(FrameId, FrameSlotUndo), ScopeError> {
        if let Some(&slot) = self.free.last() {
            let entry = self
                .slots
                .get_mut(slot as usize)
                .ok_or(ScopeError::Invariant)?;
            let generation = entry.generation;
            let next_generation = generation
                .checked_add(1)
                .ok_or(ScopeError::GenerationExhausted)?;
            self.free.pop();
            entry.generation = next_generation;
            entry.occupied = true;
            let id = FrameId {
                slot,
                generation: next_generation,
            };
            return Ok((id, FrameSlotUndo::RestoreReused { id, generation }));
        }
        let slot = u32::try_from(self.slots.len()).map_err(|_| ScopeError::SlotExhausted)?;
        self.slots
            .try_reserve_exact(1)
            .map_err(|_| ScopeError::Allocation)?;
        self.slots.push(Slot {
            generation: 0,
            occupied: true,
        });
        let id = FrameId {
            slot,
            generation: 0,
        };
        Ok((id, FrameSlotUndo::RemoveNew { id }))
    }

    pub fn release(&mut self, id: FrameId) -> Result<(), ScopeError> {
        self.release_with_undo(id).map(|_| ())
    }

    pub(crate) fn release_with_undo(&mut self, id: FrameId) -> Result<FrameSlotUndo, ScopeError> {
        let entry = self
            .slots
            .get_mut(id.slot as usize)
            .ok_or(ScopeError::StaleFrame(id))?;
        if !entry.occupied || entry.generation != id.generation {
            return Err(ScopeError::StaleFrame(id));
        }
        self.free
            .try_reserve_exact(1)
            .map_err(|_| ScopeError::Allocation)?;
        entry.occupied = false;
        self.free.push(id.slot);
        Ok(FrameSlotUndo::RestoreReleased { id })
    }

    pub(crate) fn undo(&mut self, undo: FrameSlotUndo) -> Result<(), ScopeError> {
        match undo {
            FrameSlotUndo::RemoveNew { id } => {
                let slot = self.slots.pop().ok_or(ScopeError::Invariant)?;
                if usize::try_from(id.slot).map_err(|_| ScopeError::Invariant)? != self.slots.len()
                    || !slot.occupied
                    || slot.generation != id.generation
                {
                    return Err(ScopeError::Invariant);
                }
            }
            FrameSlotUndo::RestoreReused { id, generation } => {
                let slot = self
                    .slots
                    .get_mut(id.slot as usize)
                    .ok_or(ScopeError::Invariant)?;
                if !slot.occupied || slot.generation != id.generation {
                    return Err(ScopeError::Invariant);
                }
                slot.occupied = false;
                slot.generation = generation;
                self.free.push(id.slot);
            }
            FrameSlotUndo::RestoreReleased { id } => {
                if self.free.pop() != Some(id.slot) {
                    return Err(ScopeError::Invariant);
                }
                let slot = self
                    .slots
                    .get_mut(id.slot as usize)
                    .ok_or(ScopeError::Invariant)?;
                if slot.occupied || slot.generation != id.generation {
                    return Err(ScopeError::Invariant);
                }
                slot.occupied = true;
            }
        }
        Ok(())
    }

    pub fn is_live(&self, id: FrameId) -> bool {
        self.slots
            .get(id.slot as usize)
            .is_some_and(|entry| entry.occupied && entry.generation == id.generation)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ScopeError {
    #[error("stale frame handle {0:?}")]
    StaleFrame(FrameId),
    #[error("frame generation exhausted")]
    GenerationExhausted,
    #[error("frame slot domain exhausted")]
    SlotExhausted,
    #[error("frame slot allocation failed")]
    Allocation,
    #[error("frame-slot internal invariant failed")]
    Invariant,
}
