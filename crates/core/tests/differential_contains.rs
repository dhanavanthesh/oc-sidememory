mod common;

use std::sync::Arc;

use oc_sidememory::sidememory::{Guide, GuideOptions, SequenceDecision};
use oc_sidememory::{compile_schema, CompileOptions};

#[test]
fn finite_domain_contains_oracle_has_no_false_decisions() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let schema = br#"{
        "type":"array",
        "items":{"type":"string","enum":["a","b","c"]},
        "maxItems":3,
        "contains":{"const":"a"},
        "minContains":1,
        "maxContains":2
    }"#;
    let compiled =
        Arc::new(compile_schema(schema, &vocabulary, width, &CompileOptions::default()).unwrap());
    let mut guide = Guide::new(compiled, GuideOptions::default()).unwrap();
    let alphabet = ["a", "b", "c"];
    let mut total = 0;
    let mut accepted = 0;
    let mut false_accepts = 0;
    let mut false_rejects = 0;
    for len in 0..=3 {
        let combinations = 3_usize.pow(len);
        for mut code in 0..combinations {
            let mut values = Vec::with_capacity(len as usize);
            for _ in 0..len {
                values.push(alphabet[code % alphabet.len()]);
                code /= alphabet.len();
            }
            let document = format!(
                "[{}]",
                values
                    .iter()
                    .map(|value| format!("\"{value}\""))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let matches = values.iter().filter(|value| **value == "a").count();
            let oracle = (1..=2).contains(&matches);
            let tokens: Vec<_> = document.bytes().map(u32::from).collect();
            let actual = guide.probe_sequence(&tokens, true).unwrap() == SequenceDecision::Allow;
            total += 1;
            accepted += usize::from(actual);
            false_accepts += usize::from(actual && !oracle);
            false_rejects += usize::from(!actual && oracle);
        }
    }
    assert_eq!(total, 40);
    assert_eq!(accepted, 24);
    assert_eq!(false_accepts, 0);
    assert_eq!(false_rejects, 0);
}
