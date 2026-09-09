use std::sync::Arc;

use oc_sidememory::sidememory::{Guide, GuideError, GuideOptions, ProbeDecision};
use oc_sidememory::{compile_schema, CompileOptions, Vocabulary};

const OPEN_A_COMMA: u32 = 0;
const A: u32 = 1;
const B: u32 = 2;
const B_THEN_A_CLOSE: u32 = 3;
const A_ALIAS: u32 = 4;
const OPEN: u32 = 5;
const COMMA: u32 = 6;
const CLOSE: u32 = 7;
const EOS: u32 = 8;

fn guide() -> Guide {
    let mut vocabulary = Vocabulary::new(EOS);
    for (bytes, token) in [
        (br#"["a","#.as_slice(), OPEN_A_COMMA),
        (br#""a""#.as_slice(), A),
        (br#""b""#.as_slice(), B),
        (br#""b","a"]"#.as_slice(), B_THEN_A_CLOSE),
        (br#""a""#.as_slice(), A_ALIAS),
        (b"[".as_slice(), OPEN),
        (b",".as_slice(), COMMA),
        (b"]".as_slice(), CLOSE),
    ] {
        vocabulary.try_insert(bytes.to_vec(), token).unwrap();
    }
    let compiled = compile_schema(
        br#"{
            "type":"array",
            "items":{"type":"string"},
            "minItems":3,
            "maxItems":3,
            "uniqueItems":true
        }"#,
        &vocabulary,
        usize::try_from(EOS).unwrap() + 1,
        &CompileOptions::default(),
    )
    .unwrap();
    Guide::new(Arc::new(compiled), GuideOptions::default()).unwrap()
}

#[test]
fn token_aliases_have_identical_semantic_decisions() {
    let mut guide = guide();
    guide.advance(OPEN_A_COMMA).unwrap();

    assert!(matches!(guide.probe(A).unwrap(), ProbeDecision::Deny(_)));
    assert!(matches!(
        guide.probe(A_ALIAS).unwrap(),
        ProbeDecision::Deny(_)
    ));
    assert_eq!(guide.probe(B).unwrap(), ProbeDecision::Allow);
}

#[test]
fn late_duplicate_rolls_back_every_earlier_event_in_the_token() {
    let mut guide = guide();
    guide.advance(OPEN_A_COMMA).unwrap();
    let before = guide.debug_snapshot();

    assert!(matches!(
        guide.advance(B_THEN_A_CLOSE),
        Err(GuideError::SemanticRejection(_))
    ));
    assert_eq!(guide.debug_snapshot(), before);

    assert_eq!(guide.probe(B).unwrap(), ProbeDecision::Allow);
    assert!(matches!(guide.probe(A).unwrap(), ProbeDecision::Deny(_)));
}
