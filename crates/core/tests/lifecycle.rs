use std::sync::Arc;

use oc_sidememory::sidememory::{Guide, GuideError, GuideOptions, Lifecycle, ProbeDecision};
use oc_sidememory::{compile_schema, CompileOptions, Vocabulary};

const OPEN: u32 = 0;
const CLOSE: u32 = 1;
const EOS: u32 = 7;
const WIDTH: usize = 8;

fn guide(max_rollback_tokens: usize) -> Guide {
    let mut vocabulary = Vocabulary::new(EOS);
    vocabulary.try_insert(b"[".to_vec(), OPEN).unwrap();
    vocabulary.try_insert(b"]".to_vec(), CLOSE).unwrap();
    let compiled = compile_schema(
        br#"{"type":"array","maxItems":0,"uniqueItems":true}"#,
        &vocabulary,
        WIDTH,
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
fn eos_probe_commit_and_rollback_are_transactional() {
    let mut guide = guide(8);
    guide.advance(OPEN).unwrap();
    guide.advance(CLOSE).unwrap();
    let state = guide.state_id();
    let rollback = guide.rollback_available();

    assert!(guide.is_accepting().unwrap());
    assert!(!guide.is_terminated());
    assert_eq!(guide.probe(EOS).unwrap(), ProbeDecision::Allow);
    assert!(!guide.is_terminated());
    assert_eq!(guide.state_id(), state);
    assert_eq!(guide.rollback_available(), rollback);

    guide.advance(EOS).unwrap();
    assert!(guide.is_accepting().unwrap());
    assert!(guide.is_terminated());
    assert!(guide.allowed_tokens().unwrap().is_empty());
    let mut mask = [u32::MAX];
    guide.write_mask(&mut mask).unwrap();
    assert_eq!(mask, [0]);
    assert!(matches!(
        guide.advance(EOS),
        Err(GuideError::InvalidLifecycle {
            state: Lifecycle::Terminated,
            ..
        })
    ));

    guide.rollback(1).unwrap();
    assert!(!guide.is_terminated());
    assert!(guide.is_accepting().unwrap());
    assert_eq!(guide.state_id(), state);
}

#[test]
fn reset_reconstructs_initial_state_and_clears_rollback() {
    let mut guide = guide(8);
    let initial = guide.state_id();
    guide.advance(OPEN).unwrap();
    guide.rollback(0).unwrap();
    assert_eq!(guide.rollback_available(), 1);
    guide.reset().unwrap();
    assert_eq!(guide.state_id(), initial);
    assert_eq!(guide.rollback_available(), 0);
    assert!(!guide.is_accepting().unwrap());
    assert!(!guide.is_terminated());
}

#[test]
fn zero_rollback_retains_no_checkpoint() {
    let mut guide = guide(0);
    guide.advance(OPEN).unwrap();
    guide.advance(CLOSE).unwrap();
    assert_eq!(guide.rollback_available(), 0);
    assert_eq!(guide.retained_rollback_bytes(), 0);
    assert!(matches!(
        guide.rollback(1),
        Err(GuideError::RollbackUnavailable {
            requested: 1,
            available: 0
        })
    ));
}
