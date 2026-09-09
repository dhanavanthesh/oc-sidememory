use oc_sidememory::sidememory::{FrameSlots, ScopeError};

#[test]
fn reused_slots_change_generation_and_stale_handles_remain_invalid() {
    let mut slots = FrameSlots::default();
    let first = slots.allocate().unwrap();
    slots.release(first).unwrap();
    let second = slots.allocate().unwrap();
    assert_eq!(first.slot, second.slot);
    assert_ne!(first.generation, second.generation);
    assert!(!slots.is_live(first));
    assert!(slots.is_live(second));
    assert!(matches!(
        slots.release(first),
        Err(ScopeError::StaleFrame(_))
    ));
}

#[test]
fn sibling_container_instances_receive_independent_identities() {
    let mut slots = FrameSlots::default();
    let left = slots.allocate().unwrap();
    let right = slots.allocate().unwrap();
    assert_ne!(left, right);
    assert!(slots.is_live(left));
    assert!(slots.is_live(right));
}
