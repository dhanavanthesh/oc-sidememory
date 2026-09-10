use std::collections::HashSet;
use std::sync::Arc;

use oc_sidememory::json_schema::extensions::ExtensionPlanV1;
use oc_sidememory::sidememory::{Guide, GuideError, GuideOptions, ProbeDecision};
use oc_sidememory::{compile_schema, CompileOptions, Vocabulary};

const INVALID: &str = r#"{"id":1,"values":["blue","red","red"],"confirmation":2}"#;
const VALID: &str = r#"{"id":1,"values":["blue","red"],"confirmation":1.0}"#;
const INVALID_TOKEN: u32 = 128;
const VALID_TOKEN: u32 = 129;
const EOS: u32 = 130;

fn guide() -> Guide {
    let mut vocabulary = Vocabulary::new(EOS);
    for byte in 0_u8..=127 {
        vocabulary.try_insert(vec![byte], u32::from(byte)).unwrap();
    }
    vocabulary
        .try_insert(INVALID.as_bytes().to_vec(), INVALID_TOKEN)
        .unwrap();
    vocabulary
        .try_insert(VALID.as_bytes().to_vec(), VALID_TOKEN)
        .unwrap();
    let schema = br#"{
        "type":"object",
        "properties":{
            "id":{"type":"number"},
            "values":{"type":"array","items":{"type":"string","enum":["red","blue"]},"maxItems":3,"uniqueItems":true,"contains":{"const":"blue"}},
            "confirmation":{"type":"number"}
        },
        "required":["id","values","confirmation"],
        "additionalProperties":false
    }"#;
    let extension = ExtensionPlanV1::from_json(
        r#"{"version":1,"objects":[{"schemaPath":"$","propertyOrder":["id","values","confirmation"],"captures":[{"name":"primary","sourceProperty":"id"}],"relations":[{"targetProperty":"confirmation","operator":"equal","capture":"primary"}]}]}"#,
    )
    .unwrap();
    let options = CompileOptions {
        extension_plan: Some(extension),
        ..CompileOptions::default()
    };
    let compiled = compile_schema(schema, &vocabulary, 131, &options).unwrap();
    Guide::new(Arc::new(compiled), GuideOptions::default()).unwrap()
}

fn mask_bits(mask: &[u32]) -> HashSet<u32> {
    (0..=EOS)
        .filter(|token| mask[*token as usize / 32] & (1 << (*token as usize % 32)) != 0)
        .collect()
}

#[test]
fn late_single_token_failure_restores_every_semantic_component() {
    let mut guide = guide();
    let before = guide.debug_snapshot();
    assert!(matches!(
        guide.probe(INVALID_TOKEN).unwrap(),
        ProbeDecision::Deny(_)
    ));
    assert_eq!(guide.debug_snapshot(), before);
    assert!(matches!(
        guide.advance(INVALID_TOKEN),
        Err(GuideError::SemanticRejection(_))
    ));
    assert_eq!(guide.debug_snapshot(), before);
    assert_eq!(guide.probe(VALID_TOKEN).unwrap(), ProbeDecision::Allow);
}

#[test]
fn semantic_mask_and_allowed_tokens_agree_for_composed_candidate() {
    let mut guide = guide();
    let allowed: HashSet<_> = guide.allowed_tokens().unwrap().into_iter().collect();
    let mut mask = vec![u32::MAX; guide.model_width().div_ceil(32)];
    guide.write_mask(&mut mask).unwrap();
    assert_eq!(mask_bits(&mask), allowed);
    assert!(!allowed.contains(&INVALID_TOKEN));
    assert!(allowed.contains(&VALID_TOKEN));
}

#[test]
fn successful_object_and_eos_are_independent_rollback_boundaries() {
    let mut guide = guide();
    let initial = guide.debug_snapshot();
    guide.advance(VALID_TOKEN).unwrap();
    let before_eos = guide.debug_snapshot();
    guide.advance(EOS).unwrap();
    assert!(guide.is_terminated());
    guide.rollback(1).unwrap();
    assert_eq!(guide.debug_snapshot(), before_eos);
    guide.rollback(1).unwrap();
    assert_eq!(guide.debug_snapshot(), initial);
}
