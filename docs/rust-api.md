# Rust API

The crate root exports `compile_schema`, `CompileOptions`, `CompiledSchema`, `Vocabulary`, `Guide`, `GuideOptions`, `ImportedMemory`, `ExtensionPlanV1`, and `RelationOperator`.

Compile with `compile_schema(schema, vocabulary, model_width, options)`. Construct a guide with `Guide::new` or `Guide::new_with_imports`. Runtime methods include `allowed_tokens`, `write_mask`, `probe`, `advance`, `rollback`, `reset`, `is_accepting`, `is_terminated`, and `probe_sequence`.

`ImportedMemory::from_json(identity, version, json)` canonicalizes immutable sets. `GuideError` keeps structural, semantic, resource, lifecycle, rollback, and invariant failures distinct.
