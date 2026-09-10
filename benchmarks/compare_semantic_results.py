from __future__ import annotations

import argparse
import json
from pathlib import Path


def load(path: Path) -> dict[str, dict[str, object]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    return {row["case"]: row for row in payload["results"]}


def main() -> int:
    parser = argparse.ArgumentParser(description="Compare semantic benchmark runs.")
    parser.add_argument("baseline", type=Path)
    parser.add_argument("optimized", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    before, after = load(args.baseline), load(args.optimized)
    if before.keys() != after.keys():
        raise SystemExit("benchmark case sets differ")
    comparisons = []
    for name in sorted(before):
        old, new = before[name], after[name]
        if old["digest"] != new["digest"]:
            raise SystemExit(f"output digest mismatch for {name}")
        old_p95, new_p95 = float(old["p95_ns"]), float(new["p95_ns"])
        comparisons.append(
            {
                "case": name,
                "baselineP95Ns": old_p95,
                "optimizedP95Ns": new_p95,
                "p95Speedup": old_p95 / new_p95,
                "p95ChangePercent": (new_p95 / old_p95 - 1.0) * 100.0,
                "digestMatch": True,
            }
        )
    output = {"formatVersion": 1, "comparisons": comparisons}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")
    print(f"compared {len(comparisons)} benchmark rows")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
