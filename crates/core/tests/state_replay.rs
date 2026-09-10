#![cfg(debug_assertions)]

use std::sync::Arc;

use oc_sidememory::json_schema::extensions::ExtensionPlanV1;
use oc_sidememory::sidememory::{verify_guide_replay, Guide, GuideOptions, ProbeDecision};
use oc_sidememory::{compile_schema, CompileOptions, Vocabulary};

const OPEN: u32 = 0;
const A: u32 = 1;
const COMMA: u32 = 2;
const B: u32 = 3;
const CLOSE: u32 = 4;
const INNER_A: u32 = 5;
const INNER_B: u32 = 6;
const EOS: u32 = 9;

fn guide(schema: &[u8]) -> Guide {
    let mut vocabulary = Vocabulary::new(EOS);
    for (bytes, token) in [
        (b"[".as_slice(), OPEN),
        (br#""a""#.as_slice(), A),
        (b",".as_slice(), COMMA),
        (br#""b""#.as_slice(), B),
        (b"]".as_slice(), CLOSE),
        (br#"["a"]"#.as_slice(), INNER_A),
        (br#"["b"]"#.as_slice(), INNER_B),
    ] {
        vocabulary.try_insert(bytes.to_vec(), token).unwrap();
    }
    let compiled = compile_schema(
        schema,
        &vocabulary,
        usize::try_from(EOS).unwrap() + 1,
        &CompileOptions::default(),
    )
    .unwrap();
    Guide::new(Arc::new(compiled), GuideOptions::default()).unwrap()
}

#[test]
fn replay_matches_after_every_lifecycle_operation() {
    let mut decoder =
        guide(br#"{"type":"array","items":{"type":"string"},"maxItems":2,"uniqueItems":true}"#);
    verify_guide_replay(&decoder).unwrap();
    decoder.advance(OPEN).unwrap();
    decoder.advance(A).unwrap();
    decoder.advance(COMMA).unwrap();
    verify_guide_replay(&decoder).unwrap();

    assert!(matches!(decoder.probe(A).unwrap(), ProbeDecision::Deny(_)));
    assert!(decoder.advance(A).is_err());
    verify_guide_replay(&decoder).unwrap();

    decoder.advance(B).unwrap();
    decoder.advance(CLOSE).unwrap();
    decoder.advance(EOS).unwrap();
    verify_guide_replay(&decoder).unwrap();
    decoder.rollback(1).unwrap();
    verify_guide_replay(&decoder).unwrap();
    decoder.reset().unwrap();
    verify_guide_replay(&decoder).unwrap();
}

#[test]
fn replay_matches_nested_uniqueness_histories_by_value() {
    let mut decoder = guide(
        br#"{
            "type":"array",
            "items":{"type":"array","items":{"type":"string"},"uniqueItems":true},
            "maxItems":2,
            "uniqueItems":true
        }"#,
    );
    for token in [OPEN, INNER_A, COMMA, INNER_B, CLOSE] {
        decoder.advance(token).unwrap();
        verify_guide_replay(&decoder).unwrap();
    }
    decoder.rollback(2).unwrap();
    verify_guide_replay(&decoder).unwrap();
}

#[test]
fn replay_matches_live_and_closed_capture_registers() {
    let mut vocabulary = Vocabulary::new(128);
    for byte in 0_u8..=127 {
        vocabulary.try_insert(vec![byte], u32::from(byte)).unwrap();
    }
    let schema = br#"{
        "type":"object",
        "properties":{"source":{"type":"string"},"target":{"type":"string"}},
        "required":["source","target"],
        "additionalProperties":false
    }"#;
    let extension = ExtensionPlanV1::from_json(
        r#"{"version":1,"objects":[{"schemaPath":"$","propertyOrder":["source","target"],"captures":[{"name":"saved","sourceProperty":"source"}],"relations":[{"targetProperty":"target","operator":"equal","capture":"saved"}]}]}"#,
    )
    .unwrap();
    let options = CompileOptions {
        extension_plan: Some(extension),
        ..CompileOptions::default()
    };
    let compiled = Arc::new(compile_schema(schema, &vocabulary, 129, &options).unwrap());
    let mut guide = Guide::new(compiled, GuideOptions::default()).unwrap();
    for byte in br#"{"source":"x","#.iter().copied() {
        guide.advance(u32::from(byte)).unwrap();
    }
    verify_guide_replay(&guide).unwrap();
    for byte in br#""target":"x"}"#.iter().copied() {
        guide.advance(u32::from(byte)).unwrap();
    }
    verify_guide_replay(&guide).unwrap();
    guide.rollback(1).unwrap();
    verify_guide_replay(&guide).unwrap();
}
