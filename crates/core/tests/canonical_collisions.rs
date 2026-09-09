use std::hash::{BuildHasher, Hasher};

use oc_sidememory::sidememory::{CanonicalArena, RuntimeLimits};

#[derive(Clone)]
struct ConstantBuildHasher;

struct ConstantHasher;

impl BuildHasher for ConstantBuildHasher {
    type Hasher = ConstantHasher;

    fn build_hasher(&self) -> Self::Hasher {
        ConstantHasher
    }
}

impl Hasher for ConstantHasher {
    fn finish(&self) -> u64 {
        0
    }

    fn write(&mut self, _: &[u8]) {}
}

#[test]
fn forced_fingerprint_collisions_never_replace_exact_equality() {
    let mut arena = CanonicalArena::new(RuntimeLimits::default());
    let left = arena.add_string(b"left").unwrap();
    let right = arena.add_string(b"right").unwrap();
    assert_eq!(arena.fingerprint(left, &ConstantBuildHasher).unwrap(), 0);
    assert_eq!(arena.fingerprint(right, &ConstantBuildHasher).unwrap(), 0);
    assert!(!arena.equal(left, right).unwrap());
}
