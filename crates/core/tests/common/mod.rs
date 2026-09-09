#![allow(dead_code)]

use oc_sidememory::sidememory::{JsonCursor, RuntimeLimits};
use oc_sidememory::Vocabulary;

pub fn ascii_vocabulary() -> (Vocabulary, usize) {
    let eos = 128;
    let mut vocabulary = Vocabulary::new(eos);
    for byte in 0_u8..=127 {
        vocabulary
            .try_insert(vec![byte], u32::from(byte))
            .expect("ASCII token");
    }
    (vocabulary, 129)
}

pub fn byte_vocabulary() -> (Vocabulary, usize) {
    let eos = 256;
    let mut vocabulary = Vocabulary::new(eos);
    for byte in 0_u8..=u8::MAX {
        vocabulary
            .try_insert(vec![byte], u32::from(byte))
            .expect("byte token");
    }
    (vocabulary, 257)
}

pub fn parse(bytes: &[u8]) -> JsonCursor {
    let mut cursor = JsonCursor::new(RuntimeLimits::default());
    cursor.feed_bytes(bytes).expect("valid JSON bytes");
    cursor.finish_eos().expect("complete JSON document");
    cursor
}
