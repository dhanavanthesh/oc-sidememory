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
