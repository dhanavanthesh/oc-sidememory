# OC-Sidememory

Semantic side-memory for constrained LLM decoding.

OC-Sidememory keeps the finite-state structural guide from
[outlines-core](https://github.com/dottxt-ai/outlines-core) and adds a
checked compiler, exact tokenizer byte identities, an incremental JSON
cursor, canonical JSON values, and transactional semantic enforcement.

## Principle

Do not replace the DFA. Semantic memory is a sparse refinement of the
mask:

$$M_{\mathrm{OC}} = M_{\mathrm{DFA}} \wedge \lnot M_{\mathrm{deny}}, \qquad L_{\mathrm{OC}} \subseteq L_{\mathrm{DFA}}$$

$$\mathrm{admit}(v) \iff v \mathbin{\theta} \mathcal{M}, \qquad \theta \in \{\,\in,\ \notin,\ =,\ \neq\,\}$$

A completed value $v$ is admitted by its relation $\theta$ to runtime
memory $\mathcal{M}$: `uniqueItems` is $v \notin \mathcal{H}$ over a
history set, cross-field equality is $v = R$ over a register, membership
is $v \in S$ over an imported set. Schemas with no side-memory
constraints use the existing fast path unchanged.

## Status

`SidememoryGuide` enforces `uniqueItems`, `contains`, `minContains`, and
`maxContains` while tokens are generated. Probes, semantic masks, direct
advance, EOS, reset, and bounded rollback use the same Rust transition
engine. Exact JSON equality treats numeric aliases and object key order
according to JSON Schema equality rather than source spelling.

An optional version 1 extension plan supports direct properties in one
object instance. It can capture an earlier property and require a later
property to be equal, unequal, a member of an immutable imported set, or
not a member of that set. Extension-bearing objects use their declared
generation order. Imported memory has an application-defined identity and
version and is immutable for the lifetime of a guide.

The predicate accepted by `contains` supports boolean schemas, checked
scalar types, exact scalar `const` and `enum`, closed objects, `required`,
homogeneous arrays, array item bounds, and nested `uniqueItems`.

The checked compiler rejects unsupported predicate composition, including
nested `contains`, `$ref`, `allOf`, `anyOf`, `oneOf`, `not`, conditionals,
and `unevaluatedItems`. Capture relations are limited to direct properties
in the same object instance. Live `SidememoryGuide` serialization is not
supported. The compatibility `Guide` remains the legacy DFA-only API.

## License

Apache-2.0. See [`LICENSE`](LICENSE), [`NOTICE`](NOTICE) and
[`PROVENANCE.md`](PROVENANCE.md).
