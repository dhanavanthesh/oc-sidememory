mod common;

use oc_sidememory::json_schema::compile::{compile_schema, CompileOptions};
use oc_sidememory::json_schema::diagnostic::CompileError;
use oc_sidememory::sidememory::ResourceError;

#[test]
fn unique_items_builds_a_semantic_plan_without_changing_the_array_shape() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let schema = br#"{"type":"array","items":{"type":"string"},"uniqueItems":true}"#;
    let compiled = compile_schema(schema, &vocabulary, width, &CompileOptions::default()).unwrap();
    assert_eq!(compiled.memory_plan().unique_items().len(), 1);
    assert!(compiled.regular_plan().contains("\\["));
}

#[test]
fn annotations_are_accepted_but_unknown_extensions_are_not() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let options = CompileOptions::default();
    compile_schema(
        br#"{"type":"string","description":"value"}"#,
        &vocabulary,
        width,
        &options,
    )
    .unwrap();
    let error = compile_schema(
        br#"{"type":"string","x-surprise":true}"#,
        &vocabulary,
        width,
        &options,
    )
    .unwrap_err();
    assert!(error.to_string().contains("x-surprise"));
}

#[test]
fn array_bounds_are_checked_before_structural_compilation() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let mut options = CompileOptions::default();
    options.runtime_limits.max_array_items = 8;
    let error = compile_schema(
        br#"{"type":"array","items":{"type":"null"},"maxItems":9}"#,
        &vocabulary,
        width,
        &options,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        CompileError::ResourceLimit(ResourceError::LimitExceeded {
            limit_name: "max_array_items",
            limit_value: 8,
            requested: 9,
        })
    ));
}
