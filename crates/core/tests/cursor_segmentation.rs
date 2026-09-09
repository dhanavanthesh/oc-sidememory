mod common;

use oc_sidememory::sidememory::{JsonCursor, RuntimeLimits};

const CORPUS: &[&[u8]] = &[
    b"null",
    b"true",
    b"false",
    b"0",
    b"-0",
    b"1.2300e2",
    b"\"hello\"",
    b"\"\\u00e9\"",
    b"\"\\uD83D\\uDE00\"",
    b"[]",
    b"[1]",
    b"[1,2]",
    b"{}",
    b"{\"a\":1}",
    b"{\"a\":[1,{\"b\":\"x\"}]}",
];

#[test]
fn every_single_boundary_split_has_identical_logical_result() {
    for document in CORPUS {
        let expected = common::parse(document);
        for split in 0..=document.len() {
            let mut actual = JsonCursor::new(RuntimeLimits::default());
            actual.feed_bytes(&document[..split]).unwrap();
            actual.feed_bytes(&document[split..]).unwrap();
            actual.finish_eos().unwrap();
            assert_eq!(
                actual.snapshot(),
                expected.snapshot(),
                "split {split} in {:?}",
                document
            );
            assert!(actual
                .arena()
                .equal_across(
                    actual.root().unwrap(),
                    expected.arena(),
                    expected.root().unwrap()
                )
                .unwrap());
        }
    }
}

#[test]
fn deterministic_many_chunk_partitions_match_one_chunk() {
    let document = "{\"a\":[1,{\"b\":\"é\",\"c\":\"\\uD83D\\uDE00\"}],\"d\":false}".as_bytes();
    let expected = common::parse(document);
    for stride in 1..=11 {
        let mut actual = JsonCursor::new(RuntimeLimits::default());
        for chunk in document.chunks(stride) {
            actual.feed_bytes(chunk).unwrap();
        }
        actual.finish_eos().unwrap();
        assert!(actual
            .arena()
            .equal_across(
                actual.root().unwrap(),
                expected.arena(),
                expected.root().unwrap()
            )
            .unwrap());
    }
}
