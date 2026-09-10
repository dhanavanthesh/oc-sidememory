mod common;

use oc_sidememory::sidememory::ProbeDecision;

fn main() {
    let schema = r#"{"type":"object","properties":{"id":{"type":"number"},"confirmation":{"type":"number"}},"required":["id","confirmation"],"additionalProperties":false}"#;
    let extensions = r#"{"version":1,"objects":[{"schemaPath":"$","propertyOrder":["id","confirmation"],"captures":[{"name":"primary","sourceProperty":"id"}],"relations":[{"targetProperty":"confirmation","operator":"equal","capture":"primary"}]}]}"#;
    let compiled = common::compile(
        schema,
        &[br#"{"id":1,"confirmation":1.0}"#, br#"{"id":1,"confirmation":2}"#],
        Some(extensions),
    );
    let mut guide = common::guide(compiled);
    assert_eq!(guide.probe(0).expect("probe succeeds"), ProbeDecision::Allow);
    assert!(matches!(guide.probe(1).expect("probe succeeds"), ProbeDecision::Deny(_)));
    println!("capture equality: accepted 1 == 1.0 and rejected 1 != 2");
}
