# Modifications

OC-Sidememory extends and modifies the imported `outlines-core` source tree.

Major changes include:

- reorganized the project into Rust core, Python, and CLI crates;
- added schema compilation and semantic constraint handling;
- added canonical JSON value representation and equality;
- added `uniqueItems` history tracking;
- added `contains`, `minContains`, and `maxContains` counters;
- added captured values and cross-field relations;
- added runtime imported membership sets;
- added transactional probe, journal, checkpoint, and rollback support;
- added semantic token-mask filtering;
- added OC-Sidememory Python and Rust APIs;
- added CLI commands, tests, examples, documentation, and benchmarks.

Some original `outlines-core` files were retained, relocated, or modified. Those files include source-level notices identifying their origin.

New OC-Sidememory semantic-memory modules were developed specifically for this project.

For the exact upstream source, see `PROVENANCE.md`.
