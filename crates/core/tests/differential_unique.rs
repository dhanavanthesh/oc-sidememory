mod common;

use std::sync::Arc;

use oc_sidememory::sidememory::{Guide, GuideOptions, SequenceDecision};
use oc_sidememory::{compile_schema, CompileOptions};

fn accepted(compiled: &Arc<oc_sidememory::CompiledSchema>, document: &[u8]) -> bool {
    let tokens: Vec<u32> = document.iter().map(|byte| u32::from(*byte)).collect();
    let mut guide = Guide::new(Arc::clone(compiled), GuideOptions::default()).unwrap();
    let decision = guide
        .probe_sequence(&tokens, true)
        .unwrap_or_else(|error| panic!("failed to process {document:?}: {error:?}"));
    matches!(decision, SequenceDecision::Allow)
}

#[test]
fn exhaustive_three_value_domain_matches_independent_oracle() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let compiled = Arc::new(
        compile_schema(
            br#"{
                "type":"array",
                "items":{"type":"string","enum":["a","b","c"]},
                "minItems":0,
                "maxItems":4,
                "uniqueItems":true
            }"#,
            &vocabulary,
            width,
            &CompileOptions::default(),
        )
        .unwrap(),
    );
    let values = ["a", "b", "c"];
    let mut total = 0;
    let mut accepted_count = 0;
    let mut false_accepts = 0;
    let mut false_rejects = 0;

    for len in 0_u32..=4 {
        let combinations = 3_u32.pow(len);
        for ordinal in 0..combinations {
            let mut cursor = ordinal;
            let mut items = Vec::with_capacity(usize::try_from(len).unwrap());
            for _ in 0..len {
                let index = usize::try_from(cursor % 3).unwrap();
                items.push(values[index]);
                cursor /= 3;
            }
            let document = format!(r#"["{}"]"#, items.join(r#"",""#));
            let document = if items.is_empty() {
                "[]".to_owned()
            } else {
                document
            };
            let oracle = items
                .iter()
                .enumerate()
                .all(|(index, value)| !items[..index].contains(value));
            let actual = accepted(&compiled, document.as_bytes());
            total += 1;
            accepted_count += usize::from(actual);
            false_accepts += usize::from(actual && !oracle);
            false_rejects += usize::from(!actual && oracle);
        }
    }

    assert_eq!(total, 121);
    assert_eq!(accepted_count, 16);
    assert_eq!(false_accepts, 0);
    assert_eq!(false_rejects, 0);
}

#[test]
fn exact_json_equality_controls_uniqueness() {
    assert_cases(
        br#"{"type":"number"}"#,
        &[br#"[1,1.0]"#, br#"[1,1e0]"#, br#"[0,-0]"#],
        &[br#"[9007199254740992,9007199254740993]"#],
        false,
    );
    assert_cases(br#"{"type":"null"}"#, &[br#"[null,null]"#], &[], false);
    assert_cases(
        br#"{"type":"array","items":{"type":"string"}}"#,
        &[br#"[["a","b"],["a","b"]]"#],
        &[br#"[["a","b"],["b","a"]]"#],
        false,
    );
    assert_cases(
        br#"{
            "type":"object",
            "properties":{"a":{"type":"number"},"b":{"type":"number"}},
            "required":["a","b"],
            "additionalProperties":false
        }"#,
        &[br#"[{"a":1,"b":2},{"b":2.0,"a":1e0}]"#],
        &[],
        false,
    );
    assert_cases(
        br#"{"type":"string"}"#,
        &[r#"["é","\u00e9"]"#.as_bytes()],
        &[r#"["é","e\u0301"]"#.as_bytes()],
        true,
    );
    assert_cases(
        br#"{"enum":[true,false,1,0]}"#,
        &[],
        &[br#"[true,1]"#, br#"[false,0]"#],
        false,
    );
}

fn assert_cases(
    item_schema: &[u8],
    duplicates: &[&[u8]],
    distinct: &[&[u8]],
    full_byte_vocabulary: bool,
) {
    let (vocabulary, width) = if full_byte_vocabulary {
        common::byte_vocabulary()
    } else {
        common::ascii_vocabulary()
    };
    let item_schema = std::str::from_utf8(item_schema).unwrap();
    let schema = format!(r#"{{"type":"array","items":{item_schema},"uniqueItems":true}}"#);
    let compiled = Arc::new(
        compile_schema(
            schema.as_bytes(),
            &vocabulary,
            width,
            &CompileOptions::default(),
        )
        .unwrap(),
    );
    for document in duplicates {
        assert!(!accepted(&compiled, document), "accepted {document:?}");
    }
    for document in distinct {
        assert!(accepted(&compiled, document), "rejected {document:?}");
    }
}
