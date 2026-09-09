# Provenance

OC-Sidememory is built on
[outlines-core](https://github.com/dottxt-ai/outlines-core) (Apache-2.0),
with its source imported at commit
`116a7c381b446814fdfc9b2b28bea7ca4f900aae`.

The first commit in this repository is that upstream tree, unmodified.
Every later commit is OC-Sidememory work.

- `LICENSE` (Apache-2.0) is kept in full. An OC-Sidememory copyright line
  is added beside the upstream one, as Apache-2.0 section 4 allows.
- `NOTICE` carries attribution to outlines-core.
- The `upstream` git remote points at outlines-core, for reference only.

OC-Sidememory is an independent project. It is not a fork staged for
merge-back and claims no upstream endorsement.

The exact decimal implementation was written for this repository. It
does not contain copied Maskforge code. Compiler
classification tests use the upstream JSON Schema Test Suite by pinned
revision as test data; the suite is not distributed with the package.

The JSON cursor owns generation-tagged frame identities and buffered
container children. Schema dispatch and parent routing will be added with
the authoritative semantic runtime instead of being represented by unused
placeholder frame types.

Debug replay currently compares committed token replay through the Index
and a fresh cursor. Byte-DFA replay and accepting-boundary finish probes
remain required when transactional runtime state is introduced.
