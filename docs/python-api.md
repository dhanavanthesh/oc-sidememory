# Python API

`compile_schema(schema_json, vocabulary, model_width, *, extensions_json=None)` returns `CompiledSchema`. `Vocabulary.from_transformers(tokenizer)` accepts a loaded fast tokenizer.

`SidememoryGuide(compiled, max_rollback=32, *, imports=None)` provides `get_tokens`, `get_mask`, `fill_mask`, `probe`, `advance`, `rollback`, `reset`, `is_accepting`, `is_terminated`, `get_allowed_rollback`, and `accepts_tokens`.

`ImportedMemory.from_json(identity, version, json_text)` builds immutable sets in Rust. Error classes distinguish compile, structural, semantic, resource, lifecycle, rollback, and invariant failures.

`Guide` is the legacy DFA-only compatibility class. Live `SidememoryGuide` serialization is unsupported.
