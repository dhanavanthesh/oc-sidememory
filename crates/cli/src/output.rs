use oc_sidememory::sidememory::{GuideError, SemanticViolation};
use serde_json::{json, Value};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug)]
pub struct CliError {
    pub exit_code: i32,
    pub message: String,
    pub body: Value,
}

impl CliError {
    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(2, "invalid_input", message)
    }

    pub fn compile(message: impl Into<String>) -> Self {
        Self::new(3, "compilation_failure", message)
    }

    pub fn new(exit_code: i32, category: &str, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            exit_code,
            body: json!({
                "formatVersion": FORMAT_VERSION,
                "accepted": false,
                "category": category,
                "message": message,
            }),
            message,
        }
    }

    pub fn guide(error: GuideError, token_index: Option<usize>) -> Self {
        let (exit_code, category) = match &error {
            GuideError::StructuralRejection { .. } | GuideError::UnknownToken { .. } => {
                (4, "structural_rejection")
            }
            GuideError::SemanticRejection(_) => (5, "semantic_rejection"),
            GuideError::Resource(_) => (6, "resource_limit"),
            GuideError::InvalidLifecycle { .. }
            | GuideError::RollbackUnavailable { .. }
            | GuideError::Rollback(_) => (7, "lifecycle_or_rollback"),
            GuideError::InternalInvariant { .. } | GuideError::Poisoned { .. } => {
                (8, "internal_invariant")
            }
            _ => (8, "runtime_failure"),
        };
        let mut body = json!({
            "formatVersion": FORMAT_VERSION,
            "accepted": false,
            "category": category,
            "message": error.to_string(),
        });
        if let Some(index) = token_index {
            body["tokenIndex"] = json!(index);
        }
        if let GuideError::SemanticRejection(violation) = &error {
            body = semantic_body(violation, token_index);
        }
        Self {
            exit_code,
            message: error.to_string(),
            body,
        }
    }
}

pub fn semantic_body(violation: &SemanticViolation, token_index: Option<usize>) -> Value {
    let mut body = json!({"formatVersion": FORMAT_VERSION, "accepted": false});
    if let Some(index) = token_index {
        body["tokenIndex"] = json!(index);
    }
    match violation {
        SemanticViolation::DuplicateArrayItem {
            constraint,
            item_index,
            ..
        } => {
            body["category"] = json!("duplicate_array_item");
            body["constraint"] = json!(constraint.get());
            body["itemIndex"] = json!(item_index);
        }
        SemanticViolation::ContainsMinimumNotMet {
            constraint,
            processed,
            matched,
            lower,
            ..
        } => {
            body["category"] = json!("contains_minimum_not_met");
            body["constraint"] = json!(constraint.get());
            body["processed"] = json!(processed);
            body["matched"] = json!(matched);
            body["lower"] = json!(lower);
        }
        SemanticViolation::ContainsMaximumExceeded {
            constraint,
            item_index,
            processed,
            matched,
            upper,
            ..
        } => {
            body["category"] = json!("contains_maximum_exceeded");
            body["constraint"] = json!(constraint.get());
            body["itemIndex"] = json!(item_index);
            body["processed"] = json!(processed);
            body["matched"] = json!(matched);
            body["upper"] = json!(upper);
        }
        SemanticViolation::ContainsMinimumUnreachable {
            constraint,
            item_index,
            processed,
            matched,
            lower,
            ..
        } => {
            body["category"] = json!("contains_minimum_unreachable");
            body["constraint"] = json!(constraint.get());
            body["itemIndex"] = json!(item_index);
            body["processed"] = json!(processed);
            body["matched"] = json!(matched);
            body["lower"] = json!(lower);
        }
        SemanticViolation::MissingCapture {
            property, capture, ..
        } => relation(&mut body, "missing_capture", property, Some(capture), None),
        SemanticViolation::EqualityMismatch {
            property, capture, ..
        } => relation(
            &mut body,
            "equality_mismatch",
            property,
            Some(capture),
            None,
        ),
        SemanticViolation::InequalityMismatch {
            property, capture, ..
        } => relation(
            &mut body,
            "inequality_mismatch",
            property,
            Some(capture),
            None,
        ),
        SemanticViolation::ImportedMemberRequired {
            property,
            import_set,
            ..
        } => relation(
            &mut body,
            "imported_member_required",
            property,
            None,
            Some(import_set),
        ),
        SemanticViolation::ImportedMemberForbidden {
            property,
            import_set,
            ..
        } => relation(
            &mut body,
            "imported_member_forbidden",
            property,
            None,
            Some(import_set),
        ),
    }
    body
}

fn relation(
    body: &mut Value,
    category: &str,
    property: &str,
    capture: Option<&str>,
    import_set: Option<&str>,
) {
    body["category"] = json!(category);
    body["property"] = json!(property);
    if let Some(capture) = capture {
        body["capture"] = json!(capture);
    }
    if let Some(import_set) = import_set {
        body["importSet"] = json!(import_set);
    }
}

#[cfg(test)]
mod tests {
    use oc_sidememory::sidememory::{GuideError, ResourceError};

    use super::CliError;

    #[test]
    fn runtime_exit_categories_are_stable() {
        let resource = CliError::guide(
            GuideError::Resource(ResourceError::LimitExceeded {
                limit_name: "test",
                limit_value: 0,
                requested: 1,
            }),
            None,
        );
        assert_eq!(resource.exit_code, 6);
        let lifecycle = CliError::guide(
            GuideError::RollbackUnavailable {
                requested: 1,
                available: 0,
            },
            None,
        );
        assert_eq!(lifecycle.exit_code, 7);
        let invariant = CliError::guide(
            GuideError::InternalInvariant {
                context: "test".into(),
            },
            None,
        );
        assert_eq!(invariant.exit_code, 8);
    }
}
