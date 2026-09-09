use oc_sidememory::sidememory::{CanonicalNumber, NumberError, RuntimeLimits};

fn number(value: &str) -> CanonicalNumber {
    CanonicalNumber::parse(value.as_bytes(), &RuntimeLimits::default()).unwrap()
}

#[test]
fn equivalent_decimal_spellings_normalize_identically() {
    for value in ["1", "1.0", "1e0", "10e-1"] {
        assert_eq!(number(value), number("1"), "{value}");
    }
    assert_eq!(number("1.2300e2"), number("123"));
}

#[test]
fn every_zero_has_one_unsigned_representation() {
    for value in ["0", "-0", "0.0", "0e999999"] {
        let value = number(value);
        assert!(!value.is_negative());
        assert_eq!(value.coefficient(), "0");
        assert_eq!(value.exponent(), "0");
    }
}

#[test]
fn huge_signed_exponents_are_arbitrary_length() {
    let positive = number("1e214748364812345678901234567890");
    let negative = number("10e-214748364812345678901234567891");
    assert_eq!(positive.exponent(), "214748364812345678901234567890");
    assert_eq!(negative.exponent(), "-214748364812345678901234567890");
}

#[test]
fn invalid_json_number_grammar_is_rejected() {
    for value in ["", "-", "01", ".1", "1.", "1e", "1e+", "+1", "--1"] {
        assert_eq!(
            CanonicalNumber::parse(value.as_bytes(), &RuntimeLimits::default()),
            Err(NumberError::InvalidGrammar),
            "{value}"
        );
    }
}

#[test]
fn digit_limits_are_explicit() {
    let limits = RuntimeLimits {
        max_number_digits: 2,
        ..RuntimeLimits::default()
    };
    assert_eq!(
        CanonicalNumber::parse(b"123", &limits),
        Err(NumberError::LimitExceeded {
            limit_name: "max_number_digits",
            limit_value: 2,
            requested: 3,
        })
    );
}
