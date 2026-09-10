# OC-Sidememory

OC-Sidememory is a Rust and Python constrained-decoding engine that refines structural token masks with exact, rollback-safe JSON semantic constraints.

## Features

- Checked compilation of a documented JSON Schema Draft 2020-12 subset.
- Generation-time `uniqueItems`, `contains`, `minContains`, and `maxContains`.
- Versioned opt-in capture equality and immutable imported membership.
- Exact JSON equality for numbers, strings, arrays, and objects.
- Atomic probe and advance, EOS, reset, and bounded token rollback.
- A Rust semantic authority, thin Python adapter, and JSON CLI.

Unsupported combinations fail compilation. See the [supported profile](docs/supported-profile.md) and [limitations](docs/limitations.md).

## Installation

The project is not published to crates.io or PyPI. Build from a checkout:

```console
uv sync
uv run maturin develop --release
```

For Rust, use a path dependency until publication is authorized:

```toml
[dependencies]
oc-sidememory = { path = "crates/core", default-features = false }
```

## Python quick start

```python
import json
import oc_sidememory
from transformers import AutoTokenizer

tokenizer = AutoTokenizer.from_pretrained("gpt2", local_files_only=True)
vocabulary = oc_sidememory.Vocabulary.from_transformers(tokenizer)
schema = {
    "type": "array",
    "items": {"type": "string", "enum": ["red", "green", "blue"]},
    "minItems": 3,
    "maxItems": 3,
    "uniqueItems": True,
}
compiled = oc_sidememory.compile_schema(
    json.dumps(schema), vocabulary, tokenizer.vocab_size
)
guide = oc_sidememory.SidememoryGuide(compiled, max_rollback=32)
tokens = tokenizer.encode('["red","red","blue"]', add_special_tokens=False)
assert not guide.accepts_tokens(tokens, finish=True)
```

The public Python examples pin GPT-2 revision `607a30d783dfa663caf39e06633721c8d4cfcd7e`, force offline loading, and never download silently. The revision is a public model version, not a credential.

## Rust example

Buildable programs using the public API are in [examples](examples/README.md). They use deterministic byte vocabularies and require no network or model.

## Architecture and guarantees

The DFA remains authoritative for structure. Semantics may only remove candidates:

```math
\operatorname{Allowed}(\sigma,x)=
\operatorname{DFAAllowed}(q,x)\land
\operatorname{SemanticAllowed}(\sigma,x)
```

```math
M_{\mathrm{OC}}=M_{\mathrm{DFA}}\land\neg M_{\mathrm{deny}}
```

Probe, mask, direct advance, sequence checking, and EOS use one byte-by-byte Rust transition. An outer transaction covers the cursor, router, canonical arena, histories, counters, registers, lifecycle, and trace.

```math
\operatorname{admit}(v)\iff\operatorname{canon}(v)\notin H_a
```

```math
c_a=\sum_{i=1}^{n}\mathbf{1}[\operatorname{Validate}(v_i,P)],\qquad l\le c_a\le u
```

```math
\operatorname{StateAfterProbe}(\sigma,x)=\sigma
```

Monotonic revision counters and retained allocation capacity are excluded from logical state equality. See [architecture](docs/architecture.md), [Rust API](docs/rust-api.md), and [Python API](docs/python-api.md).

## Extensions and imports

Cross-field relations are an explicit version 1 generation extension, not standard JSON Schema. An extension-bearing object declares property order and may capture an earlier direct property for equality or inequality. Membership relations query immutable, identity-versioned imported sets. See [extensions](docs/extensions-v1.md) and [imported memory](docs/imported-memory.md).

## CLI

The `oc-sidememory` binary provides `compile`, `tokens`, `mask`, `check`, `inspect-imports`, and `capabilities`. Inputs and outputs are versioned JSON; token bytes use base64. See the [CLI reference](docs/cli.md).

## Performance

The semantic engine uses journaled mutation instead of cloning complete guide state. Initial local measurements and denominators are in [performance](docs/performance.md). A candidate-buffer experiment was removed after it regressed representative p95 mask latency. No universal speedup or zero-overhead claim is made.

## Development

```console
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
uv run pytest -q
```

Deterministic correctness tests require no model or network. Cached-model examples are demonstrations, not correctness oracles.

## Provenance and license

OC-Sidememory is independently developed from an Apache-2.0 outlines-core source import and is not endorsed by that project. See [provenance](PROVENANCE.md), [modifications](MODIFICATIONS.md), [NOTICE](NOTICE), and [LICENSE](LICENSE).
