# Limitations

1. The compiler supports a checked JSON Schema subset, not the full specification.
2. Contains predicates exclude references, combinators, nested contains, conditionals, and unevaluated assertions.
3. Captures apply to fixed-order direct properties in one object instance.
4. Imported memory is immutable.
5. Live semantic guides cannot be serialized.
6. Rollback and semantic work are bounded by configured limits.
7. Mask cost scales with candidates, token bytes, events, predicate work, and exact comparisons.
8. Transformers examples require a cached pinned GPT-2 snapshot. They are demonstrations, not correctness oracles.
