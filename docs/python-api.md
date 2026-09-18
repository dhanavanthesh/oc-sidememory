# Python API

## Transformers facade

`from_transformers(model, tokenizer)` builds a reusable `Runtime`. It supports decoder-only CPU
models and obtains the mask width from the model's real output embeddings when available.

`Generator(runtime, output, *, extensions=None, imports=None, max_rollback=32)` compiles once and
creates an independent `SidememoryGuide` for every generated sequence. `output` may be a JSON
Schema string, a schema dictionary, or a Pydantic-compatible type. `extensions` accepts a mapping,
JSON string, or `Path`. An imports mapping must contain exactly `identity`, `version`, and `data`.

Calling a generator accepts a raw prompt, chat messages, or a homogeneous batch of either. The
result is parsed JSON, or an instance of the supplied Pydantic-compatible type. Greedy decoding and
sampling are supported. Beam search, multiple return sequences, assisted generation, prompt lookup,
and per-call EOS/padding overrides are rejected before generation begins.

```python
runtime = oc_sidememory.from_transformers(model, tokenizer)
generate = oc_sidememory.Generator(runtime, schema)
result = generate(messages, max_new_tokens=128)
```

`GenerationError` reports an empty mask, incomplete JSON at the token limit, or an unexpected model
output shape.

## Low-level guide API

`compile_schema(schema_json, vocabulary, model_width, *, extensions_json=None)` returns `CompiledSchema`. `Vocabulary.from_transformers(tokenizer)` accepts a loaded fast tokenizer.

`SidememoryGuide(compiled, max_rollback=32, *, imports=None)` provides `get_tokens`, `get_mask`, `fill_mask`, `probe`, `advance`, `rollback`, `reset`, `is_accepting`, `is_terminated`, `get_allowed_rollback`, and `accepts_tokens`.

`ImportedMemory.from_json(identity, version, json_text)` builds immutable sets in Rust. Error classes distinguish compile, structural, semantic, resource, lifecycle, rollback, and invariant failures.

`Guide` is the legacy DFA-only compatibility class. Live `SidememoryGuide` serialization is unsupported.
