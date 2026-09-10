from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import statistics
import subprocess
import sys
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Callable

import oc_sidememory


@dataclass(frozen=True)
class Summary:
    minimum_ns: int
    p50_ns: int
    p95_ns: int
    p99_ns: int
    maximum_ns: int
    mean_ns: float
    standard_deviation_ns: float
    operations_per_second: float


def percentile(values: list[int], fraction: float) -> int:
    ordered = sorted(values)
    index = min(len(ordered) - 1, math.ceil(fraction * len(ordered)) - 1)
    return ordered[max(index, 0)]


def summarize(values: list[int]) -> Summary:
    mean = statistics.fmean(values)
    return Summary(
        minimum_ns=min(values),
        p50_ns=percentile(values, 0.50),
        p95_ns=percentile(values, 0.95),
        p99_ns=percentile(values, 0.99),
        maximum_ns=max(values),
        mean_ns=mean,
        standard_deviation_ns=statistics.pstdev(values),
        operations_per_second=1_000_000_000 / mean,
    )


def measure(operation: Callable[[int], object], warmup: int, iterations: int) -> tuple[Summary, str]:
    for sample in range(warmup):
        operation(sample)
    timings: list[int] = []
    digest = hashlib.sha256()
    for sample in range(iterations):
        start = time.perf_counter_ns()
        value = operation(sample)
        timings.append(time.perf_counter_ns() - start)
        digest.update(repr(value).encode("utf-8"))
    return summarize(timings), digest.hexdigest()


def vocabulary(documents: list[bytes]) -> tuple[oc_sidememory.Vocabulary, int]:
    eos = len(documents)
    mapping: dict[bytes, list[int]] = {}
    for token_id, document in enumerate(documents):
        mapping.setdefault(document, []).append(token_id)
    return oc_sidememory.Vocabulary(eos, mapping), eos + 1


def array_documents(size: int) -> list[bytes]:
    return [json.dumps([f"v{index}"], separators=(",", ":")).encode() for index in range(size)]


def plan_schema(plan: str, size: int) -> tuple[str, str | None, object | None, list[bytes]]:
    documents = array_documents(size)
    item_values = [f"v{index}" for index in range(size)]
    schema: dict[str, object] = {
        "type": "array",
        "items": {"type": "string", "enum": item_values},
        "minItems": 1,
        "maxItems": 1,
    }
    extensions = None
    imports = None
    if plan in {"unique", "combined"}:
        schema["uniqueItems"] = True
    if plan in {"contains", "combined"}:
        schema["contains"] = {"enum": item_values[::2] or ["v0"]}
        schema["minContains"] = 1
    if plan == "capture_equality":
        documents = [
            json.dumps({"id": index, "confirmation": index}, separators=(",", ":")).encode()
            for index in range(size)
        ]
        schema = {
            "type": "object",
            "properties": {"id": {"type": "integer"}, "confirmation": {"type": "integer"}},
            "required": ["id", "confirmation"],
            "additionalProperties": False,
        }
        extensions = json.dumps(
            {
                "version": 1,
                "objects": [
                    {
                        "schemaPath": "$",
                        "propertyOrder": ["id", "confirmation"],
                        "captures": [{"name": "id", "sourceProperty": "id"}],
                        "relations": [
                            {"targetProperty": "confirmation", "operator": "equal", "capture": "id"}
                        ],
                    }
                ],
            }
        )
    elif plan == "membership":
        documents = [
            json.dumps({"customer": f"v{index}"}, separators=(",", ":")).encode()
            for index in range(size)
        ]
        schema = {
            "type": "object",
            "properties": {"customer": {"type": "string"}},
            "required": ["customer"],
            "additionalProperties": False,
        }
        extensions = json.dumps(
            {
                "version": 1,
                "objects": [
                    {
                        "schemaPath": "$",
                        "propertyOrder": ["customer"],
                        "relations": [
                            {"targetProperty": "customer", "operator": "memberOf", "import": "values"}
                        ],
                    }
                ],
            }
        )
        imports = oc_sidememory.ImportedMemory.from_json(
            "semantic-matrix",
            "1",
            json.dumps({"values": item_values}),
        )
    return json.dumps(schema), extensions, imports, documents


def mask_case(plan: str, size: int, warmup: int, iterations: int) -> dict[str, object]:
    schema, extensions, imports, documents = plan_schema(plan, size)
    vocab, width = vocabulary(documents)
    compile_start = time.perf_counter_ns()
    compiled = oc_sidememory.compile_schema(schema, vocab, width, extensions_json=extensions)
    compile_ns = time.perf_counter_ns() - compile_start
    guides = [oc_sidememory.SidememoryGuide(compiled, imports=imports) for _ in range(4)]
    summary, digest = measure(
        lambda sample: guides[sample % len(guides)].get_mask(),
        warmup,
        iterations,
    )
    candidate_bytes = sum(map(len, documents))
    row = {
        "case": f"mask/{plan}/{size}",
        "operation": "mask",
        "semanticPlan": plan,
        "candidateCount": size,
        "candidateBytes": candidate_bytes,
        "modelWidth": width,
        "compileNs": compile_ns,
        "iterations": iterations,
        "warmupIterations": warmup,
        "digest": digest,
        **asdict(summary),
    }
    row["nsPerCandidate"] = summary.mean_ns / size
    row["nsPerCandidateByte"] = summary.mean_ns / candidate_bytes
    return row


def transition_cases(warmup: int, iterations: int) -> list[dict[str, object]]:
    schema, extensions, imports, documents = plan_schema("combined", 16)
    vocab, width = vocabulary(documents)
    compiled = oc_sidememory.compile_schema(schema, vocab, width, extensions_json=extensions)

    probe_guides = [oc_sidememory.SidememoryGuide(compiled, max_rollback=128, imports=imports) for _ in range(4)]
    probe, probe_digest = measure(
        lambda sample: probe_guides[sample % 4].probe(sample % len(documents)), warmup, iterations
    )

    def advance(sample: int) -> int:
        guide = oc_sidememory.SidememoryGuide(compiled, max_rollback=128, imports=imports)
        guide.advance((sample * 2) % len(documents))
        return guide.get_allowed_rollback()

    advance_summary, advance_digest = measure(advance, warmup, iterations)

    rollback_guides = []
    for sample in range(warmup + iterations):
        guide = oc_sidememory.SidememoryGuide(compiled, max_rollback=128, imports=imports)
        guide.advance((sample * 2) % len(documents))
        rollback_guides.append(guide)
    rollback_summary, rollback_digest = measure(
        lambda sample: rollback_guides[sample + warmup].rollback(1), 0, iterations
    )

    rows = []
    for operation, summary, digest in [
        ("probe", probe, probe_digest),
        ("advance", advance_summary, advance_digest),
        ("rollback", rollback_summary, rollback_digest),
    ]:
        rows.append(
            {
                "case": f"transition/{operation}",
                "operation": operation,
                "semanticPlan": "combined",
                "candidateCount": 1,
                "candidateBytes": len(documents[0]),
                "modelWidth": width,
                "iterations": iterations,
                "warmupIterations": warmup,
                "digest": digest,
                **asdict(summary),
            }
        )
    return rows


def environment() -> dict[str, object]:
    repo = Path(__file__).resolve().parents[1]
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=repo, text=True).strip())
    return {
        "commit": commit,
        "dirty": dirty,
        "buildProfile": "release native extension",
        "python": sys.version.split()[0],
        "operatingSystem": platform.platform(),
        "architecture": platform.machine(),
        "processor": platform.processor(),
        "logicalCores": os.cpu_count(),
        "tokenizerIdentity": "deterministic-byte-vocabulary",
        "tokenizerRevision": "1",
        "cacheMode": "no semantic-result cache; four equivalent guide states rotated",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Measure deterministic semantic operations.")
    parser.add_argument("--mode", choices=("quick", "full"), default="quick")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.mode == "quick":
        sizes, warmup, iterations = [3, 100], 8, 40
    else:
        sizes, warmup, iterations = [3, 100, 1_000, 10_000], 16, 100

    rows = [
        mask_case(plan, size, warmup, iterations)
        for plan in ("none", "unique", "contains", "capture_equality", "membership", "combined")
        for size in sizes
    ]
    rows.extend(transition_cases(warmup, iterations))
    result = {
        "formatVersion": 1,
        "environment": environment(),
        "matrix": {
            "semanticPlans": ["none", "unique", "contains", "capture_equality", "membership", "combined"],
            "candidateSizes": sizes,
            "tokenShape": "complete document per token",
            "transitionRollbackLimit": 128,
        },
        "results": rows,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {len(rows)} benchmark rows to {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
