use std::sync::Arc;

use oc_sidememory::sidememory::{Guide, GuideOptions, SequenceDecision};
use oc_sidememory::{compile_schema, CompileOptions};

fn accepts(predicate: &str, item: &str) -> bool {
    let items = if item.starts_with('{') {
        r#"{"type":"object","properties":{"id":{"type":"number"},"name":{"type":"string"}},"required":[],"additionalProperties":false}"#
    } else if item.starts_with('[') {
        r#"{"type":"array","items":{"type":"number"},"minItems":0,"maxItems":3}"#
    } else if item.starts_with('"') {
        r#"{"type":"string"}"#
    } else if matches!(item, "true" | "false") {
        r#"{"type":"boolean"}"#
    } else {
        r#"{"type":"number"}"#
    };
    let schema = format!(
        r#"{{"type":"array","items":{items},"contains":{predicate},"minContains":1,"maxContains":1}}"#
    );
    let document = format!("[{item}]");
    let (vocabulary, width) = common::ascii_vocabulary();
    let compiled = compile_schema(
        schema.as_bytes(),
        &vocabulary,
        width,
        &CompileOptions::default(),
    )
    .unwrap();
    let mut guide = Guide::new(Arc::new(compiled), GuideOptions::default()).unwrap();
    let tokens: Vec<_> = document.bytes().map(u32::from).collect();
    guide.probe_sequence(&tokens, true).unwrap() == SequenceDecision::Allow
}

#[test]
fn scalar_types_const_and_enum_are_exact() {
    assert!(accepts(r#"{"type":"number","const":1.0}"#, "1e0"));
    assert!(accepts(r#"{"type":"integer"}"#, "1.0"));
    assert!(!accepts(r#"{"type":"integer"}"#, "1.5"));
    assert!(accepts(r#"{"type":"string","enum":["a","b"]}"#, r#""b""#));
    assert!(!accepts(r#"{"type":"boolean"}"#, "1"));
}

#[test]
fn closed_object_predicates_check_required_values() {
    let predicate = r#"{
        "type":"object",
        "properties":{"id":{"type":"integer"},"name":{"const":"ok"}},
        "required":["id","name"],
        "additionalProperties":false
    }"#;
    assert!(accepts(predicate, r#"{"id":1.0,"name":"ok"}"#));
    assert!(!accepts(predicate, r#"{"id":1}"#));
    assert!(!accepts(predicate, r#"{"id":1,"name":"bad"}"#));
}

#[test]
fn homogeneous_array_bounds_and_nested_unique_items_are_checked() {
    let predicate = r#"{
        "type":"array",
        "items":{"type":"number"},
        "minItems":2,
        "maxItems":2,
        "uniqueItems":true
    }"#;
    assert!(accepts(predicate, "[1,2]"));
    assert!(!accepts(predicate, "[1,1.0]"));
    assert!(!accepts(predicate, "[1]"));
}
mod common;
