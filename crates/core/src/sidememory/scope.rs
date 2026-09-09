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

#[derive(Default)]
struct Slot {
    generation: u32,
    occupied: bool,
}

impl FrameSlots {
    pub fn allocate(&mut self) -> Result<FrameId, ScopeError> {
        if let Some(slot) = self.free.pop() {
            let entry = self
                .slots
                .get_mut(slot as usize)
                .ok_or(ScopeError::Invariant)?;
            entry.generation = entry
                .generation
                .checked_add(1)
                .ok_or(ScopeError::GenerationExhausted)?;
            entry.occupied = true;
            return Ok(FrameId {
                slot,
                generation: entry.generation,
            });
        }
        let slot = u32::try_from(self.slots.len()).map_err(|_| ScopeError::SlotExhausted)?;
        self.slots.push(Slot {
            generation: 0,
            occupied: true,
        });
        Ok(FrameId {
            slot,
            generation: 0,
        })
    }

    pub fn release(&mut self, id: FrameId) -> Result<(), ScopeError> {
        let entry = self
            .slots
            .get_mut(id.slot as usize)
            .ok_or(ScopeError::StaleFrame(id))?;
        if !entry.occupied || entry.generation != id.generation {
            return Err(ScopeError::StaleFrame(id));
        }
        entry.occupied = false;
        self.free.push(id.slot);
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
    #[error("frame-slot internal invariant failed")]
    Invariant,
}
