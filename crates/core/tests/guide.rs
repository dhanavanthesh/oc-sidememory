use std::sync::Arc;

use oc_sidememory::sidememory::{
    Guide, GuideError, GuideOptions, ProbeDecision, SemanticViolation, SequenceDecision,
};
use oc_sidememory::{compile_schema, CompileOptions, Vocabulary};

const OPEN: u32 = 0;
const A: u32 = 1;
const COMMA: u32 = 2;
const B: u32 = 3;
const CLOSE: u32 = 4;
const A_ALIAS: u32 = 17;
const EOS: u32 = 20;
const MODEL_WIDTH: usize = 21;

fn guide() -> Guide {
    let mut vocabulary = Vocabulary::new(EOS);
    vocabulary.try_insert(b"[".to_vec(), OPEN).unwrap();
    vocabulary.try_insert(br#""a""#.to_vec(), A).unwrap();
    vocabulary.try_insert(b",".to_vec(), COMMA).unwrap();
    vocabulary.try_insert(br#""b""#.to_vec(), B).unwrap();
    vocabulary.try_insert(b"]".to_vec(), CLOSE).unwrap();
    vocabulary.try_insert(br#""a""#.to_vec(), A_ALIAS).unwrap();
    let compiled = compile_schema(
        br#"{
            "type":"array",
            "items":{"type":"string","enum":["a","b"]},
            "minItems":0,
            "maxItems":2,
            "uniqueItems":true
        }"#,
        &vocabulary,
        MODEL_WIDTH,
        &CompileOptions::default(),
    )
    .unwrap();
    Guide::new(Arc::new(compiled), GuideOptions::default()).unwrap()
}

fn bit_is_set(mask: &[u32], token: u32) -> bool {
    let token = usize::try_from(token).unwrap();
    mask[token / 32] & (1_u32 << (token % 32)) != 0
}

#[test]
fn direct_advance_cannot_bypass_unique_items() {
    let mut guide = guide();
    guide.advance(OPEN).unwrap();
    guide.advance(A).unwrap();
    guide.advance(COMMA).unwrap();
    let state = guide.state_id();
    let rollback = guide.rollback_available();

    let decision = guide.probe(A).unwrap();
    assert!(matches!(
        decision,
        ProbeDecision::Deny(SemanticViolation::DuplicateArrayItem { item_index: 1, .. })
    ));
    assert_eq!(guide.state_id(), state);
    assert_eq!(guide.rollback_available(), rollback);

    let error = guide.advance(A_ALIAS).unwrap_err();
    assert!(matches!(
        error,
        GuideError::SemanticRejection(SemanticViolation::DuplicateArrayItem { item_index: 1, .. })
    ));
    assert_eq!(guide.state_id(), state);
    assert_eq!(guide.rollback_available(), rollback);
    assert_eq!(guide.probe(B).unwrap(), ProbeDecision::Allow);
}

#[test]
fn masks_exclude_all_duplicate_aliases_and_match_allowed_tokens() {
    let mut guide = guide();
    guide.advance(OPEN).unwrap();
    guide.advance(A).unwrap();
    guide.advance(COMMA).unwrap();

    let allowed = guide.allowed_tokens().unwrap();
    let mut mask = vec![u32::MAX; MODEL_WIDTH.div_ceil(32)];
    let summary = guide.write_mask(&mut mask).unwrap();

    assert_eq!(summary.allowed, allowed.len());
    for token in 0..u32::try_from(MODEL_WIDTH).unwrap() {
        assert_eq!(bit_is_set(&mask, token), allowed.contains(&token));
    }
    assert!(!allowed.contains(&A));
    assert!(!allowed.contains(&A_ALIAS));
    assert!(allowed.contains(&B));
}

#[test]
fn rolling_back_tokens_restores_uniqueness_history() {
    let mut guide = guide();
    guide.advance(OPEN).unwrap();
    guide.advance(A).unwrap();
    let state_after_a = guide.state_id();
    guide.advance(COMMA).unwrap();
    guide.advance(B).unwrap();

    guide.rollback(2).unwrap();
    assert_eq!(guide.state_id(), state_after_a);
    assert_eq!(guide.rollback_available(), 2);

    guide.advance(COMMA).unwrap();
    assert!(matches!(
        guide.probe(A).unwrap(),
        ProbeDecision::Deny(SemanticViolation::DuplicateArrayItem { .. })
    ));
    assert_eq!(guide.probe(B).unwrap(), ProbeDecision::Allow);
}

#[test]
fn sequence_probe_accumulates_semantics_but_restores_observable_state() {
    let mut guide = guide();
    let state = guide.state_id();
    assert_eq!(
        guide
            .probe_sequence(&[OPEN, A, COMMA, A_ALIAS, CLOSE], true)
            .unwrap(),
        SequenceDecision::Deny {
            token_index: 3,
            violation: match guide.probe_sequence(&[OPEN, A, COMMA, A], false).unwrap() {
                SequenceDecision::Deny { violation, .. } => violation,
                SequenceDecision::Allow => panic!("duplicate sequence was allowed"),
            },
        }
    );
    assert_eq!(guide.state_id(), state);
    assert_eq!(guide.rollback_available(), 0);
    assert_eq!(
        guide
            .probe_sequence(&[OPEN, A, COMMA, B, CLOSE], true)
            .unwrap(),
        SequenceDecision::Allow
    );
    assert_eq!(guide.state_id(), state);
    assert_eq!(guide.rollback_available(), 0);
}
