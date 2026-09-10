mod common;

use oc_sidememory::sidememory::ProbeDecision;

fn main() {
    let schema = r#"{"type":"object","properties":{"id":{"type":"number"},"values":{"type":"array","items":{"type":"string","enum":["a","b"]},"minItems":2,"maxItems":2,"uniqueItems":true,"contains":{"const":"b"}},"confirmation":{"type":"number"}},"required":["id","values","confirmation"],"additionalProperties":false}"#;
    let extensions = r#"{"version":1,"objects":[{"schemaPath":"$","propertyOrder":["id","values","confirmation"],"captures":[{"name":"primary","sourceProperty":"id"}],"relations":[{"targetProperty":"confirmation","operator":"equal","capture":"primary"}]}]}"#;
    let compiled = common::compile(
        schema,
        &[
            br#"{"id":1,"values":["a","b"],"confirmation":1.0}"#,
            br#"{"id":1,"values":["a","a"],"confirmation":1}"#,
        ],
        Some(extensions),
    );
    let mut guide = common::guide(compiled);
    guide.advance(0).expect("valid document advances");
    assert_eq!(guide.rollback_available(), 1);
    guide.rollback(1).expect("rollback succeeds");
    assert_eq!(guide.rollback_available(), 0);
    assert!(matches!(guide.probe(1).expect("probe succeeds"), ProbeDecision::Deny(_)));
    assert_eq!(guide.probe(0).expect("probe succeeds"), ProbeDecision::Allow);
    println!("composition: rollback restored state and duplicate input remained rejected");
}
