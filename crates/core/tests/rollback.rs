use std::sync::Arc;

use oc_sidememory::sidememory::{Guide, GuideError, GuideOptions, ProbeDecision};
use oc_sidememory::{compile_schema, CompileOptions, Vocabulary};

const OPEN: u32 = 0;
const INNER_A: u32 = 1;
const INNER_A_ALIAS: u32 = 2;
const COMMA: u32 = 3;
const CLOSE: u32 = 4;
const INNER_B: u32 = 5;
const EOS: u32 = 8;

fn guide(max_rollback_tokens: usize) -> Guide {
    let mut vocabulary = Vocabulary::new(EOS);
    vocabulary.try_insert(b"[".to_vec(), OPEN).unwrap();
    vocabulary
        .try_insert(br#"["a"]"#.to_vec(), INNER_A)
        .unwrap();
    vocabulary
        .try_insert(br#"["a"]"#.to_vec(), INNER_A_ALIAS)
        .unwrap();
    vocabulary.try_insert(b",".to_vec(), COMMA).unwrap();
    vocabulary.try_insert(b"]".to_vec(), CLOSE).unwrap();
    vocabulary
        .try_insert(br#"["b"]"#.to_vec(), INNER_B)
        .unwrap();
    let compiled = compile_schema(
        br#"{
            "type":"array",
            "items":{
                "type":"array",
                "items":{"type":"string"},
                "uniqueItems":true
            },
            "uniqueItems":true
        }"#,
        &vocabulary,
        usize::try_from(EOS).unwrap() + 1,
        &CompileOptions::default(),
    )
    .unwrap();
    Guide::new(
        Arc::new(compiled),
        GuideOptions {
            max_rollback_tokens,
            ..GuideOptions::default()
        },
    )
    .unwrap()
}

#[test]
fn bounded_window_evicts_only_the_oldest_successful_token() {
    let mut guide = guide(2);
    guide.advance(OPEN).unwrap();
    let state_after_open = guide.state_id();
    guide.advance(INNER_A).unwrap();
    assert_eq!(guide.rollback_available(), 2);
    assert!(guide.retained_rollback_bytes() > 0);

    guide.rollback(1).unwrap();
    assert_eq!(guide.state_id(), state_after_open);
    assert_eq!(guide.probe(INNER_A_ALIAS).unwrap(), ProbeDecision::Allow);
    guide.advance(INNER_A_ALIAS).unwrap();
    assert_eq!(guide.rollback_available(), 2);

    guide.rollback(2).unwrap();
    assert_eq!(guide.retained_rollback_bytes(), 0);
    assert!(matches!(
        guide.rollback(1),
        Err(GuideError::RollbackUnavailable {
            requested: 1,
            available: 0
        })
    ));
}

#[test]
fn rollback_across_nested_close_reopens_exact_history_state() {
    let mut guide = guide(8);
    guide.advance(OPEN).unwrap();
    let state_after_open = guide.state_id();
    guide.advance(INNER_A).unwrap();

    guide.rollback(1).unwrap();
    assert_eq!(guide.state_id(), state_after_open);
    assert_eq!(guide.probe(INNER_A_ALIAS).unwrap(), ProbeDecision::Allow);
}
