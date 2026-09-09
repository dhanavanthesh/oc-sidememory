use std::sync::Arc;

use oc_sidememory::sidememory::{Guide, GuideOptions, ProbeDecision};
use oc_sidememory::{compile_schema, CompileOptions, Vocabulary};

const OPEN: u32 = 0;
const VALUE: u32 = 10;
const COMMA: u32 = 11;
const ALIAS: u32 = 87;
const CLOSE: u32 = 88;
const EOS: u32 = 95;

#[test]
fn token_ids_with_identical_bytes_have_identical_semantic_results() {
    let mut vocabulary = Vocabulary::new(EOS);
    for (bytes, token) in [
        (b"[".as_slice(), OPEN),
        (br#""red""#.as_slice(), VALUE),
        (b",".as_slice(), COMMA),
        (br#""red""#.as_slice(), ALIAS),
        (b"]".as_slice(), CLOSE),
    ] {
        vocabulary.try_insert(bytes.to_vec(), token).unwrap();
    }
    let compiled = compile_schema(
        br#"{
            "type":"array",
            "items":{"type":"string"},
            "maxItems":2,
            "uniqueItems":true
        }"#,
        &vocabulary,
        usize::try_from(EOS).unwrap() + 1,
        &CompileOptions::default(),
    )
    .unwrap();
    let mut guide = Guide::new(Arc::new(compiled), GuideOptions::default()).unwrap();
    guide.advance(OPEN).unwrap();
    guide.advance(VALUE).unwrap();
    guide.advance(COMMA).unwrap();

    assert_eq!(guide.probe(VALUE).unwrap(), guide.probe(ALIAS).unwrap());
    assert!(matches!(
        guide.probe(VALUE).unwrap(),
        ProbeDecision::Deny(_)
    ));
    let allowed = guide.allowed_tokens().unwrap();
    assert!(!allowed.contains(&VALUE));
    assert!(!allowed.contains(&ALIAS));
}
