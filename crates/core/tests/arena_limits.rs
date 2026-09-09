use oc_sidememory::sidememory::{ArenaError, CanonicalArena, ResourceError, RuntimeLimits};

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

#[test]
fn value_and_arena_byte_limits_are_independent() {
    let limits = RuntimeLimits {
        max_value_bytes: 3,
        max_arena_bytes: 8,
        ..RuntimeLimits::default()
    };
    let mut arena = CanonicalArena::new(limits);
    arena.add_string(b"abc").unwrap();
    arena.add_string(b"def").unwrap();

    assert!(matches!(
        arena.add_string(b"long"),
        Err(ArenaError::Resource(ResourceError::LimitExceeded {
            limit_name: "max_value_bytes",
            ..
        }))
    ));
    assert!(matches!(
        arena.add_string(b"ghi"),
        Err(ArenaError::Resource(ResourceError::LimitExceeded {
            limit_name: "max_arena_bytes",
            ..
        }))
    ));
}

#[test]
fn object_keys_use_the_arena_byte_limit() {
    let limits = RuntimeLimits {
        max_value_bytes: 3,
        max_arena_bytes: 5,
        ..RuntimeLimits::default()
    };
    let mut arena = CanonicalArena::new(limits);
    let value = arena.add_null().unwrap();
    let before = arena.mark();
    let error = arena
        .add_object(vec![(b"abc".to_vec(), value), (b"def".to_vec(), value)])
        .unwrap_err();
    assert!(matches!(
        error,
        ArenaError::Resource(ResourceError::LimitExceeded {
            limit_name: "max_arena_bytes",
            ..
        })
    ));
    assert_eq!(arena.mark(), before);
}
