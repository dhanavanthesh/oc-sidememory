mod common;

use oc_sidememory::sidememory::ProbeDecision;

fn main() {
    let schema = r#"{"type":"array","items":{"type":"integer","enum":[1,2]},"minItems":2,"maxItems":2,"contains":{"const":1},"minContains":1,"maxContains":1}"#;
    let compiled = common::compile(schema, &[b"[1,2]", b"[2,2]"], None);
    let mut guide = common::guide(compiled);
    assert_eq!(guide.probe(0).expect("probe succeeds"), ProbeDecision::Allow);
    assert!(matches!(guide.probe(1).expect("probe succeeds"), ProbeDecision::Deny(_)));
    println!("contains: accepted one match and rejected zero matches");
}
