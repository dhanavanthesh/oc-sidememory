mod common;

use std::hash::{BuildHasher, Hasher};

use oc_sidememory::json_schema::compile::{compile_schema, CompileOptions};
use oc_sidememory::sidememory::{
    CanonicalArena, CanonicalNumber, CanonicalSet, ConstraintId, FrameId, HistoryKey, HistoryStore,
    InsertResult, RuntimeLimits,
};

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
fn plan_dispatches_unique_items_by_schema_node() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let compiled = compile_schema(
        br#"{"type":"array","items":{"type":"string"},"uniqueItems":true}"#,
        &vocabulary,
        width,
        &CompileOptions::default(),
    )
    .unwrap();
    let root = compiled.ir().root();
    let constraint = compiled.memory_plan().unique_for_node(root).unwrap();
    assert_eq!(constraint.array_schema_node, root);
    assert_eq!(constraint.constraint_id.get(), 0);
    assert!(compiled
        .memory_plan()
        .unique_for_node(constraint.item_schema_node)
        .is_none());
}

#[test]
fn exact_equality_resolves_forced_fingerprint_collisions() {
    let limits = RuntimeLimits::default();
    let mut arena = CanonicalArena::new(limits.clone());
    let one = arena
        .add_number(CanonicalNumber::parse(b"1", &limits).unwrap())
        .unwrap();
    let one_point_zero = arena
        .add_number(CanonicalNumber::parse(b"1.0", &limits).unwrap())
        .unwrap();
    let two = arena
        .add_number(CanonicalNumber::parse(b"2", &limits).unwrap())
        .unwrap();
    let mut set = CanonicalSet::with_hash_builder(ConstantBuildHasher);
    assert_eq!(set.insert(&arena, one).unwrap(), InsertResult::Inserted);
    assert_eq!(
        set.insert(&arena, one_point_zero).unwrap(),
        InsertResult::Duplicate
    );
    assert_eq!(set.insert(&arena, two).unwrap(), InsertResult::Inserted);
    assert_eq!(set.len(), 2);
}

#[test]
fn sibling_array_instances_keep_independent_histories() {
    let limits = RuntimeLimits::default();
    let mut arena = CanonicalArena::new(limits);
    let value = arena.add_string(b"same").unwrap();
    let constraint = ConstraintId::from_raw(0);
    let left = HistoryKey {
        frame: FrameId {
            slot: 0,
            generation: 0,
        },
        constraint,
    };
    let right = HistoryKey {
        frame: FrameId {
            slot: 1,
            generation: 0,
        },
        constraint,
    };
    let mut histories = HistoryStore::default();
    histories.create_limited(left, 2).unwrap();
    histories.create_limited(right, 2).unwrap();
    assert_eq!(
        histories.insert_limited(left, &arena, value, 2, 2).unwrap(),
        InsertResult::Inserted
    );
    assert_eq!(
        histories
            .insert_limited(right, &arena, value, 2, 2)
            .unwrap(),
        InsertResult::Inserted
    );
    assert_eq!(histories.item_count(), 2);
}

#[test]
fn history_rejects_objects_with_reordered_keys_and_equal_numbers() {
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
    let mut set = CanonicalSet::default();
    assert_eq!(set.insert(&arena, left).unwrap(), InsertResult::Inserted);
    assert_eq!(set.insert(&arena, right).unwrap(), InsertResult::Duplicate);
}
