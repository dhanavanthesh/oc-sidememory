mod common;

use std::sync::Arc;

use oc_sidememory::sidememory::{Guide, GuideOptions, ImportedMemory, ProbeDecision};

fn main() {
    let schema = r#"{"type":"object","properties":{"customer":{"type":"string"}},"required":["customer"],"additionalProperties":false}"#;
    let extensions = r#"{"version":1,"objects":[{"schemaPath":"$","propertyOrder":["customer"],"relations":[{"targetProperty":"customer","operator":"memberOf","import":"valid_customers"}]}]}"#;
    let compiled = common::compile(
        schema,
        &[br#"{"customer":"cust_7"}"#, br#"{"customer":"cust_8"}"#],
        Some(extensions),
    );
    let imports = Arc::new(
        ImportedMemory::from_json(
            "customer-catalog",
            "1",
            r#"{"valid_customers":["cust_7","cust_9"]}"#,
        )
        .expect("example imports are valid"),
    );
    let mut guide = Guide::new_with_imports(compiled, GuideOptions::default(), imports)
        .expect("example guide builds");
    assert_eq!(guide.probe(0).expect("probe succeeds"), ProbeDecision::Allow);
    assert!(matches!(guide.probe(1).expect("probe succeeds"), ProbeDecision::Deny(_)));
    println!("membership: accepted an imported member and rejected a missing value");
}
