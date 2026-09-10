mod common;

use std::hash::{BuildHasher, Hasher};
use std::sync::Arc;

use oc_sidememory::json_schema::extensions::ExtensionPlanV1;
use oc_sidememory::sidememory::{
    Guide, GuideError, GuideOptions, ImportedMemory, SequenceDecision,
};
use oc_sidememory::{compile_schema, CompileOptions};

fn compiled(operator: &str) -> Arc<oc_sidememory::json_schema::compile::CompiledSchema> {
    let (vocabulary, width) = common::ascii_vocabulary();
    let schema = r#"{
        "type":"object",
        "properties":{"customer_id":{"type":"number"}},
        "required":["customer_id"],
        "additionalProperties":false
    }"#;
    let extension = format!(
        r#"{{"version":1,"objects":[{{"schemaPath":"$","propertyOrder":["customer_id"],"captures":[],"relations":[{{"targetProperty":"customer_id","operator":"{operator}","import":"valid"}}]}}]}}"#
    );
    let options = CompileOptions {
        extension_plan: Some(ExtensionPlanV1::from_json(&extension).unwrap()),
        ..CompileOptions::default()
    };
    Arc::new(compile_schema(schema.as_bytes(), &vocabulary, width, &options).unwrap())
}

fn accepts(guide: &mut Guide, document: &str) -> bool {
    let tokens: Vec<_> = document.bytes().map(u32::from).collect();
    guide.probe_sequence(&tokens, true).unwrap() == SequenceDecision::Allow
}

#[test]
fn imports_are_identity_versioned_and_required_at_construction() {
    let imports = ImportedMemory::from_json("catalog", "2026-09-10", r#"{"valid":[1]}"#).unwrap();
    assert_eq!(imports.identity(), "catalog");
    assert_eq!(imports.version(), "2026-09-10");
    assert_eq!(imports.set_count(), 1);
    assert!(matches!(
        Guide::new(compiled("memberOf"), GuideOptions::default()),
        Err(GuideError::MissingImport { .. })
    ));
}

#[test]
fn member_and_not_member_use_exact_cross_arena_equality() {
    let imports = Arc::new(
        ImportedMemory::from_json("catalog", "v1", r#"{"valid":[1,9007199254740993]}"#).unwrap(),
    );
    let mut member = Guide::new_with_imports(
        compiled("memberOf"),
        GuideOptions::default(),
        imports.clone(),
    )
    .unwrap();
    assert!(accepts(&mut member, r#"{"customer_id":1.0}"#));
    assert!(accepts(&mut member, r#"{"customer_id":9007199254740993}"#));
    assert!(!accepts(&mut member, r#"{"customer_id":9007199254740992}"#));

    let mut forbidden =
        Guide::new_with_imports(compiled("notMemberOf"), GuideOptions::default(), imports).unwrap();
    assert!(!accepts(&mut forbidden, r#"{"customer_id":1e0}"#));
    assert!(accepts(&mut forbidden, r#"{"customer_id":2}"#));
}

#[derive(Clone)]
struct ZeroBuildHasher;

struct ZeroHasher;

impl BuildHasher for ZeroBuildHasher {
    type Hasher = ZeroHasher;

    fn build_hasher(&self) -> Self::Hasher {
        ZeroHasher
    }
}

impl Hasher for ZeroHasher {
    fn finish(&self) -> u64 {
        0
    }

    fn write(&mut self, _bytes: &[u8]) {}
}

#[test]
fn forced_cross_arena_collisions_still_require_exact_equality() {
    use oc_sidememory::sidememory::{CanonicalArena, CanonicalSet, RuntimeLimits};

    let mut stored = CanonicalArena::new(RuntimeLimits::default());
    let one = stored.add_string(b"one").unwrap();
    let two = stored.add_string(b"two").unwrap();
    let mut set = CanonicalSet::with_hash_builder(ZeroBuildHasher);
    set.insert(&stored, one).unwrap();
    set.insert(&stored, two).unwrap();

    let mut candidate = CanonicalArena::new(RuntimeLimits::default());
    let two_alias = candidate.add_string(b"two").unwrap();
    let three = candidate.add_string(b"three").unwrap();
    assert!(set.contains_across(&stored, &candidate, two_alias).unwrap());
    assert!(!set.contains_across(&stored, &candidate, three).unwrap());
}

#[test]
fn composite_imported_objects_use_order_independent_exact_equality() {
    let (vocabulary, width) = common::ascii_vocabulary();
    let schema = br#"{
        "type":"object",
        "properties":{"payload":{"type":"object","properties":{"a":{"type":"number"},"b":{"type":"number"}},"required":["a","b"],"additionalProperties":false}},
        "required":["payload"],
        "additionalProperties":false
    }"#;
    let extension = ExtensionPlanV1::from_json(
        r#"{"version":1,"objects":[{"schemaPath":"$","propertyOrder":["payload"],"relations":[{"targetProperty":"payload","operator":"memberOf","import":"valid"}]}]}"#,
    )
    .unwrap();
    let options = CompileOptions {
        extension_plan: Some(extension),
        ..CompileOptions::default()
    };
    let compiled = Arc::new(compile_schema(schema, &vocabulary, width, &options).unwrap());
    let imports = Arc::new(
        ImportedMemory::from_json("objects", "v1", r#"{"valid":[{"a":1,"b":2}]}"#).unwrap(),
    );
    let mut guide = Guide::new_with_imports(compiled, GuideOptions::default(), imports).unwrap();
    assert!(accepts(&mut guide, r#"{"payload":{"b":2.0,"a":1e0}}"#));
    assert!(!accepts(&mut guide, r#"{"payload":{"a":1,"b":3}}"#));
}
