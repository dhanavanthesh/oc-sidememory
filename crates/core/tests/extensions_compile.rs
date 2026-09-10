mod common;

use oc_sidememory::json_schema::diagnostic::CompileError;
use oc_sidememory::json_schema::extensions::ExtensionPlanV1;
use oc_sidememory::{compile_schema, CompileOptions};

const SCHEMA: &str = r#"{
    "type":"object",
    "properties":{"id":{"type":"string"},"confirmation":{"type":"string"}},
    "required":["id","confirmation"],
    "additionalProperties":false
}"#;

fn compile(
    extension: &str,
) -> Result<oc_sidememory::json_schema::compile::CompiledSchema, CompileError> {
    let (vocabulary, width) = common::ascii_vocabulary();
    let options = CompileOptions {
        extension_plan: Some(ExtensionPlanV1::from_json(extension)?),
        ..CompileOptions::default()
    };
    compile_schema(SCHEMA.as_bytes(), &vocabulary, width, &options)
}

fn plan(captures: &str, relations: &str, order: &str) -> String {
    format!(
        r#"{{"version":1,"objects":[{{"schemaPath":"$","propertyOrder":{order},"captures":{captures},"relations":{relations}}}]}}"#
    )
}

#[test]
fn valid_plan_compiles_to_one_object_dispatch() {
    let extension = plan(
        r#"[{"name":"primary","sourceProperty":"id"}]"#,
        r#"[{"targetProperty":"confirmation","operator":"equal","capture":"primary"}]"#,
        r#"["id","confirmation"]"#,
    );
    assert_eq!(
        compile(&extension)
            .unwrap()
            .memory_plan()
            .object_extension_count(),
        1
    );
}

#[test]
fn invalid_version_order_names_and_properties_are_rejected() {
    assert!(ExtensionPlanV1::from_json(r#"{"version":2,"objects":[]}"#)
        .and_then(|extension| {
            let (vocabulary, width) = common::ascii_vocabulary();
            let options = CompileOptions {
                extension_plan: Some(extension),
                ..CompileOptions::default()
            };
            compile_schema(SCHEMA.as_bytes(), &vocabulary, width, &options)
        })
        .is_err());
    for extension in [
        plan("[]", "[]", "[]"),
        plan("[]", "[]", r#"["id","id"]"#),
        plan(
            r#"[{"name":"x","sourceProperty":"missing"}]"#,
            "[]",
            r#"["id","confirmation"]"#,
        ),
        plan(
            "[]",
            r#"[{"targetProperty":"missing","operator":"memberOf","import":"set"}]"#,
            r#"["id","confirmation"]"#,
        ),
    ] {
        assert!(
            compile(&extension).is_err(),
            "extension unexpectedly compiled: {extension}"
        );
    }
}

#[test]
fn duplicate_unknown_and_forward_captures_are_rejected() {
    for extension in [
        plan(
            r#"[{"name":"x","sourceProperty":"id"},{"name":"x","sourceProperty":"id"}]"#,
            "[]",
            r#"["id","confirmation"]"#,
        ),
        plan(
            "[]",
            r#"[{"targetProperty":"confirmation","operator":"equal","capture":"missing"}]"#,
            r#"["id","confirmation"]"#,
        ),
        plan(
            r#"[{"name":"x","sourceProperty":"confirmation"}]"#,
            r#"[{"targetProperty":"id","operator":"equal","capture":"x"}]"#,
            r#"["id","confirmation"]"#,
        ),
    ] {
        assert!(
            compile(&extension).is_err(),
            "extension unexpectedly compiled: {extension}"
        );
    }
}

#[test]
fn incompatible_capture_relation_types_are_rejected() {
    let schema = r#"{
        "type":"object",
        "properties":{"id":{"type":"string"},"confirmation":{"type":"integer"}},
        "required":["id","confirmation"],
        "additionalProperties":false
    }"#;
    let extension = ExtensionPlanV1::from_json(&plan(
        r#"[{"name":"x","sourceProperty":"id"}]"#,
        r#"[{"targetProperty":"confirmation","operator":"equal","capture":"x"}]"#,
        r#"["id","confirmation"]"#,
    ))
    .unwrap();
    let (vocabulary, width) = common::ascii_vocabulary();
    let options = CompileOptions {
        extension_plan: Some(extension),
        ..CompileOptions::default()
    };
    assert!(compile_schema(schema.as_bytes(), &vocabulary, width, &options).is_err());
}
