# Examples

The Python examples use a pinned Transformers model and an already loaded fast tokenizer. They set offline mode and require that snapshot in the local Hugging Face cache, so they never download silently.

Python examples progress from `uniqueItems` and `contains` through capture equality, imported membership, and composed rollback. Run them from an environment containing the installed extension.

The Rust examples form a standalone Cargo project under `rust/` and exercise the same public core API with deterministic byte vocabularies.
