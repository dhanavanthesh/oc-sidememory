mod common;

use oc_sidememory::sidememory::ProbeDecision;

fn main() {
    let schema = r#"{"type":"array","items":{"type":"string","enum":["a","b"]},"minItems":2,"maxItems":2,"uniqueItems":true}"#;
    let compiled = common::compile(schema, &[br#"["a","b"]"#, br#"["a","a"]"#], None);
    let mut guide = common::guide(compiled);
    assert_eq!(guide.probe(0).expect("probe succeeds"), ProbeDecision::Allow);
    assert!(matches!(guide.probe(1).expect("probe succeeds"), ProbeDecision::Deny(_)));
    println!("uniqueItems: accepted distinct values and rejected a duplicate");
}
