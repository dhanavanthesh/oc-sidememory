<!-- Portions derived from outlines-core and modified by OC-Sidememory contributors. -->
<!-- See PROVENANCE.md and MODIFICATIONS.md. -->

<div align="center">

# OC-Sidememory

**Make an LLM remember semantic rules while it generates structured output.**

Exact semantic memory for schema-constrained token generation in Rust and Python.

[![PyPI](https://img.shields.io/pypi/v/oc-sidememory.svg)](https://pypi.org/project/oc-sidememory/)
[![crates.io](https://img.shields.io/crates/v/oc-sidememory.svg)](https://crates.io/crates/oc-sidememory)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

</div>

[Why](#why-oc-sidememory) · [Decision rule](#token-acceptance-rule) ·
[Start here](#start-here-prompt-to-guaranteed-json) · [Stateful rules](#compose-stateful-rules) ·
[Patterns](#patterns-you-can-ship) · [Documentation](#documentation)

OC-Sidememory blocks tokens that violate stateful rules: duplicates, counted predicates, runtime
membership, and cross-field relations.

## Why OC-Sidememory?

Structure alone can still repeat an action:

```json
{"actions":["search","search","search"]}
```

With semantic memory:

```json
{"actions":["search","compare","answer"]}
```

## Token acceptance rule

For guide state $\sigma$, token $x$, DFA decision $D$, and semantic decision $S$:

```math
A(\sigma,x)=D(\sigma,x)\land S(\sigma,x)
```

The semantic layer can remove structurally valid candidates, but it cannot add new candidates:

```math
M_{OC}=M_{DFA}\land\neg M_{deny}
```

## What it enforces

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

## How uniqueness changes a mask

After completing `"red"` in a unique array:

| Layer | Next item candidates |
| --- | --- |
| Structural DFA | `"red"`, `"green"`, `"blue"` |
| Semantic guide | `"green"`, `"blue"` |

## Exact guarantees

| Symbol | Meaning |
| --- | --- |
| $v$ | Completed JSON value being checked |
| $C(v)$ | Canonical form of $v$ used for exact comparison |
| $H_a$ | Values already completed in array instance $a$ |
| $Q(v)$ | Whether $v$ matches the `contains` predicate |
| $c_a$ | Number of matching items in array instance $a$ |
| $l,u$ | Required minimum and maximum match counts |
| $r$ | Earlier captured value |
| $\mathcal{M}$ | Imported membership set |
| $\sigma$ | Current logical guide state |
| $x$ | Token being tested or committed |
| $P_x$ | Probe token $x$ without changing logical state |
| $T_x$ | Commit token $x$ |
| $R_k$ | Roll back $k$ committed tokens |

Uniqueness:

```math
U(v)\iff C(v)\notin H_a
```

Counted `contains` predicate:

```math
c_a=\sum_{i=1}^{n}\mathbf{1}[Q(v_i)],\qquad l\le c_a\le u
```

Exact equality:

```math
E(v,r)\iff C(v)=C(r)
```

Imported membership:

```math
I(v,\mathcal{M})\iff C(v)\in\mathcal{M}
```

Pure probe and rollback:

```math
P_x(\sigma)=\sigma
```

```math
R_k(T_{x_k}(\cdots T_{x_1}(\sigma)))=\sigma
```

## Installation

Python:

```bash
pip install oc-sidememory
```

For the model-backed walkthrough:

```bash
pip install oc-sidememory transformers torch
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

## Start here: prompt to guaranteed JSON

Qwen writes the plan. OC-Sidememory keeps the activities distinct.

```python
import json

import oc_sidememory
import torch
from transformers import AutoModelForCausalLM, AutoTokenizer

MODEL_ID = "Qwen/Qwen2.5-0.5B-Instruct"

tokenizer = AutoTokenizer.from_pretrained(MODEL_ID)
model = AutoModelForCausalLM.from_pretrained(MODEL_ID).eval()

schema = {
    "type": "object",
    "properties": {
        "city": {"type": "string", "enum": ["Kyoto", "Paris", "Reykjavik"]},
        "pace": {"type": "string", "enum": ["relaxed", "balanced", "busy"]},
        "activities": {
            "type": "array",
            "items": {
                "type": "string",
                "enum": ["temples", "food tour", "museum", "hiking"],
            },
            "minItems": 3,
            "maxItems": 3,
            "uniqueItems": True,
        },
    },
    "required": ["city", "pace", "activities"],
    "additionalProperties": False,
}

prompt = """Create a balanced Kyoto itinerary.
Choose exactly three different activities from temples, food tour, museum, and hiking.
Return only the JSON object."""

messages = [
    {"role": "system", "content": "Return only JSON matching the requested schema."},
    {"role": "user", "content": prompt},
]
text = tokenizer.apply_chat_template(messages, tokenize=False, add_generation_prompt=True)
input_ids = tokenizer(text, return_tensors="pt").input_ids

vocabulary = oc_sidememory.Vocabulary.from_transformers(tokenizer)
compiled = oc_sidememory.compile_schema(
    json.dumps(schema), vocabulary, model.config.vocab_size
)
guide = oc_sidememory.SidememoryGuide(compiled)
```

The model-to-guide loop is shown once:

<details>
<summary>Show the complete Transformers decoding loop</summary>

```python

generated = []
past_key_values = None
with torch.inference_mode():
    for _ in range(128):
        result = model(
            input_ids=input_ids,
            past_key_values=past_key_values,
            use_cache=True,
        )
        past_key_values = result.past_key_values

        allowed = torch.tensor(guide.get_tokens(), dtype=torch.long)
        if allowed.numel() == 0:
            raise RuntimeError("schema has no valid continuation")

        logits = result.logits[0, -1]
        token = allowed[torch.argmax(logits[allowed])].item()
        guide.advance(token)

        if token == vocabulary.get_eos_token_id():
            break
        generated.append(token)
        if guide.is_accepting():
            break
        input_ids = torch.tensor([[token]])
    else:
        raise RuntimeError("generation exceeded max_new_tokens")

output = tokenizer.decode(generated)
print(output)
```

</details>

Output from the verified CPU run:

```json
{
  "activities": ["temples", "food tour", "museum"],
  "city": "Kyoto",
  "pace": "balanced"
}
```

Run the [complete travel-itinerary example](examples/python/00_travel_itinerary.py):

```bash
python examples/python/00_travel_itinerary.py
```

### The decoding flow

```text
prompt -> model logits -> OC-Sidememory valid token IDs -> choose token
       -> advance model cache + semantic state -> repeat -> valid JSON
```

Compile once per tokenizer/schema pair. Create one `SidememoryGuide` per sequence.

## Compose stateful rules

One guide can enforce all of these:

- `actions` must contain three distinct values and include `"answer"`;
- `customer` must exist in an immutable allowlist loaded for this request;
- `confirmation` must exactly match the earlier `request_id`, even when numbers use different valid
  JSON spellings such as `1` and `1.0`.

Prompt:

```text
Build an action plan for customer cust_7 and assign a numeric request ID.
Use search, compare, and answer exactly once. Repeat the request ID in confirmation.
Return only the JSON object.
```

```python
schema = {
    "type": "object",
    "properties": {
        "request_id": {"type": "number"},
        "customer": {"type": "string"},
        "actions": {
            "type": "array",
            "items": {"enum": ["search", "compare", "answer"]},
            "minItems": 3,
            "maxItems": 3,
            "uniqueItems": True,
            "contains": {"const": "answer"},
        },
        "confirmation": {"type": "number"},
    },
    "required": ["request_id", "customer", "actions", "confirmation"],
    "additionalProperties": False,
}

extensions = {
    "version": 1,
    "objects": [{
        "schemaPath": "$",
        "propertyOrder": ["request_id", "customer", "actions", "confirmation"],
        "captures": [{"name": "rid", "sourceProperty": "request_id"}],
        "relations": [
            {
                "targetProperty": "customer",
                "operator": "memberOf",
                "import": "live_customers",
            },
            {
                "targetProperty": "confirmation",
                "operator": "equal",
                "capture": "rid",
            },
        ],
    }],
}

imports = oc_sidememory.ImportedMemory.from_json(
    "customer-snapshot", "catalog-v1", '{"live_customers":["cust_7","cust_9"]}'
)
compiled = oc_sidememory.compile_schema(
    json.dumps(schema),
    vocabulary,
    model.config.vocab_size,
    extensions_json=json.dumps(extensions),
)
guide = oc_sidememory.SidememoryGuide(compiled, imports=imports, max_rollback=32)
```

Run the complete model-backed version:

```bash
python examples/python/06_customer_action_plan.py
```

A verified run produced:

```json
{
  "request_id": 1234567890,
  "customer": "cust_7",
  "actions": ["search", "compare", "answer"],
  "confirmation": 1234567890
}
```

## Patterns you can ship

| Product pattern | Schema and memory recipe | What cannot slip through |
| --- | --- | --- |
| Agent tool plan | `uniqueItems` + `contains` | Repeated tools or a plan missing a required action |
| Product bundle | `uniqueItems` + counted `contains` | Duplicate products or the wrong category mix |
| Account selection | Imported `memberOf` | An ID outside the request's authorized snapshot |
| RAG answer | Imported `memberOf` on the selected citation | A citation ID that was not retrieved |
| Two-field confirmation | Capture + exact `equal` relation | A confirmation value differing from its source |
| Exclusion workflow | Imported `notMemberOf` | A blocked or previously consumed value |
| Beam search/backtracking | Pure `probe` + bounded `rollback` | Semantic state leaking from an abandoned branch |

## Validate candidates without running a model

The same guide validates tokenized candidates:

```python
schema = {
    "type": "array",
    "items": {"type": "string", "enum": ["red", "green", "blue"]},
    "minItems": 3,
    "maxItems": 3,
    "uniqueItems": True,
}

compiled = oc_sidememory.compile_schema(json.dumps(schema), vocabulary, tokenizer.vocab_size)
guide = oc_sidememory.SidememoryGuide(compiled)

valid = tokenizer.encode('["red","green","blue"]', add_special_tokens=False)
duplicate = tokenizer.encode('["red","red","blue"]', add_special_tokens=False)

assert guide.accepts_tokens(valid, finish=True)
assert not guide.accepts_tokens(duplicate, finish=True)
```

`accepts_tokens`, `probe`, `advance`, and mask construction use the same Rust transition routine.
Python does not contain a second semantic validator.

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
