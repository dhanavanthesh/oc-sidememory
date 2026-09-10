# Imported memory

Imported memory is an immutable snapshot of named JSON arrays with caller-supplied identity and version. Rust canonicalizes values and uses fingerprint buckets followed by exact cross-arena equality.

Guides share imports through `Arc`. Rollback never mutates imports, reset retains them, and replacement requires a new guide. Identity and version are descriptive metadata, not cryptographic verification.
