use std::sync::Arc;

use oc_sidememory::json_schema::extensions::ExtensionPlanV1;
use oc_sidememory::sidememory::{
    FailurePoint, Guide, GuideError, GuideOptions, HistoryError, Lifecycle, ProbeDecision,
    ResourceError,
};
use oc_sidememory::{compile_schema, CompileOptions, Vocabulary};

const OPEN: u32 = 0;
const A: u32 = 1;
const COMMA: u32 = 2;
const B: u32 = 3;
const CLOSE: u32 = 4;
const EOS: u32 = 9;

fn guide(max_history_items: usize) -> Guide {
    let mut options = GuideOptions::default();
    options.limits.max_history_items = max_history_items;
    guide_with_options(options)
}

fn guide_with_limits(max_history_items: usize, max_equality_checks_per_mask: usize) -> Guide {
    let mut options = GuideOptions::default();
    options.limits.max_history_items = max_history_items;
    options.limits.max_equality_checks_per_mask = max_equality_checks_per_mask;
    guide_with_options(options)
}

fn guide_with_options(options: GuideOptions) -> Guide {
    let mut vocabulary = Vocabulary::new(EOS);
    for (bytes, token) in [
        (b"[".as_slice(), OPEN),
        (br#""a""#.as_slice(), A),
        (b",".as_slice(), COMMA),
        (br#""b""#.as_slice(), B),
        (b"]".as_slice(), CLOSE),
    ] {
        vocabulary.try_insert(bytes.to_vec(), token).unwrap();
    }
    let compiled = compile_schema(
        br#"{
            "type":"array",
            "items":{"type":"string"},
            "maxItems":2,
            "uniqueItems":true
        }"#,
        &vocabulary,
        usize::try_from(EOS).unwrap() + 1,
        &CompileOptions::default(),
    )
    .unwrap();
    Guide::new(Arc::new(compiled), options).unwrap()
}

#[test]
fn history_limit_error_restores_candidate_and_zeroes_mask() {
    let mut guide = guide(1);
    guide.advance(OPEN).unwrap();
    guide.advance(A).unwrap();
    guide.advance(COMMA).unwrap();
    let state = guide.state_id();
    let rollback = guide.rollback_available();
    let mut mask = [u32::MAX];

    let error = guide.write_mask(&mut mask).unwrap_err();
    assert!(matches!(
        error,
        GuideError::History(HistoryError::Resource(ResourceError::LimitExceeded {
            limit_name: "max_history_items",
            ..
        }))
    ));
    assert_eq!(mask, [0]);
    assert_eq!(guide.state_id(), state);
    assert_eq!(guide.rollback_available(), rollback);
    assert!(matches!(guide.probe(A).unwrap(), ProbeDecision::Deny(_)));
    guide.reset().unwrap();
    guide.advance(OPEN).unwrap();
    guide.advance(CLOSE).unwrap();
    guide.advance(EOS).unwrap();
}

#[test]
fn equality_budget_error_restores_state_and_zeroes_mask() {
    let mut guide = guide_with_limits(8, 0);
    guide.advance(OPEN).unwrap();
    guide.advance(A).unwrap();
    guide.advance(COMMA).unwrap();
    let state = guide.state_id();
    let rollback = guide.rollback_available();
    let mut mask = [u32::MAX];

    let error = guide.write_mask(&mut mask).unwrap_err();
    assert!(matches!(
        error,
        GuideError::History(HistoryError::Resource(ResourceError::LimitExceeded {
            limit_name: "max_equality_checks_per_mask",
            ..
        }))
    ));
    assert_eq!(mask, [0]);
    assert_eq!(guide.state_id(), state);
    assert_eq!(guide.rollback_available(), rollback);
    assert!(matches!(guide.probe(A).unwrap(), ProbeDecision::Deny(_)));
}

#[test]
fn retained_rollback_limit_rejects_commit_without_changing_state() {
    let mut options = GuideOptions::default();
    options.limits.max_retained_rollback_bytes = 0;
    let mut guide = guide_with_options(options);
    let state = guide.state_id();

    let error = guide.advance(OPEN).unwrap_err();
    assert!(matches!(
        error,
        GuideError::Resource(ResourceError::LimitExceeded {
            limit_name: "max_retained_rollback_bytes",
            ..
        })
    ));
    assert_eq!(guide.state_id(), state);
    assert_eq!(guide.rollback_available(), 0);
    assert_eq!(guide.retained_rollback_bytes(), 0);
    assert_eq!(guide.probe(OPEN).unwrap(), ProbeDecision::Allow);
}

#[test]
fn opening_failures_restore_complete_logical_state() {
    for point in [
        FailurePoint::BeforeCursorMutation,
        FailurePoint::AfterCursorMutation,
        FailurePoint::AfterFrameAllocation,
        FailurePoint::AfterHistoryCreation,
        FailurePoint::BeforeCheckpointInsert,
    ] {
        let mut guide = guide(8);
        let before = guide.debug_snapshot();
        guide.inject_failure_once(point);

        assert!(guide.advance(OPEN).is_err(), "failure point {point:?}");
        assert_eq!(guide.debug_snapshot(), before, "failure point {point:?}");
        assert_eq!(guide.probe(OPEN).unwrap(), ProbeDecision::Allow);
    }
}

#[test]
fn history_insert_failures_restore_complete_logical_state() {
    for point in [
        FailurePoint::BeforeHistoryInsert,
        FailurePoint::AfterHistoryInsert,
    ] {
        let mut guide = guide(8);
        guide.advance(OPEN).unwrap();
        let before = guide.debug_snapshot();
        guide.inject_failure_once(point);

        assert!(guide.advance(A).is_err(), "failure point {point:?}");
        assert_eq!(guide.debug_snapshot(), before, "failure point {point:?}");
        assert_eq!(guide.probe(A).unwrap(), ProbeDecision::Allow);
    }
}

#[test]
fn array_close_failures_restore_history_and_router() {
    for point in [
        FailurePoint::BeforeArrayClose,
        FailurePoint::AfterHistoryClose,
    ] {
        let mut guide = guide(8);
        guide.advance(OPEN).unwrap();
        guide.advance(A).unwrap();
        let before = guide.debug_snapshot();
        guide.inject_failure_once(point);

        assert!(guide.advance(CLOSE).is_err(), "failure point {point:?}");
        assert_eq!(guide.debug_snapshot(), before, "failure point {point:?}");
        assert_eq!(guide.probe(CLOSE).unwrap(), ProbeDecision::Allow);
    }
}

#[test]
fn mask_failure_zeroes_output_and_preserves_state() {
    let mut guide = guide(8);
    let before = guide.debug_snapshot();
    let mut mask = [u32::MAX];
    guide.inject_failure_once(FailurePoint::DuringMask);

    assert!(guide.write_mask(&mut mask).is_err());
    assert_eq!(mask, [0]);
    assert_eq!(guide.debug_snapshot(), before);
    assert_eq!(guide.probe(OPEN).unwrap(), ProbeDecision::Allow);
}

#[test]
fn restoration_failure_poisons_until_reset() {
    let mut guide = guide(8);
    guide.inject_failure_once(FailurePoint::DuringRestore);

    assert!(matches!(
        guide.probe(OPEN),
        Err(GuideError::Poisoned { .. })
    ));
    assert!(matches!(
        guide.probe(OPEN),
        Err(GuideError::InvalidLifecycle {
            state: Lifecycle::Poisoned,
            ..
        })
    ));
    assert!(matches!(
        guide.rollback(0),
        Err(GuideError::Poisoned { .. })
    ));

    guide.reset().unwrap();
    assert_eq!(guide.probe(OPEN).unwrap(), ProbeDecision::Allow);
}

fn whole_document_guide(schema: &[u8], document: &[u8], options: CompileOptions) -> Guide {
    whole_document_guide_with_options(schema, document, options, GuideOptions::default())
}

fn whole_document_guide_with_options(
    schema: &[u8],
    document: &[u8],
    options: CompileOptions,
    guide_options: GuideOptions,
) -> Guide {
    const DOCUMENT: u32 = 128;
    const DOCUMENT_EOS: u32 = 129;
    let mut vocabulary = Vocabulary::new(DOCUMENT_EOS);
    for byte in 0_u8..=127 {
        vocabulary.try_insert(vec![byte], u32::from(byte)).unwrap();
    }
    vocabulary.try_insert(document.to_vec(), DOCUMENT).unwrap();
    let compiled = compile_schema(schema, &vocabulary, 130, &options).unwrap();
    Guide::new(Arc::new(compiled), guide_options).unwrap()
}

#[test]
fn contains_failures_restore_counter_state() {
    let schema = br#"{
        "type":"array",
        "items":{"type":"string"},
        "maxItems":1,
        "contains":{"const":"a"}
    }"#;
    for point in [
        FailurePoint::AfterCounterCreation,
        FailurePoint::BeforeContainsEvaluation,
        FailurePoint::AfterCounterUpdate,
    ] {
        let mut guide = whole_document_guide(schema, br#"["a"]"#, CompileOptions::default());
        let before = guide.debug_snapshot();
        guide.inject_failure_once(point);
        assert!(guide.advance(128).is_err(), "failure point {point:?}");
        assert_eq!(guide.debug_snapshot(), before, "failure point {point:?}");
        assert_eq!(guide.probe(128).unwrap(), ProbeDecision::Allow);
    }
}

#[test]
fn predicate_equality_budget_restores_state_and_zeroes_mask() {
    let schema = br#"{
        "type":"array",
        "items":{"type":"array","items":{"type":"integer"},"maxItems":2},
        "maxItems":1,
        "contains":{
            "type":"array",
            "items":{"type":"integer"},
            "maxItems":2,
            "uniqueItems":true
        }
    }"#;
    let mut options = GuideOptions::default();
    options.limits.max_predicate_equality_checks = 0;
    let mut guide = whole_document_guide_with_options(
        schema,
        br#"[[1,1]]"#,
        CompileOptions::default(),
        options,
    );
    let before = guide.debug_snapshot();
    let mut mask = [u32::MAX; 5];

    let error = guide.write_mask(&mut mask).unwrap_err();
    assert!(matches!(
        error,
        GuideError::Resource(ResourceError::LimitExceeded {
            limit_name: "max_predicate_equality_checks",
            ..
        })
    ));
    assert_eq!(mask, [0; 5]);
    assert_eq!(guide.debug_snapshot(), before);
    assert_eq!(guide.probe(u32::from(b'[')).unwrap(), ProbeDecision::Allow);
}

#[test]
fn capture_and_relation_failures_restore_register_state() {
    let schema = br#"{
        "type":"object",
        "properties":{"source":{"type":"string"},"target":{"type":"string"}},
        "required":["source","target"],
        "additionalProperties":false
    }"#;
    let extension = ExtensionPlanV1::from_json(
        r#"{"version":1,"objects":[{"schemaPath":"$","propertyOrder":["source","target"],"captures":[{"name":"saved","sourceProperty":"source"}],"relations":[{"targetProperty":"target","operator":"equal","capture":"saved"}]}]}"#,
    )
    .unwrap();
    for point in [
        FailurePoint::AfterRegisterScopeCreation,
        FailurePoint::AfterCapturePublication,
        FailurePoint::BeforeRelationEvaluation,
        FailurePoint::AfterRegisterScopeClose,
    ] {
        let options = CompileOptions {
            extension_plan: Some(extension.clone()),
            ..CompileOptions::default()
        };
        let mut guide = whole_document_guide(schema, br#"{"source":"x","target":"x"}"#, options);
        let before = guide.debug_snapshot();
        guide.inject_failure_once(point);
        assert!(guide.advance(128).is_err(), "failure point {point:?}");
        assert_eq!(guide.debug_snapshot(), before, "failure point {point:?}");
        assert_eq!(guide.probe(128).unwrap(), ProbeDecision::Allow);
    }
}
