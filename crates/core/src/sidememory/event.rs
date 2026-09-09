use super::scope::FrameId;
use super::value::CanonicalId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JsonEvent {
    ArrayStart(FrameId),
    ObjectStart(FrameId),
    PropertyKeySealed {
        object: FrameId,
        key: Vec<u8>,
    },
    ScalarSealed(CanonicalId),
    ItemSealed {
        array: FrameId,
        index: u64,
        value: CanonicalId,
    },
    PropertyValueSealed {
        object: FrameId,
        key: Vec<u8>,
        value: CanonicalId,
    },
    ArrayEnd {
        frame: FrameId,
        value: CanonicalId,
    },
    ObjectEnd {
        frame: FrameId,
        value: CanonicalId,
    },
    RootComplete(CanonicalId),
}
