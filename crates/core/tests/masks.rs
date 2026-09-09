use std::collections::HashSet;
use std::sync::Arc;

use oc_sidememory::sidememory::{Guide, GuideError, GuideOptions};
use oc_sidememory::{compile_schema, CompileOptions, Vocabulary};

const OPEN: u32 = 0;
const A: u32 = 3;
const COMMA: u32 = 35;
const B: u32 = 67;
const CLOSE: u32 = 68;
const EOS: u32 = 69;
const WIDTH: usize = 70;

fn compiled(unique: bool) -> Arc<oc_sidememory::CompiledSchema> {
    let mut vocabulary = Vocabulary::new(EOS);
    for (bytes, token) in [
        (b"[".as_slice(), OPEN),
        (br#""a""#.as_slice(), A),
        (b",".as_slice(), COMMA),
        (br#""b""#.as_slice(), B),
        (b"]".as_slice(), CLOSE),
    ] {
        vocabulary.try_insert(bytes.to_vec(), token).unwrap();
    }
    let schema = format!(
        r#"{{"type":"array","items":{{"type":"string"}},"maxItems":2,"uniqueItems":{unique}}}"#
    );
    Arc::new(
        compile_schema(
            schema.as_bytes(),
            &vocabulary,
            WIDTH,
            &CompileOptions::default(),
        )
        .unwrap(),
    )
}

fn bits(mask: &[u32]) -> HashSet<u32> {
    (0..u32::try_from(WIDTH).unwrap())
        .filter(|token| mask[*token as usize / 32] & (1_u32 << (*token as usize % 32)) != 0)
        .collect()
}

#[test]
fn sparse_model_width_mask_matches_allowed_tokens_and_clears_padding() {
    let compiled = compiled(true);
    let mut guide = Guide::new(compiled.clone(), GuideOptions::default()).unwrap();
    guide.advance(OPEN).unwrap();
    guide.advance(A).unwrap();
    guide.advance(COMMA).unwrap();

    let structural: HashSet<_> = compiled
        .index()
        .allowed_tokens(&guide.state_id())
        .unwrap()
        .into_iter()
        .collect();
    let allowed: HashSet<_> = guide.allowed_tokens().unwrap().into_iter().collect();
    let mut mask = vec![u32::MAX; WIDTH.div_ceil(32)];
    let summary = guide.write_mask(&mut mask).unwrap();

    assert_eq!(bits(&mask), allowed);
    assert!(allowed.is_subset(&structural));
    assert!(!allowed.contains(&A));
    assert!(allowed.contains(&B));
    assert_eq!(summary.allowed, allowed.len());
    assert_eq!(summary.words, WIDTH.div_ceil(32));
    assert_eq!(mask[2] & !0b11_1111, 0);
}

#[test]
fn invalid_mask_length_is_typed_and_no_plan_path_matches_dfa() {
    let compiled = compiled(false);
    let mut guide = Guide::new(compiled.clone(), GuideOptions::default()).unwrap();
    let mut short = [u32::MAX; 1];
    assert!(matches!(
        guide.write_mask(&mut short),
        Err(GuideError::InvalidMaskBuffer {
            expected_words: 3,
            actual_words: 1
        })
    ));

    let expected: HashSet<_> = compiled
        .index()
        .allowed_tokens(&guide.state_id())
        .unwrap()
        .into_iter()
        .collect();
    let actual: HashSet<_> = guide.allowed_tokens().unwrap().into_iter().collect();
    assert_eq!(actual, expected);
}
