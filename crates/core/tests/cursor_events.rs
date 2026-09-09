use oc_sidememory::sidememory::{CompileLimits, JsonCursor, JsonEvent, RuntimeLimits, TokenTable};
use oc_sidememory::Vocabulary;

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
    let events = feed_single_token(b"[", b"12]");
    assert_eq!(events.len(), 4);
    assert!(matches!(events[0], JsonEvent::ScalarSealed(_)));
    assert!(matches!(events[1], JsonEvent::ItemSealed { index: 0, .. }));
    assert!(matches!(events[2], JsonEvent::ArrayEnd { .. }));
    assert!(matches!(events[3], JsonEvent::RootComplete(_)));

    let events = feed_single_token(b"[", b"true,");
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], JsonEvent::ScalarSealed(_)));
    assert!(matches!(events[1], JsonEvent::ItemSealed { index: 0, .. }));

    let events = feed_single_token(b"[", b"\"value\"]");
    assert_eq!(events.len(), 4);
    assert!(matches!(events[0], JsonEvent::ScalarSealed(_)));
    assert!(matches!(events[1], JsonEvent::ItemSealed { index: 0, .. }));
    assert!(matches!(events[2], JsonEvent::ArrayEnd { .. }));
    assert!(matches!(events[3], JsonEvent::RootComplete(_)));

    let events = feed_single_token(br#"{"a":{"#, br#"},"next":"#);
    assert_eq!(events.len(), 3);
    assert!(matches!(events[0], JsonEvent::ObjectEnd { .. }));
    assert!(matches!(
        &events[1],
        JsonEvent::PropertyValueSealed { key, .. } if key == b"a"
    ));
    assert!(matches!(
        &events[2],
        JsonEvent::PropertyKeySealed { key, .. } if key == b"next"
    ));

    let events = feed_single_token(b"[true", b",false]");
    assert_eq!(events.len(), 6);
    assert!(matches!(events[0], JsonEvent::ScalarSealed(_)));
    assert!(matches!(events[1], JsonEvent::ItemSealed { index: 0, .. }));
    assert!(matches!(events[2], JsonEvent::ScalarSealed(_)));
    assert!(matches!(events[3], JsonEvent::ItemSealed { index: 1, .. }));
    assert!(matches!(events[4], JsonEvent::ArrayEnd { .. }));
    assert!(matches!(events[5], JsonEvent::RootComplete(_)));

    let events = feed_single_token(br#"{"a":1"#, br#", "b":2}"#);
    assert_eq!(events.len(), 7);
    assert!(matches!(events[0], JsonEvent::ScalarSealed(_)));
    assert!(matches!(
        &events[1],
        JsonEvent::PropertyValueSealed { key, .. } if key == b"a"
    ));
    assert!(matches!(
        &events[2],
        JsonEvent::PropertyKeySealed { key, .. } if key == b"b"
    ));
    assert!(matches!(events[3], JsonEvent::ScalarSealed(_)));
    assert!(matches!(
        &events[4],
        JsonEvent::PropertyValueSealed { key, .. } if key == b"b"
    ));
    assert!(matches!(events[5], JsonEvent::ObjectEnd { .. }));
    assert!(matches!(events[6], JsonEvent::RootComplete(_)));

    let events = feed_single_token(b"", br#"["a","b"]"#);
    assert_eq!(events.len(), 7);
    assert!(matches!(events[0], JsonEvent::ArrayStart(_)));
    assert!(matches!(events[1], JsonEvent::ScalarSealed(_)));
    assert!(matches!(events[2], JsonEvent::ItemSealed { index: 0, .. }));
    assert!(matches!(events[3], JsonEvent::ScalarSealed(_)));
    assert!(matches!(events[4], JsonEvent::ItemSealed { index: 1, .. }));
    assert!(matches!(events[5], JsonEvent::ArrayEnd { .. }));
    assert!(matches!(events[6], JsonEvent::RootComplete(_)));
}

fn feed_single_token(prefix: &[u8], token: &[u8]) -> Vec<JsonEvent> {
    let mut vocabulary = Vocabulary::new(1);
    vocabulary.try_insert(token.to_vec(), 0).unwrap();
    let table = TokenTable::build(&vocabulary, 2, &CompileLimits::default()).unwrap();
    let mut cursor = JsonCursor::new(RuntimeLimits::default());
    cursor.feed_bytes(prefix).unwrap();
    cursor.feed_token(0, &table).unwrap()
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
