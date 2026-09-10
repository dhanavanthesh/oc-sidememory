# Contributing

Keep changes focused and preserve the single Rust semantic transition authority. Add tests for observable behavior. Do not introduce Python-side semantic validation or a separate mask evaluator.

Before proposing a change, run formatting, strict Clippy, Rust tests, and Python tests. Performance changes need recorded A/B results, identical semantic digests, and bounded memory impact.
