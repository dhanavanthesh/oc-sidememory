# Examples

`python/00_travel_itinerary.py` is the complete README walkthrough. It downloads a pinned
Qwen 0.5B snapshot on its first run and generates a multi-field JSON object from a prompt.

The semantic ladder from `01_unique_items.py` through `05_composed_rollback.py` uses a pinned cached
model. Those scripts run offline and never download silently.

The Python examples progress from prompt-to-object generation and `uniqueItems` through `contains`,
capture equality, imported membership, and composed rollback. Run them from an environment
containing the installed extension, Transformers, and PyTorch.

`python/06_customer_action_plan.py` closes the ladder with a Qwen generation that composes
`uniqueItems`, `contains`, imported membership, and capture equality in one customer workflow.

The Rust examples form a standalone Cargo project under `rust/` and exercise the same public core API with deterministic byte vocabularies.
