use oc_sidememory::sidememory::{CursorError, JsonCursor, RuntimeLimits};

#[test]
fn eos_seals_a_complete_root_number_and_literal() {
    for document in [b"12".as_slice(), b"true".as_slice(), b"null".as_slice()] {
        let mut cursor = JsonCursor::new(RuntimeLimits::default());
        cursor.feed_bytes(document).unwrap();
        cursor.finish_eos().unwrap();
        assert!(cursor.root().is_some());
    }
}

#[test]
fn eos_rejects_partial_values_and_open_containers() {
    for document in [
        b"\"x".as_slice(),
        b"[1".as_slice(),
        b"tru".as_slice(),
        b"1e".as_slice(),
    ] {
        let mut cursor = JsonCursor::new(RuntimeLimits::default());
        cursor.feed_bytes(document).unwrap();
        assert!(matches!(
            cursor.finish_eos(),
            Err(CursorError::IncompleteDocument { .. })
        ));
    }
}

#[test]
fn eos_is_a_one_way_control_operation() {
    let mut cursor = JsonCursor::new(RuntimeLimits::default());
    cursor.feed_bytes(b"null").unwrap();
    cursor.finish_eos().unwrap();
    assert_eq!(cursor.finish_eos(), Err(CursorError::AlreadyTerminated));
    assert_eq!(cursor.feed_byte(b' '), Err(CursorError::AlreadyTerminated));
}
