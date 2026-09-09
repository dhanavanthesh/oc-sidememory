use oc_sidememory::sidememory::{CanonicalArena, RuntimeLimits};

#[test]
fn failed_growth_does_not_mutate_arena() {
    let limits = RuntimeLimits {
        max_value_bytes: 2,
        ..RuntimeLimits::default()
    };
    let mut arena = CanonicalArena::new(limits);
    let before = arena.mark();
    assert!(arena.add_string(b"abc").is_err());
    assert_eq!(arena.mark(), before);
}

#[test]
fn marks_restore_all_logical_lengths() {
    let mut arena = CanonicalArena::new(RuntimeLimits::default());
    let mark = arena.mark();
    let value = arena.add_string(b"temporary").unwrap();
    arena.add_array(&[value]).unwrap();
    arena.restore(mark).unwrap();
    assert_eq!(arena.lengths(), (0, 0, 0, 0));
}

#[test]
fn deep_iterative_equality_does_not_recurse() {
    let limits = RuntimeLimits {
        max_arena_nodes: 10_000,
        ..RuntimeLimits::default()
    };
    let mut arena = CanonicalArena::new(limits);
    let mut left = arena.add_null().unwrap();
    let mut right = arena.add_null().unwrap();
    for _ in 0..2_000 {
        left = arena.add_array(&[left]).unwrap();
        right = arena.add_array(&[right]).unwrap();
    }
    assert!(arena.equal(left, right).unwrap());
}
