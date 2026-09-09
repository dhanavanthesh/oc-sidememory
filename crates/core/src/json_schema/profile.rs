#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dialect {
    Draft202012,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnknownKeywordPolicy {
    Reject,
    AllowKnownAnnotations,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DuplicateKeyPolicy {
    Reject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnicodePolicy {
    ScalarValues,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedProfile {
    pub dialect: Dialect,
    pub max_schema_depth: u32,
    pub allow_annotations: bool,
    pub unknown_keyword_policy: UnknownKeywordPolicy,
    pub duplicate_object_key_policy: DuplicateKeyPolicy,
    pub unicode_policy: UnicodePolicy,
}

impl Default for CheckedProfile {
    fn default() -> Self {
        Self {
            dialect: Dialect::Draft202012,
            max_schema_depth: 128,
            allow_annotations: true,
            unknown_keyword_policy: UnknownKeywordPolicy::AllowKnownAnnotations,
            duplicate_object_key_policy: DuplicateKeyPolicy::Reject,
            unicode_policy: UnicodePolicy::ScalarValues,
        }
    }
}
