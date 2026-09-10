# Semantic benchmarks

`semantic_matrix.py` measures compilation separately from masks and transitions. It uses deterministic byte vocabularies and requires no network access.

Run from the integration environment with an installed release extension:

```powershell
uv run --project . --no-sync python ..\oc-sidememory\benchmarks\semantic_matrix.py `
  --mode full --output evidence\semantic-results.json
```

The runner rotates four equivalent guide instances, validates output digests, and reports minimum, p50, p95, p99, maximum, mean, standard deviation, operations per second, and normalized candidate costs. It does not claim model throughput or cross-platform performance.

Compare two runs with `compare_semantic_results.py`. The comparison fails if case sets or output digests differ.
