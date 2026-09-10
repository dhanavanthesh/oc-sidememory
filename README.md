<div align="center">

# OC-Sidememory

**Exact semantic memory for schema-constrained token generation in Rust and Python.**

[![PyPI](https://img.shields.io/pypi/v/oc-sidememory.svg)](https://pypi.org/project/oc-sidememory/)
[![crates.io](https://img.shields.io/crates/v/oc-sidememory.svg)](https://crates.io/crates/oc-sidememory)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

</div>

OC-Sidememory combines a byte-level DFA with transactional semantic state. The DFA checks JSON
structure. The semantic guide tracks completed values, contains counters, captures, and immutable
membership sets.

For guide state $\sigma$, token $x$, DFA decision $D$, and semantic decision $S$:

```math
A(\sigma,x)=D(\sigma,x)\land S(\sigma,x)
```

The semantic layer can remove structurally valid candidates, but it cannot add new candidates:

```math
M_{OC}=M_{DFA}\land\neg M_{deny}
```

## Features

| Capability | Behavior |
| --- | --- |
| `uniqueItems` | Rejects an exact duplicate in the current array instance |
| `contains` | Enforces `minContains` and `maxContains` over completed direct items |
| Captures | Compares a later property with an earlier property in declared order |
| Imported membership | Checks values against immutable, versioned sets |
| Exact equality | Handles numbers, decoded strings, arrays, and order-independent objects |
| Transactions | Makes probes pure and rejected advances atomic |
| Rollback | Restores bounded token checkpoints, including EOS |

Unsupported schema combinations fail during compilation. See the
[supported profile](docs/supported-profile.md) and [limitations](docs/limitations.md).

## Installation

Python:

```bash
pip install oc-sidememory
```

Rust:

```bash
cargo add oc-sidememory
```

From a checkout:

```bash
uv sync
uv run maturin develop --release
```

## Python example

Build the vocabulary from a normal Transformers tokenizer. Compile the schema once, then create one
guide for each generation sequence.

```python
import json

import oc_sidememory
from transformers import AutoTokenizer


tokenizer = AutoTokenizer.from_pretrained("openai-community/gpt2")
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

valid = tokenizer.encode('["red","green","blue"]', add_special_tokens=False)
duplicate = tokenizer.encode('["red","red","blue"]', add_special_tokens=False)

assert guide.accepts_tokens(valid, finish=True)
assert not guide.accepts_tokens(duplicate, finish=True)
```

`accepts_tokens`, `probe`, `advance`, and mask construction use the same Rust transition routine.
Python does not contain a second semantic validator.

## How uniqueness changes a mask

After completing `"red"` in a unique array:

| Layer | Next item candidates |
| --- | --- |
| Structural DFA | `"red"`, `"green"`, `"blue"` |
| Semantic guide | `"green"`, `"blue"` |

The duplicate is removed before sampling. A direct attempt to advance it is rejected by the same
semantic transition.

## Exact guarantees

Let $C(v)$ be the canonical value of $v$, and let $H_a$ be the history of live array instance $a$.
Uniqueness admits $v$ exactly when:

```math
U(v)\iff C(v)\notin H_a
```

For predicate $P$, contains counts matching completed direct items:

```math
c_a=\sum_{i=1}^{n}\mathbf{1}[P(v_i)],\qquad l\le c_a\le u
```

Equality and imported membership use canonical values, not raw JSON spelling or fingerprints:

```math
E(v,r)\iff C(v)=C(r)
```

```math
I(v,S)\iff C(v)\in S
```

A probe leaves logical state unchanged:

```math
P_x(\sigma)=\sigma
```

Rolling back $k$ committed token transitions restores the earlier logical state:

```math
R_k(T_{x_k}(\cdots T_{x_1}(\sigma)))=\sigma
```

Revision counters and retained allocation capacity are not part of logical state equality.

## Command-line interface

```bash
oc-sidememory capabilities
oc-sidememory compile --schema schema.json --vocabulary vocabulary.json
oc-sidememory check --schema schema.json --vocabulary vocabulary.json \
  --tokens tokens.json --finish
```

Commands emit versioned JSON. Vocabulary files store token bytes as base64, so token data does not
need to be UTF-8. See the [CLI reference](docs/cli.md).

## Performance

Mask latency depends on candidate count, token bytes, emitted events, predicate work, equality work,
and current semantic history. Recorded release-mode measurements and their environment are in
[docs/performance.md](docs/performance.md). They are engineering measurements, not universal
throughput claims.

## Documentation

| Guide | Subject |
| --- | --- |
| [Architecture](docs/architecture.md) | Ownership, transitions, journals, and rollback |
| [Python API](docs/python-api.md) | Compilation, guides, imports, masks, and exceptions |
| [Rust API](docs/rust-api.md) | Core construction, lifecycle, and error types |
| [Extensions v1](docs/extensions-v1.md) | Ordered captures and relations |
| [Imported memory](docs/imported-memory.md) | Identity, canonicalization, and lifetime |
| [Supported profile](docs/supported-profile.md) | Implemented schema surface |
| [Limitations](docs/limitations.md) | Explicit boundaries and non-goals |

The [example ladder](examples/README.md) covers uniqueness, contains, capture equality, imported
membership, and composed rollback using public APIs.

## Development

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
uv run pytest -q
```

Deterministic correctness tests require no model or network. See [CONTRIBUTING.md](CONTRIBUTING.md)
and [SECURITY.md](SECURITY.md).

## Provenance and license

OC-Sidememory is independently developed from an Apache-2.0 `outlines-core` source import and is
not endorsed by its upstream authors. See [PROVENANCE.md](PROVENANCE.md),
[MODIFICATIONS.md](MODIFICATIONS.md), [NOTICE](NOTICE), and [LICENSE](LICENSE).
