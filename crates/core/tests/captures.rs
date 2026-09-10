mod common;

use std::sync::Arc;

use oc_sidememory::json_schema::extensions::ExtensionPlanV1;
use oc_sidememory::sidememory::{Guide, GuideOptions, SequenceDecision};
use oc_sidememory::{compile_schema, CompileOptions};

fn guide(operator: &str, source_required: bool) -> Guide {
    let (vocabulary, width) = common::ascii_vocabulary();
    let required = if source_required {
        r#"["id","confirmation"]"#
    } else {
        r#"["confirmation"]"#
    };
    let schema = format!(
        r#"{{"type":"object","properties":{{"id":{{"type":"number"}},"confirmation":{{"type":"number"}}}},"required":{required},"additionalProperties":false}}"#
    );
    let extension = format!(
        r#"{{"version":1,"objects":[{{"schemaPath":"$","propertyOrder":["id","confirmation"],"captures":[{{"name":"primary","sourceProperty":"id"}}],"relations":[{{"targetProperty":"confirmation","operator":"{operator}","capture":"primary"}}]}}]}}"#
    );
    let options = CompileOptions {
        extension_plan: Some(ExtensionPlanV1::from_json(&extension).unwrap()),
        ..CompileOptions::default()
    };
    let compiled = compile_schema(schema.as_bytes(), &vocabulary, width, &options).unwrap();
    Guide::new(Arc::new(compiled), GuideOptions::default()).unwrap()
}

fn accepts(guide: &mut Guide, document: &str) -> bool {
    let tokens: Vec<_> = document.bytes().map(u32::from).collect();
    guide.probe_sequence(&tokens, true).unwrap() == SequenceDecision::Allow
}

#[test]
fn equality_and_inequality_use_exact_json_number_equality() {
    let mut equal = guide("equal", true);
    assert!(accepts(&mut equal, r#"{"id":1,"confirmation":1.0}"#));
    assert!(!accepts(&mut equal, r#"{"id":1,"confirmation":2}"#));

    let mut not_equal = guide("notEqual", true);
    assert!(accepts(&mut not_equal, r#"{"id":1,"confirmation":2}"#));
    assert!(!accepts(&mut not_equal, r#"{"id":1,"confirmation":1e0}"#));
}

#[test]
fn absent_optional_source_reports_a_semantic_denial() {
    let mut guide = guide("equal", false);
    assert!(!accepts(&mut guide, r#"{"confirmation":1}"#));
}

#[test]
fn probe_and_object_close_leave_register_state_unchanged() {
    let mut guide = guide("equal", true);
    let before = guide.debug_snapshot();
    assert!(accepts(&mut guide, r#"{"id":7,"confirmation":7}"#));
    assert_eq!(guide.debug_snapshot(), before);
}

#[test]
fn sibling_object_instances_keep_independent_register_scopes() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let pair = r#"{"type":"object","properties":{"id":{"type":"string"},"confirmation":{"type":"string"}},"required":["id","confirmation"],"additionalProperties":false}"#;
    let schema = format!(
        r#"{{"type":"object","properties":{{"left":{pair},"right":{pair}}},"required":["left","right"],"additionalProperties":false}}"#
    );
    let extension = ExtensionPlanV1::from_json(
        r#"{"version":1,"objects":[{"schemaPath":"$.left","propertyOrder":["id","confirmation"],"captures":[{"name":"left_id","sourceProperty":"id"}],"relations":[{"targetProperty":"confirmation","operator":"equal","capture":"left_id"}]},{"schemaPath":"$.right","propertyOrder":["id","confirmation"],"captures":[{"name":"right_id","sourceProperty":"id"}],"relations":[{"targetProperty":"confirmation","operator":"equal","capture":"right_id"}]}]}"#,
    )
    .unwrap();
    let options = CompileOptions {
        extension_plan: Some(extension),
        ..CompileOptions::default()
    };
    let compiled = compile_schema(schema.as_bytes(), &vocabulary, width, &options).unwrap();
    let mut guide = Guide::new(Arc::new(compiled), GuideOptions::default()).unwrap();
    assert!(accepts(
        &mut guide,
        r#"{"left":{"id":"a","confirmation":"a"},"right":{"id":"b","confirmation":"b"}}"#
    ));
    assert!(!accepts(
        &mut guide,
        r#"{"left":{"id":"a","confirmation":"a"},"right":{"id":"b","confirmation":"a"}}"#
    ));
}
