mod common;

use oc_sidememory::json_schema::compile::{compile_ir, compile_schema, CompileOptions};
use oc_sidememory::json_schema::diagnostic::CompileError;

#[test]
fn inherited_priority_dispatch_hazards_are_rejected() {
    let cases = [
        (
            r#"{"allOf":[{"type":"integer"},{"type":"integer"}]}"#,
            "allOf",
        ),
        (
            r#"{"oneOf":[{"type":"integer"},{"type":"number"}]}"#,
            "oneOf",
        ),
        (
            r#"{"anyOf":[{"type":"array","uniqueItems":true},{"type":"array"}]}"#,
            "anyOf",
        ),
        (
            r#"{"type":"array","prefixItems":[{"type":"integer"}]}"#,
            "prefixItems",
        ),
        (
            r##"{"$ref":"#/$defs/x","uniqueItems":true,"$defs":{"x":{"type":"array"}}}"##,
            "$ref",
        ),
    ];
    let vocabulary = oc_sidememory::Vocabulary::new(0);
    for (schema, keyword) in cases {
        let error = compile_schema(
            schema.as_bytes(),
            &vocabulary,
            1,
            &CompileOptions::default(),
        )
        .unwrap_err();
        match error {
            CompileError::UnsupportedKeyword {
                keyword: actual, ..
            } => assert_eq!(&*actual, keyword),
            other => panic!("expected unsupported {keyword}, got {other:?}"),
        }
    }
}

#[test]
fn malformed_and_contradictory_schemas_keep_distinct_categories() {
    let vocabulary = oc_sidememory::Vocabulary::new(0);
    assert!(matches!(
        compile_schema(b"{", &vocabulary, 1, &CompileOptions::default()),
        Err(CompileError::MalformedSchema { .. })
    ));
    assert!(matches!(
        compile_schema(
            br#"{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":1}"#,
            &vocabulary,
            1,
            &CompileOptions::default()
        ),
        Err(CompileError::UnsatisfiableSchema { .. })
    ));
}

#[test]
fn keyword_values_are_checked_before_applicability() {
    for (schema, keyword) in [
        (r#"{"type":"string","uniqueItems":"yes"}"#, "uniqueItems"),
        (r#"{"type":"string","minItems":-1}"#, "minItems"),
        (r#"{"type":"string","properties":[]}"#, "properties"),
        (r#"{"type":"string","required":["a","a"]}"#, "required"),
        (
            r#"{"type":"string","additionalProperties":null}"#,
            "additionalProperties",
        ),
    ] {
        let error = compile_ir(schema.as_bytes(), &CompileOptions::default()).unwrap_err();
        match error {
            CompileError::InvalidKeywordValue {
                keyword: actual, ..
            } => assert_eq!(&*actual, keyword),
            other => panic!("expected invalid {keyword}, got {other:?}"),
        }
    }
}

#[test]
fn applicable_assertions_require_an_explicit_matching_type() {
    for (schema, keyword) in [
        (r#"{"items":true}"#, "items"),
        (r#"{"type":"string","maxItems":1}"#, "maxItems"),
        (r#"{"uniqueItems":true}"#, "uniqueItems"),
        (r#"{"properties":{}}"#, "properties"),
        (r#"{"type":"array","required":[]}"#, "required"),
    ] {
        let error = compile_ir(schema.as_bytes(), &CompileOptions::default()).unwrap_err();
        match error {
            CompileError::UnsupportedKeyword {
                keyword: actual, ..
            } => assert_eq!(&*actual, keyword),
            other => panic!("expected unsupported {keyword}, got {other:?}"),
        }
    }
}

#[test]
fn false_unique_items_remains_a_profile_no_op() {
    let ir = compile_ir(
        br#"{"type":"string","uniqueItems":false}"#,
        &CompileOptions::default(),
    )
    .unwrap();
    assert!(ir.nodes()[0].semantic.is_empty());
}
