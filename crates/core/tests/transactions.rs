use oc_sidememory::sidememory::{
    CanonicalArena, ConstraintId, CursorJournal, FrameId, HistoryKey, HistoryStore, InsertResult,
    JsonCursor, RuntimeLimits, SemanticJournal,
};

#[test]
fn multi_event_candidate_restores_cursor_and_arena() {
    let mut cursor = JsonCursor::new(RuntimeLimits::default());
    let mut journal = CursorJournal::new(4096);
    let mut events = Vec::new();
    for byte in br#"["a","# {
        cursor
            .feed_byte_into(*byte, &mut events, &mut journal)
            .unwrap();
        events.clear();
    }
    let mark = journal.mark(&cursor);
    let before = cursor.snapshot();
    let arena_before = cursor.arena().lengths();

    for byte in br#""b","a"]"# {
        cursor
            .feed_byte_into(*byte, &mut events, &mut journal)
            .unwrap();
        events.clear();
    }
    cursor.restore(mark, &mut journal).unwrap();

    assert_eq!(cursor.snapshot(), before);
    assert_eq!(cursor.arena().lengths(), arena_before);
}

#[test]
fn failed_byte_is_atomic_for_the_convenience_api() {
    let mut cursor = JsonCursor::new(RuntimeLimits::default());
    cursor.feed_bytes(br#"{"key""#).unwrap();
    let before = cursor.snapshot();
    let arena_before = cursor.arena().lengths();
    assert!(cursor.feed_byte(b'x').is_err());
    assert_eq!(cursor.snapshot(), before);
    assert_eq!(cursor.arena().lengths(), arena_before);
}

#[test]
fn semantic_journal_restores_insert_and_created_history() {
    let mut arena = CanonicalArena::new(RuntimeLimits::default());
    let value = arena.add_string(b"value").unwrap();
    let key = HistoryKey {
        frame: FrameId {
            slot: 7,
            generation: 2,
        },
        constraint: ConstraintId::from_raw(3),
    };
    let mut histories = HistoryStore::default();
    let mut journal = SemanticJournal::new(16);
    let mark = journal.mark();

    histories.create_journaled(key, 4, &mut journal).unwrap();
    assert_eq!(
        histories
            .insert_journaled(key, &arena, value, 4, 4, &mut journal)
            .unwrap(),
        InsertResult::Inserted
    );
    assert_eq!(histories.item_count(), 1);

    journal.restore(mark, &mut histories).unwrap();
    assert_eq!(histories.len(), 0);
    assert_eq!(histories.item_count(), 0);
}

#[test]
fn closing_history_is_restored_without_copying_its_values() {
    let mut arena = CanonicalArena::new(RuntimeLimits::default());
    let value = arena.add_string(b"value").unwrap();
    let key = HistoryKey {
        frame: FrameId {
            slot: 1,
            generation: 9,
        },
        constraint: ConstraintId::from_raw(0),
    };
    let mut histories = HistoryStore::default();
    let mut journal = SemanticJournal::new(16);
    histories.create_journaled(key, 4, &mut journal).unwrap();
    histories
        .insert_journaled(key, &arena, value, 4, 4, &mut journal)
        .unwrap();
    let mark = journal.mark();

    histories.close_journaled(key, &mut journal).unwrap();
    assert!(histories.get(key).is_none());
    journal.restore(mark, &mut histories).unwrap();

    let restored = histories.get(key).unwrap();
    assert_eq!(restored.len(), 1);
    assert!(restored.contains(&arena, value).unwrap());
}
