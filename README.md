# OC-Sidememory

Semantic side-memory for constrained LLM decoding.

OC-Sidememory keeps the finite-state structural guide from
[outlines-core](https://github.com/dottxt-ai/outlines-core) and adds a
checked compiler, exact tokenizer byte identities, an incremental JSON
cursor, and canonical JSON values for later semantic enforcement.

Array uniqueness is represented in a compiled `MemoryPlan`. Runtime
history and semantic mask filtering are not available yet.

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

The compiler and exact-value foundation are implemented. The
compatibility `Guide` remains DFA-only while semantic enforcement is
under development.

## License

Apache-2.0. See [`LICENSE`](LICENSE), [`NOTICE`](NOTICE) and
[`PROVENANCE.md`](PROVENANCE.md).
