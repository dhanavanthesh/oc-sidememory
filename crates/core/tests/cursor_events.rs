use oc_sidememory::sidememory::{JsonCursor, JsonEvent, RuntimeLimits};

#[test]
fn number_delimiter_emits_scalar_item_array_and_root_events_in_order() {
    let mut cursor = JsonCursor::new(RuntimeLimits::default());
    let mut events = cursor.feed_bytes(b"[12").unwrap();
    events.extend(cursor.feed_byte(b']').unwrap());
    assert!(matches!(events[0], JsonEvent::ArrayStart(_)));
    assert!(matches!(events[1], JsonEvent::ScalarSealed(_)));
    assert!(matches!(events[2], JsonEvent::ItemSealed { index: 0, .. }));
    assert!(matches!(events[3], JsonEvent::ArrayEnd { .. }));
    assert!(matches!(events[4], JsonEvent::RootComplete(_)));
}

#[test]
fn adversarial_multi_event_chunks_are_processed_causally() {
    for document in [
        b"[12]".as_slice(),
        b"[true,false]".as_slice(),
        b"[\"value\"]".as_slice(),
        b"{\"a\":{},\"next\":1}".as_slice(),
        b"[\"a\",\"b\"]".as_slice(),
    ] {
        let mut cursor = JsonCursor::new(RuntimeLimits::default());
        let events = cursor.feed_bytes(document).unwrap();
        cursor.finish_eos().unwrap();
        assert!(events
            .iter()
            .any(|event| matches!(event, JsonEvent::RootComplete(_))));
    }
}

#[test]
fn property_events_are_not_array_item_events() {
    let mut cursor = JsonCursor::new(RuntimeLimits::default());
    let events = cursor.feed_bytes(br#"{"a":1}"#).unwrap();
    assert!(events
        .iter()
        .any(|event| matches!(event, JsonEvent::PropertyKeySealed { key, .. } if key == b"a")));
    assert!(events
        .iter()
        .any(|event| matches!(event, JsonEvent::PropertyValueSealed { key, .. } if key == b"a")));
    assert!(!events
        .iter()
        .any(|event| matches!(event, JsonEvent::ItemSealed { .. })));
}
