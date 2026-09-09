use oc_sidememory::sidememory::{CanonicalArena, CanonicalNumber, RuntimeLimits};

#[test]
fn json_types_remain_distinct_and_numbers_are_exact() {
    let limits = RuntimeLimits::default();
    let mut arena = CanonicalArena::new(limits.clone());
    let boolean = arena.add_bool(true).unwrap();
    let number = arena
        .add_number(CanonicalNumber::parse(b"1", &limits).unwrap())
        .unwrap();
    let number_alt = arena
        .add_number(CanonicalNumber::parse(b"1.0", &limits).unwrap())
        .unwrap();
    assert!(!arena.equal(boolean, number).unwrap());
    assert!(arena.equal(number, number_alt).unwrap());
}

#[test]
fn decoded_string_bytes_are_compared_without_unicode_normalization() {
    let mut arena = CanonicalArena::new(RuntimeLimits::default());
    let composed = arena.add_string("é".as_bytes()).unwrap();
    let same = arena.add_string(&[0xc3, 0xa9]).unwrap();
    let decomposed = arena.add_string("e\u{301}".as_bytes()).unwrap();
    assert!(arena.equal(composed, same).unwrap());
    assert!(!arena.equal(composed, decomposed).unwrap());
}
