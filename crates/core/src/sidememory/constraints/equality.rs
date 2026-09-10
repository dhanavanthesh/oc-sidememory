use crate::json_schema::extensions::{RelationOperator, RelationPlan, RelationSource};

use super::super::canonical::{CanonicalArena, EqualityScratch};
use super::super::guide::{GuideError, SemanticViolation};
use super::super::register::{RegisterKey, RegisterStore};
use super::super::{CanonicalId, FrameId};

pub(crate) fn evaluate(
    relation: &RelationPlan,
    registers: &RegisterStore,
    arena: &CanonicalArena,
    object: FrameId,
    value: CanonicalId,
    scratch: &mut EqualityScratch,
) -> Result<Option<SemanticViolation>, GuideError> {
    let RelationSource::Capture { register, name } = &relation.source else {
        return Err(GuideError::InternalInvariant {
            context: "equality relation does not reference a capture".into(),
        });
    };
    let Some(source) = registers.get(RegisterKey {
        object,
        register: *register,
    }) else {
        return Ok(Some(SemanticViolation::MissingCapture {
            object,
            property: relation.target_property.clone(),
            capture: name.clone(),
        }));
    };
    let equal = arena.equal_with_scratch(value, source, scratch)?;
    match (relation.operator, equal) {
        (RelationOperator::Equal, false) => Ok(Some(SemanticViolation::EqualityMismatch {
            object,
            property: relation.target_property.clone(),
            capture: name.clone(),
        })),
        (RelationOperator::NotEqual, true) => Ok(Some(SemanticViolation::InequalityMismatch {
            object,
            property: relation.target_property.clone(),
            capture: name.clone(),
        })),
        (RelationOperator::Equal | RelationOperator::NotEqual, _) => Ok(None),
        _ => Err(GuideError::InternalInvariant {
            context: "membership operator dispatched to equality handler".into(),
        }),
    }
}
