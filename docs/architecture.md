# Architecture

`CompiledSchema` is immutable and shareable. It holds the DFA index, validated token-byte table, schema IR, resource limits, and constant-time semantic dispatch plans.

Each `Guide` owns its DFA state, incremental cursor, schema router, canonical arena, uniqueness histories, contains counters, capture registers, journals, rollback boundaries, scratch storage, and lifecycle.

One transition routine serves probe, mask, advance, sequence checking, and EOS. A token is checked structurally, consumed byte by byte, and its cursor events are processed in order. Probe restores the outer mark; successful advance retains one rollback boundary; denial and recoverable errors restore atomically.

Restoration removes semantic references before rewinding the router, cursor, and arena. Immutable imports live in a separate shared arena. Mask construction starts from DFA candidates and can only remove them.
