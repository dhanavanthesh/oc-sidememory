mod common;

use oc_sidememory::json_schema::compile::{compile_schema, CompileOptions};
use oc_sidememory::json_schema::diagnostic::CompileError;
use oc_sidememory::json_schema::ir::ScalarLiteral;

#[test]
fn adjacent_integers_above_binary_float_precision_remain_distinct() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let left = compile_schema(
        br#"{"const":9007199254740992}"#,
        &vocabulary,
        width,
        &CompileOptions::default(),
    )
    .unwrap();
    let right = compile_schema(
        br#"{"const":9007199254740993}"#,
        &vocabulary,
        width,
        &CompileOptions::default(),
    )
    .unwrap();
    assert_ne!(
        left.ir().nodes()[0].scalar.const_value,
        right.ir().nodes()[0].scalar.const_value
    );
}

#[test]
fn huge_exponent_is_preserved_without_fixed_width_conversion() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let compiled = compile_schema(
        br#"{"const":1e2147483648}"#,
        &vocabulary,
        width,
        &CompileOptions::default(),
    )
    .unwrap();
    let Some(ScalarLiteral::Number(number)) = &compiled.ir().nodes()[0].scalar.const_value else {
        panic!("number literal expected");
    };
    assert_eq!(number.exponent(), "2147483648");
}

#[test]
fn numeric_const_and_enum_lower_to_plain_decimal() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let cases = [
        (br#"{"const":5}"#.as_slice(), "5"),
        (
            br#"{"const":9007199254740993}"#.as_slice(),
            "9007199254740993",
        ),
        (br#"{"type":"integer","const":-42}"#.as_slice(), "-42"),
        (br#"{"const":1.2300e2}"#.as_slice(), "123"),
        (br#"{"const":10e-1}"#.as_slice(), "1"),
        (br#"{"const":1.5}"#.as_slice(), r"1\.5"),
        (br#"{"const":-0.007}"#.as_slice(), r"-0\.007"),
    ];
    for (schema, expected) in cases {
        let compiled = compile_schema(schema, &vocabulary, width, &CompileOptions::default())
            .expect("compiles");
        let plan = compiled.regular_plan();
        assert!(
            plan.contains(expected) && !plan.contains('e') && !plan.contains('E'),
            "schema {} -> {plan}",
            std::str::from_utf8(schema).unwrap()
        );
    }
}

#[test]
fn absurd_exponent_literal_keeps_an_exact_scientific_grammar() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let compiled = compile_schema(
        br#"{"const":1e2147483648}"#,
        &vocabulary,
        width,
        &CompileOptions::default(),
    )
    .expect("compiles");
    let plan = compiled.regular_plan();
    assert!(plan.contains("2147483648") && plan.contains('e'), "{plan}");
}

#[test]
fn string_const_grammar_matches_exact_json_encoding() {
    let (vocabulary, width) = common::ascii_vocabulary();
    for encoded in [br#""x""#.as_slice(), br#""a\"b""#.as_slice()] {
        let schema = format!(
            r#"{{"const":{}}}"#,
            std::str::from_utf8(encoded).expect("ASCII JSON string")
        );
        let compiled = compile_schema(
            schema.as_bytes(),
            &vocabulary,
            width,
            &CompileOptions::default(),
        )
        .expect("compiles");
        let mut state = compiled.index().initial_state();
        for byte in encoded {
            state = compiled
                .index()
                .next_state(&state, &u32::from(*byte))
                .expect("exact JSON encoding remains accepted");
        }
        assert!(compiled.index().is_final_state(&state));
    }
}

#[test]
fn mathematically_duplicate_enum_values_are_rejected_by_declared_policy() {
    let vocabulary = oc_sidememory::Vocabulary::new(0);
    let error = compile_schema(
        br#"{"enum":[1,1.0,1e0]}"#,
        &vocabulary,
        1,
        &CompileOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(error, CompileError::InvalidKeywordValue { .. }));
}
