# Performance

Performance changes are accepted only after correctness parity and A/B measurement.

## Recorded environment

| Field | Value |
|---|---|
| Operating system | Windows 11 x86-64 |
| Processor | AMD Ryzen 3 5300U |
| CPU cores | 4 physical, 8 logical |
| Rust | 1.97.0 |
| Python | 3.12 |

## Initial mask measurements

Release-mode synthetic vocabulary results:

| Semantic plan | Candidates | p50 | p95 |
|---|---:|---:|---:|
| No plan | 100 | 0.0076 ms | 0.0114 ms |
| Uniqueness | 100 | 0.2600 ms | 0.2910 ms |
| Contains | 100 | 0.2510 ms | 0.3675 ms |
| Combined | 100 | 0.3492 ms | 0.4701 ms |
| No plan | 1,000 | 0.0720 ms | 0.0776 ms |
| Uniqueness | 1,000 | 2.8757 ms | 3.7696 ms |
| Contains | 1,000 | 2.7653 ms | 3.4229 ms |
| Combined | 1,000 | 3.7981 ms | 5.0297 ms |

These are local engineering results, not general product claims. Candidate count, candidate bytes, sample count, build profile, commit, and host must accompany comparisons.

## Optimization decision

A reusable candidate-buffer experiment preserved semantic digests but regressed representative p95 latency. Uniqueness with 100 candidates moved from 0.2910 ms to 0.3939 ms. The change was removed.

The retained design uses journaled mutation, compiled dispatch, exact collision checks, and reusable event scratch. It does not claim zero overhead.

## Allocation and memory profile

The release-mode native allocation benchmark produced these local results for a 1,000-candidate combined mask:

| Measurement | Value |
|---|---:|
| Allocations | 12,921 |
| Allocated bytes | 390,500 |
| Peak live allocation increase | 14,118 bytes |
| Retained allocation increase | 5,900 bytes |
| Logical bytes for one retained rollback checkpoint | 2,726 bytes |

A separate 100-candidate combined run recorded a process peak RSS of 26.07 MB. Python allocation tracing peaked at 19,759 bytes, excluding native Rust allocations. These values describe the recorded Windows environment only.
