use std::sync::Arc;

use oc_sidememory::sidememory::{Guide, GuideError, GuideOptions, ProbeDecision};
use oc_sidememory::{compile_schema, CompileOptions, Vocabulary};

const OPEN: u32 = 0;
const INNER_A: u32 = 1;
const COMMA: u32 = 2;
const INNER_B: u32 = 3;
const CLOSE: u32 = 4;
const INNER_A_ALIAS: u32 = 5;
const INNER_DUPLICATE: u32 = 6;
const EOS: u32 = 12;

fn nested_guide(outer_unique: bool) -> Guide {
    let mut vocabulary = Vocabulary::new(EOS);
    vocabulary.try_insert(b"[".to_vec(), OPEN).unwrap();
    vocabulary
        .try_insert(br#"["a"]"#.to_vec(), INNER_A)
        .unwrap();
    vocabulary.try_insert(b",".to_vec(), COMMA).unwrap();
    vocabulary
        .try_insert(br#"["b"]"#.to_vec(), INNER_B)
        .unwrap();
    vocabulary.try_insert(b"]".to_vec(), CLOSE).unwrap();
    vocabulary
        .try_insert(br#"["a"]"#.to_vec(), INNER_A_ALIAS)
        .unwrap();
    vocabulary
        .try_insert(br#"["a","a"]"#.to_vec(), INNER_DUPLICATE)
        .unwrap();
    let schema = format!(
        r#"{{
            "type":"array",
            "items":{{
                "type":"array",
                "items":{{"type":"string"}},
                "uniqueItems":true
            }},
            "maxItems":2,
            "uniqueItems":{outer_unique}
        }}"#
    );
    let compiled = compile_schema(
        schema.as_bytes(),
        &vocabulary,
        usize::try_from(EOS).unwrap() + 1,
        &CompileOptions::default(),
    )
    .unwrap();
    Guide::new(Arc::new(compiled), GuideOptions::default()).unwrap()
}

#[test]
fn sibling_arrays_have_independent_inner_histories() {
    let mut guide = nested_guide(false);
    for token in [OPEN, INNER_A, COMMA, INNER_A_ALIAS, CLOSE, EOS] {
        guide.advance(token).unwrap();
    }
    assert!(guide.is_terminated());
}

#[test]
fn outer_history_rejects_equal_completed_inner_arrays() {
    let mut guide = nested_guide(true);
    guide.advance(OPEN).unwrap();
    guide.advance(INNER_A).unwrap();
    guide.advance(COMMA).unwrap();
    let state = guide.state_id();
    let rollback = guide.rollback_available();

    assert!(matches!(
        guide.probe(INNER_A_ALIAS).unwrap(),
        ProbeDecision::Deny(_)
    ));
    assert!(matches!(
        guide.advance(INNER_A_ALIAS),
        Err(GuideError::SemanticRejection(_))
    ));
    assert_eq!(guide.state_id(), state);
    assert_eq!(guide.rollback_available(), rollback);
    guide.advance(INNER_B).unwrap();
    guide.advance(CLOSE).unwrap();
    guide.advance(EOS).unwrap();
}

#[test]
fn late_duplicate_in_one_token_restores_all_earlier_events() {
    let mut guide = nested_guide(true);
    guide.advance(OPEN).unwrap();
    let state = guide.state_id();
    let rollback = guide.rollback_available();

    assert!(matches!(
        guide.probe(INNER_DUPLICATE).unwrap(),
        ProbeDecision::Deny(_)
    ));
    assert_eq!(guide.state_id(), state);
    assert_eq!(guide.rollback_available(), rollback);

    guide.advance(INNER_A).unwrap();
    guide.advance(COMMA).unwrap();
    assert_eq!(guide.probe(INNER_B).unwrap(), ProbeDecision::Allow);
}
