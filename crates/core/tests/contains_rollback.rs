use std::sync::Arc;

use oc_sidememory::sidememory::{Guide, GuideError, GuideOptions, ProbeDecision};
use oc_sidememory::{compile_schema, CompileOptions, Vocabulary};

#[test]
fn probe_and_rejected_multi_event_token_restore_counter_and_history() {
    const OPEN_RED_COMMA: u32 = 0;
    const BLUE_THEN_RED_CLOSE: u32 = 1;
    const BLUE: u32 = 2;
    const OPEN: u32 = 3;
    const COMMA: u32 = 4;
    const CLOSE: u32 = 5;
    const RED: u32 = 6;
    const EOS: u32 = 7;
    let mut vocabulary = Vocabulary::new(EOS);
    for (bytes, token) in [
        (br#"["red","#.as_slice(), OPEN_RED_COMMA),
        (br#""blue","red"]"#.as_slice(), BLUE_THEN_RED_CLOSE),
        (br#""blue""#.as_slice(), BLUE),
        (b"[".as_slice(), OPEN),
        (b",".as_slice(), COMMA),
        (b"]".as_slice(), CLOSE),
        (br#""red""#.as_slice(), RED),
    ] {
        vocabulary.try_insert(bytes.to_vec(), token).unwrap();
    }
    let compiled = compile_schema(
        br#"{
            "type":"array",
            "items":{"type":"string"},
            "maxItems":3,
            "uniqueItems":true,
            "contains":{"const":"blue"},
            "minContains":1
        }"#,
        &vocabulary,
        8,
        &CompileOptions::default(),
    )
    .unwrap();
    let mut guide = Guide::new(Arc::new(compiled), GuideOptions::default()).unwrap();
    guide.advance(OPEN_RED_COMMA).unwrap();
    let before = guide.debug_snapshot();

    assert!(matches!(
        guide.probe(BLUE_THEN_RED_CLOSE).unwrap(),
        ProbeDecision::Deny(_)
    ));
    assert_eq!(guide.debug_snapshot(), before);
    assert!(matches!(
        guide.advance(BLUE_THEN_RED_CLOSE),
        Err(GuideError::SemanticRejection(_))
    ));
    assert_eq!(guide.debug_snapshot(), before);
    assert_eq!(guide.probe(BLUE).unwrap(), ProbeDecision::Allow);
}
