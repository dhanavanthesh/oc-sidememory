mod common;

use std::sync::Arc;

use oc_sidememory::sidememory::{Guide, GuideError, GuideOptions, ProbeDecision, SequenceDecision};
use oc_sidememory::{compile_schema, CompileOptions};

fn decisions(schema: &str, documents: &[&str]) -> Vec<SequenceDecision> {
    let eos = u32::try_from(documents.len()).unwrap();
    let mut vocabulary = oc_sidememory::Vocabulary::new(eos);
    for (token, document) in documents.iter().enumerate() {
        vocabulary
            .try_insert(document.as_bytes().to_vec(), u32::try_from(token).unwrap())
            .unwrap();
    }
    let compiled = compile_schema(
        schema.as_bytes(),
        &vocabulary,
        documents.len() + 1,
        &CompileOptions::default(),
    )
    .unwrap();
    let compiled = Arc::new(compiled);
    documents
        .iter()
        .enumerate()
        .map(|(token, _)| {
            let mut guide = Guide::new(compiled.clone(), GuideOptions::default()).unwrap();
            guide
                .probe_sequence(&[u32::try_from(token).unwrap()], true)
                .unwrap()
        })
        .collect()
}

#[test]
fn nonmatching_items_only_affect_the_count() {
    let schema = r#"{
        "type":"array",
        "items":{"enum":[1,"x",false]},
        "contains":{"const":"x"}
    }"#;
    let actual = decisions(schema, &[r#"[1,"x",false]"#, "[1,false]"]);
    assert_eq!(actual[0], SequenceDecision::Allow);
    assert_ne!(actual[1], SequenceDecision::Allow);
}

#[test]
fn minimum_zero_and_boolean_false_accept_empty_and_nonmatching_arrays() {
    let schema = r#"{
        "type":"array",
        "items":{"enum":[1,true,null]},
        "contains":false,
        "minContains":0,
        "maxContains":0
    }"#;
    assert_eq!(
        decisions(schema, &["[]", "[1,true,null]"]),
        [SequenceDecision::Allow, SequenceDecision::Allow]
    );
}

#[test]
fn maximum_is_rejected_when_the_sealing_item_exceeds_it() {
    let schema = r#"{
        "type":"array",
        "items":{"type":"integer"},
        "contains":{"type":"integer"},
        "minContains":0,
        "maxContains":1
    }"#;
    let actual = decisions(schema, &["[1]", "[1,2]"]);
    assert_eq!(actual[0], SequenceDecision::Allow);
    assert_ne!(actual[1], SequenceDecision::Allow);
}

#[test]
fn finite_capacity_pruning_rejects_an_unreachable_minimum() {
    let schema = r#"{
        "type":"array",
        "items":{"enum":[1,"a","b"]},
        "maxItems":2,
        "contains":{"type":"string"},
        "minContains":2
    }"#;
    let actual = decisions(schema, &["[1]", r#"["a","b"]"#]);
    assert_ne!(actual[0], SequenceDecision::Allow);
    assert_eq!(actual[1], SequenceDecision::Allow);
}

#[test]
fn close_is_removed_until_the_contains_minimum_is_met() {
    let schema = br#"{
        "type":"array",
        "items":{"type":"string","enum":["a","x"]},
        "contains":{"const":"x"}
    }"#;
    let (vocabulary, width) = common::ascii_vocabulary();
    let compiled = compile_schema(schema, &vocabulary, width, &CompileOptions::default()).unwrap();
    let mut guide = Guide::new(Arc::new(compiled), GuideOptions::default()).unwrap();
    for token in b"[\"a\"" {
        guide.advance(u32::from(*token)).unwrap();
    }
    let before = guide.debug_snapshot();

    assert!(matches!(
        guide.probe(u32::from(b']')).unwrap(),
        ProbeDecision::Deny(_)
    ));
    assert!(!guide.allowed_tokens().unwrap().contains(&u32::from(b']')));
    assert!(matches!(
        guide.advance(u32::from(b']')),
        Err(GuideError::SemanticRejection(_))
    ));
    assert_eq!(guide.debug_snapshot(), before);
}
