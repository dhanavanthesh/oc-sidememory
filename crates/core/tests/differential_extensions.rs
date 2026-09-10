mod common;

use std::sync::Arc;

use oc_sidememory::json_schema::extensions::ExtensionPlanV1;
use oc_sidememory::sidememory::{Guide, GuideOptions, SequenceDecision};
use oc_sidememory::{compile_schema, CompileOptions};

#[test]
fn finite_domain_capture_oracle_has_no_false_decisions() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let schema = br#"{
        "type":"object",
        "properties":{"source":{"type":"string","enum":["a","b","c"]},"target":{"type":"string","enum":["a","b","c"]}},
        "required":["source","target"],
        "additionalProperties":false
    }"#;
    let extension = ExtensionPlanV1::from_json(
        r#"{"version":1,"objects":[{"schemaPath":"$","propertyOrder":["source","target"],"captures":[{"name":"source_value","sourceProperty":"source"}],"relations":[{"targetProperty":"target","operator":"equal","capture":"source_value"}]}]}"#,
    )
    .unwrap();
    let options = CompileOptions {
        extension_plan: Some(extension),
        ..CompileOptions::default()
    };
    let compiled = Arc::new(compile_schema(schema, &vocabulary, width, &options).unwrap());
    let mut guide = Guide::new(compiled, GuideOptions::default()).unwrap();
    let mut false_accepts = 0;
    let mut false_rejects = 0;
    let mut accepted = 0;
    for source in ["a", "b", "c"] {
        for target in ["a", "b", "c"] {
            let document = format!(r#"{{"source":"{source}","target":"{target}"}}"#);
            let tokens: Vec<_> = document.bytes().map(u32::from).collect();
            let actual = guide.probe_sequence(&tokens, true).unwrap() == SequenceDecision::Allow;
            let oracle = source == target;
            accepted += usize::from(actual);
            false_accepts += usize::from(actual && !oracle);
            false_rejects += usize::from(!actual && oracle);
        }
    }
    assert_eq!(accepted, 3);
    assert_eq!(false_accepts, 0);
    assert_eq!(false_rejects, 0);
}
