use oc_sidememory::sidememory::{CompileLimits, TokenFlags, TokenTable, VocabularyError};
use oc_sidememory::Vocabulary;

#[test]
fn dense_sparse_missing_and_alias_ids_are_unambiguous() {
    let mut vocabulary = Vocabulary::new(9);
    vocabulary.try_insert(b"red".to_vec(), 0).unwrap();
    vocabulary.try_insert(b"red".to_vec(), 7).unwrap();
    vocabulary.try_insert(b"blue".to_vec(), 2).unwrap();
    let table = TokenTable::build(&vocabulary, 10, &CompileLimits::default()).unwrap();
    assert_eq!(table.get(0).unwrap().bytes(), b"red");
    assert_eq!(table.get(7).unwrap().bytes(), b"red");
    assert!(matches!(
        table.get(1),
        Err(VocabularyError::UnknownToken { id: 1 })
    ));
    assert!(!table
        .get(2)
        .unwrap()
        .flags()
        .contains(TokenFlags::HAS_DIGIT));
}

#[test]
fn repeated_mapping_is_ignored_but_conflicting_identity_is_rejected() {
    let mut vocabulary = Vocabulary::new(3);
    vocabulary.try_insert(b"a".to_vec(), 1).unwrap();
    vocabulary.try_insert(b"a".to_vec(), 1).unwrap();
    TokenTable::build(&vocabulary, 4, &CompileLimits::default()).unwrap();
    vocabulary.try_insert(b"b".to_vec(), 1).unwrap();
    assert!(matches!(
        TokenTable::build(&vocabulary, 4, &CompileLimits::default()),
        Err(VocabularyError::ConflictingTokenBytes { id: 1, .. })
    ));
}

#[test]
fn width_eos_empty_and_allocation_limits_are_checked_before_allocation() {
    let mut vocabulary = Vocabulary::new(4);
    vocabulary.try_insert(Vec::<u8>::new(), 1).unwrap();
    assert!(matches!(
        TokenTable::build(&vocabulary, 5, &CompileLimits::default()),
        Err(VocabularyError::EmptyOrdinaryToken)
    ));
    assert!(matches!(
        TokenTable::build(&Vocabulary::new(1), 0, &CompileLimits::default()),
        Err(VocabularyError::InvalidModelWidth)
    ));
    assert!(matches!(
        TokenTable::build(&Vocabulary::new(5), 5, &CompileLimits::default()),
        Err(VocabularyError::TokenOutsideModelWidth { id: 5, .. })
    ));
    let limits = CompileLimits {
        max_token_table_bytes: 1,
        ..CompileLimits::default()
    };
    assert!(matches!(
        TokenTable::build(&Vocabulary::new(1), 2, &limits),
        Err(VocabularyError::Resource(_))
    ));
}

#[test]
fn ordinary_ids_outside_width_and_eos_control_are_rejected() {
    let mut vocabulary = Vocabulary::new(4);
    vocabulary.try_insert(b"a".to_vec(), 5).unwrap();
    assert!(matches!(
        TokenTable::build(&vocabulary, 5, &CompileLimits::default()),
        Err(VocabularyError::TokenOutsideModelWidth { id: 5, .. })
    ));
    let table = TokenTable::build(&Vocabulary::new(0), 1, &CompileLimits::default()).unwrap();
    assert!(matches!(
        table.get(0),
        Err(VocabularyError::EosIsControl { id: 0 })
    ));
}
