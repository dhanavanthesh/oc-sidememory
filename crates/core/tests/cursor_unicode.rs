mod common;

use oc_sidememory::sidememory::{CursorError, JsonCursor, RuntimeLimits};

#[test]
fn raw_and_escaped_unicode_build_the_same_string() {
    let raw = common::parse("\"é\"".as_bytes());
    let escaped = common::parse(br#""\u00e9""#);
    assert!(raw
        .arena()
        .equal_across(
            raw.root().unwrap(),
            escaped.arena(),
            escaped.root().unwrap()
        )
        .unwrap());
}

#[test]
fn utf8_escape_digits_and_surrogates_work_across_every_boundary() {
    for document in ["\"é\"".as_bytes(), br#""\u00e9""#, br#""\uD83D\uDE00""#] {
        for split in 0..=document.len() {
            let mut cursor = JsonCursor::new(RuntimeLimits::default());
            cursor.feed_bytes(&document[..split]).unwrap();
            cursor.feed_bytes(&document[split..]).unwrap();
            cursor.finish_eos().unwrap();
        }
    }
}

#[test]
fn malformed_utf8_and_surrogates_are_rejected() {
    for document in [
        b"\"\xc0\x80\"".as_slice(),
        b"\"\xed\xa0\x80\"".as_slice(),
        br#""\uDE00""#,
        br#""\uD83Dx""#,
        br#""\uD83D\u0041""#,
    ] {
        let mut cursor = JsonCursor::new(RuntimeLimits::default());
        assert!(
            matches!(cursor.feed_bytes(document), Err(CursorError::Syntax { .. })),
            "{document:?}"
        );
    }
}

#[test]
fn unicode_normalization_is_not_applied() {
    let composed = common::parse("\"é\"".as_bytes());
    let decomposed = common::parse("\"e\u{301}\"".as_bytes());
    assert!(!composed
        .arena()
        .equal_across(
            composed.root().unwrap(),
            decomposed.arena(),
            decomposed.root().unwrap(),
        )
        .unwrap());
}
