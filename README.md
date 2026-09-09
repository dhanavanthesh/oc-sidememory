# OC-Sidememory

Semantic side-memory for constrained LLM decoding.

OC-Sidememory keeps the finite-state structural guide from
[outlines-core](https://github.com/dottxt-ai/outlines-core) and refines
its allowed-token mask with exact, rollbackable semantic state, so
constraints that depend on earlier-generated values are enforced during
generation.

Example: an array with `"uniqueItems": true` and
`"items": {"enum": ["red", "green", "blue"]}`. After the model emits
`red`, a second `red` is still structurally valid but semantically
wrong. OC-Sidememory drops such tokens from the mask before sampling.

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

Pre-release. Design stage; the engine is not implemented yet. First
target feature is array `uniqueItems`.

## License

Apache-2.0. See [`LICENSE`](LICENSE), [`NOTICE`](NOTICE) and
[`PROVENANCE.md`](PROVENANCE.md).
