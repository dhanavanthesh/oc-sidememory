# Command-line interface

The `oc-sidememory` binary provides `compile`, `tokens`, `mask`, `check`, `inspect-imports`, and `capabilities`. Inputs and outputs use `formatVersion: 1`.

Vocabulary files declare `modelWidth`, `eosTokenId`, and entries with `id` and `bytesBase64`. Bytes need not be UTF-8. Conflicting IDs and unknown versions are rejected.

Exit codes are 0 success, 2 invalid input, 3 compilation, 4 structural rejection, 5 semantic rejection, 6 resource limit, 7 lifecycle or rollback, and 8 invariant/runtime failure. Diagnostics use stderr and JSON uses stdout.

The separate `convert-json-schema` compatibility binary remains available.
