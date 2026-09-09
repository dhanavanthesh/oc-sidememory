use oc_sidememory::sidememory::{CanonicalArena, CanonicalNumber, RuntimeLimits};

#[test]
fn arrays_preserve_order() {
    let mut arena = CanonicalArena::new(RuntimeLimits::default());
    let a = arena.add_string(b"a").unwrap();
    let b = arena.add_string(b"b").unwrap();
    let left = arena.add_array(&[a, b]).unwrap();
    let right = arena.add_array(&[b, a]).unwrap();
    assert!(!arena.equal(left, right).unwrap());
}

#[test]
fn objects_ignore_member_order_and_numeric_spelling() {
    let limits = RuntimeLimits::default();
    let mut arena = CanonicalArena::new(limits.clone());
    let one = arena
        .add_number(CanonicalNumber::parse(b"1", &limits).unwrap())
        .unwrap();
    let one_alt = arena
        .add_number(CanonicalNumber::parse(b"1e0", &limits).unwrap())
        .unwrap();
    let two = arena
        .add_number(CanonicalNumber::parse(b"2", &limits).unwrap())
        .unwrap();
    let two_alt = arena
        .add_number(CanonicalNumber::parse(b"2.0", &limits).unwrap())
        .unwrap();
    let left = arena
        .add_object(vec![(b"a".to_vec(), one), (b"b".to_vec(), two)])
        .unwrap();
    let right = arena
        .add_object(vec![(b"b".to_vec(), two_alt), (b"a".to_vec(), one_alt)])
        .unwrap();
    assert!(arena.equal(left, right).unwrap());
}

#[test]
fn duplicate_decoded_keys_are_rejected_before_publication() {
    let mut arena = CanonicalArena::new(RuntimeLimits::default());
    let value = arena.add_null().unwrap();
    let mark = arena.mark();
    assert!(arena
        .add_object(vec![(b"a".to_vec(), value), (b"a".to_vec(), value)])
        .is_err());
    assert_eq!(arena.mark(), mark);
}
