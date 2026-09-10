use crate::json_schema::extensions::{RelationOperator, RelationPlan, RelationSource};

use super::super::canonical::{CanonicalArena, EqualityScratch};
use super::super::guide::{GuideError, SemanticViolation};
use super::super::imports::ImportedMemory;
use super::super::{CanonicalId, FrameId};

pub(crate) fn evaluate(
    relation: &RelationPlan,
    imports: &ImportedMemory,
    arena: &CanonicalArena,
    object: FrameId,
    value: CanonicalId,
    scratch: &mut EqualityScratch,
) -> Result<Option<SemanticViolation>, GuideError> {
    let RelationSource::Import { name } = &relation.source else {
        return Err(GuideError::InternalInvariant {
            context: "membership relation does not reference an import".into(),
        });
    };
    let member = imports.contains_with_scratch(name, arena, value, scratch)?;
    match (relation.operator, member) {
        (RelationOperator::MemberOf, false) => {
            Ok(Some(SemanticViolation::ImportedMemberRequired {
                object,
                property: relation.target_property.clone(),
                import_set: name.clone(),
            }))
        }
        (RelationOperator::NotMemberOf, true) => {
            Ok(Some(SemanticViolation::ImportedMemberForbidden {
                object,
                property: relation.target_property.clone(),
                import_set: name.clone(),
            }))
        }
        (RelationOperator::MemberOf | RelationOperator::NotMemberOf, _) => Ok(None),
        _ => Err(GuideError::InternalInvariant {
            context: "equality operator dispatched to membership handler".into(),
        }),
    }
}
