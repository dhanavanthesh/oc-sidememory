#![cfg(debug_assertions)]

use oc_sidememory::index::Index;
use oc_sidememory::sidememory::{
    replay_committed, CompileLimits, ReplayError, RuntimeLimits, TokenTable,
};
use oc_sidememory::Vocabulary;

fn substrate() -> (Index, TokenTable) {
    let mut vocabulary = Vocabulary::new(5);
    for (bytes, id) in [
        (b"[".as_slice(), 0),
        (b"12]".as_slice(), 1),
        (b"1".as_slice(), 2),
        (b"2".as_slice(), 3),
        (b"]".as_slice(), 4),
    ] {
        vocabulary.try_insert(bytes.to_vec(), id).unwrap();
    }
    let index = Index::new(r"\[12\]", &vocabulary).unwrap();
    let table = TokenTable::build(&vocabulary, 6, &CompileLimits::default()).unwrap();
    (index, table)
}

#[test]
fn replay_is_independent_of_token_segmentation() {
    let (index, table) = substrate();
    let limits = RuntimeLimits::default();
    let combined = replay_committed(&index, &table, &[0, 1, 5], limits.clone()).unwrap();
    let split = replay_committed(&index, &table, &[0, 2, 3, 4, 5], limits).unwrap();
    assert_eq!(combined, split);
}

#[test]
fn replay_checks_dfa_and_eos_before_cursor_mutation() {
    let (index, table) = substrate();
    assert!(matches!(
        replay_committed(&index, &table, &[2], RuntimeLimits::default()),
        Err(ReplayError::MissingTransition {
            state: _,
            token_id: 2
        })
    ));
    assert!(matches!(
        replay_committed(&index, &table, &[5], RuntimeLimits::default()),
        Err(ReplayError::EosNotAllowed {
            state: _,
            token_id: 5
        })
    ));
}

#[test]
fn replay_rejects_tokens_after_eos() {
    let (index, table) = substrate();
    assert!(matches!(
        replay_committed(&index, &table, &[0, 1, 5, 0], RuntimeLimits::default()),
        Err(ReplayError::TokenAfterEos { token_id: 0 })
    ));
}
