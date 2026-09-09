#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CanonicalId(pub(crate) u32);

impl CanonicalId {
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteSpan {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChildSpan {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntrySpan {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectEntry {
    pub(crate) key: ByteSpan,
    pub(crate) value: CanonicalId,
}
