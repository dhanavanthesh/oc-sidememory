use oc_sidememory::json_schema::diagnostic::CompileError;
use oc_sidememory::{compile_schema, CompileOptions};

fn compile(
    schema: &str,
) -> Result<oc_sidememory::json_schema::compile::CompiledSchema, CompileError> {
    let mut vocabulary = oc_sidememory::Vocabulary::new(1);
    vocabulary.try_insert(b"[]".to_vec(), 0).unwrap();
    compile_schema(
        schema.as_bytes(),
        &vocabulary,
        2,
        &CompileOptions::default(),
    )
}

#[test]
fn contains_defaults_and_explicit_bounds_compile() {
    let compiled =
        compile(r#"{"type":"array","items":{"type":"string"},"contains":{"type":"string"}}"#)
            .unwrap();
    let constraint = &compiled.memory_plan().contains_constraints()[0];
    assert_eq!(constraint.lower, 1);
    assert_eq!(constraint.upper, None);

    let compiled = compile(
        r#"{"type":"array","items":{"type":"string"},"contains":true,"minContains":0,"maxContains":2}"#,
    )
    .unwrap();
    let constraint = &compiled.memory_plan().contains_constraints()[0];
    assert_eq!(constraint.lower, 0);
    assert_eq!(constraint.upper, Some(2));
}

#[test]
fn bounds_without_contains_have_no_memory_effect() {
    let compiled =
        compile(r#"{"type":"array","items":{"type":"string"},"minContains":4,"maxContains":1}"#)
            .unwrap();
    assert!(compiled.memory_plan().contains_constraints().is_empty());
}

#[test]
fn invalid_and_unsupported_predicates_fail_compilation() {
    assert!(matches!(
        compile(r#"{"type":"array","items":{"type":"string"},"contains":7}"#),
        Err(CompileError::InvalidKeywordValue { .. })
    ));
    assert!(matches!(
        compile(
            r#"{"type":"array","items":{"type":"string"},"contains":true,"minContains":2,"maxContains":1}"#
        ),
        Err(CompileError::UnsatisfiableSchema { .. })
    ));
    assert!(matches!(
        compile(
            r#"{"type":"array","items":{"type":"string"},"contains":{"type":"array","contains":true}}"#
        ),
        Err(CompileError::UnsupportedKeyword { .. })
    ));
}

#[test]
fn false_is_valid_inside_contains_without_rejecting_the_outer_schema() {
    let compiled = compile(
        r#"{"type":"array","items":{"type":"string"},"contains":false,"minContains":0,"maxContains":0}"#,
    )
    .unwrap();
    assert_eq!(compiled.memory_plan().contains_constraints().len(), 1);
}
