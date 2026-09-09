mod common;

use oc_sidememory::json_schema::compile::{compile_schema, CompileOptions};
use oc_sidememory::sidememory::{
    JsonCursor, JsonEvent, RouterJournal, RuntimeLimits, SemanticRouter,
};

#[test]
fn nested_arrays_route_to_their_compiled_nodes() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let compiled = compile_schema(
        br#"{
            "type":"array",
            "items":{
                "type":"array",
                "items":{"type":"string"},
                "uniqueItems":true
            },
            "uniqueItems":true
        }"#,
        &vocabulary,
        width,
        &CompileOptions::default(),
    )
    .unwrap();
    let root = compiled.ir().root();
    let inner = compiled
        .ir()
        .node(root)
        .unwrap()
        .array
        .as_ref()
        .unwrap()
        .items;
    let mut cursor = JsonCursor::new(RuntimeLimits::default());
    let events = cursor.feed_bytes(b"[[]]").unwrap();
    let mut router = SemanticRouter::new(root);
    let mut opened = Vec::new();

    for event in events {
        match event {
            JsonEvent::ArrayStart(frame) => {
                opened.push(router.open_array(frame, compiled.ir()).unwrap());
            }
            JsonEvent::ItemSealed { array, .. } => {
                router.complete_array_item(array).unwrap();
            }
            JsonEvent::ArrayEnd { frame, .. } => {
                router.close(frame).unwrap();
            }
            JsonEvent::RootComplete(_) => router.complete_root().unwrap(),
            _ => {}
        }
    }

    assert_eq!(opened, vec![root, inner]);
    assert!(router.frames().is_empty());
    assert!(router.is_root_complete());
}

#[test]
fn decoded_object_keys_select_property_nodes() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let compiled = compile_schema(
        br#"{
            "type":"object",
            "properties":{"items":{"type":"array","items":{"type":"integer"}}},
            "required":["items"],
            "additionalProperties":false
        }"#,
        &vocabulary,
        width,
        &CompileOptions::default(),
    )
    .unwrap();
    let root = compiled.ir().root();
    let property = compiled
        .ir()
        .node(root)
        .unwrap()
        .object
        .as_ref()
        .unwrap()
        .properties[0]
        .1;
    let mut cursor = JsonCursor::new(RuntimeLimits::default());
    let events = cursor.feed_bytes(br#"{"items":[]}"#).unwrap();
    let mut router = SemanticRouter::new(root);
    let mut selected = None;

    for event in events {
        match event {
            JsonEvent::ObjectStart(frame) => {
                router.open_object(frame, compiled.ir()).unwrap();
            }
            JsonEvent::PropertyKeySealed { object, key } => {
                selected = Some(
                    router
                        .set_pending_property(object, &key, compiled.ir())
                        .unwrap(),
                );
            }
            JsonEvent::ArrayStart(frame) => {
                router.open_array(frame, compiled.ir()).unwrap();
            }
            JsonEvent::ArrayEnd { frame, .. } | JsonEvent::ObjectEnd { frame, .. } => {
                router.close(frame).unwrap();
            }
            JsonEvent::PropertyValueSealed { object, .. } => {
                router.complete_property(object).unwrap();
            }
            JsonEvent::RootComplete(_) => router.complete_root().unwrap(),
            _ => {}
        }
    }

    assert_eq!(selected, Some(property));
    assert!(router.is_root_complete());
}

#[test]
fn router_journal_restores_nested_frame_state() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let compiled = compile_schema(
        br#"{"type":"array","items":{"type":"array","items":{"type":"null"}}}"#,
        &vocabulary,
        width,
        &CompileOptions::default(),
    )
    .unwrap();
    let root = compiled.ir().root();
    let mut cursor = JsonCursor::new(RuntimeLimits::default());
    let events = cursor.feed_bytes(b"[[]]").unwrap();
    let mut router = SemanticRouter::new(root);
    let mut journal = RouterJournal::new(32);
    let mark = journal.mark();

    for event in events {
        match event {
            JsonEvent::ArrayStart(frame) => {
                router
                    .open_array_journaled(frame, compiled.ir(), &mut journal)
                    .unwrap();
            }
            JsonEvent::ArrayEnd { frame, .. } => {
                router.close_journaled(frame, &mut journal).unwrap();
            }
            JsonEvent::ItemSealed { array, .. } => router
                .complete_array_item_journaled(array, &mut journal)
                .unwrap(),
            JsonEvent::RootComplete(_) => {
                router.complete_root_journaled(&mut journal).unwrap();
            }
            _ => {}
        }
    }
    assert!(router.is_root_complete());
    journal.restore(mark, &mut router).unwrap();
    assert!(router.frames().is_empty());
    assert!(!router.is_root_complete());
    assert_eq!(router.expected_node(), root);
}
