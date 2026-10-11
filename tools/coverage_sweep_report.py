"""Aggregate `examples/coverage_sweep.rs` output over MFInstSeg into the coverage-sweep report
Python quiddity published at f1b02ab (`docs/benchmarks/mfinstseg-full-coverage-sweep-f1b02ab.json`
in quiddity), and print the two side by side.

    python3 tools/coverage_sweep_report.py SWEEP.jsonl MFINSTSEG_ROOT QUIDDITY_CHECKOUT OUT.json

Records are counted over Python's f1b02ab census keys (`quiddity.census.CENSUS_BINDINGS` at that
commit), so totals compare like for like. Families outside that census are reported separately.
"""

from __future__ import annotations

import json
import statistics
import sys
from collections import Counter
from pathlib import Path

# Python census key at f1b02ab -> `Features` field.
CENSUS = {
    "hole": "holes",
    "hole_pattern": "hole_patterns",
    "boss": "bosses",
    "step": "turned_steps",
    "groove": "grooves",
    "flat": "flats",
    "slot": "slots",
    "oriented_slot": "oriented_slots",
    "chamfer": "chamfers",
    "angled_step": "angled_steps",
    "paired_ramp_step": "paired_ramp_steps",
    "through_step": "through_steps",
    "circular_blind_step": "circular_blind_steps",
    "blend": "blends",
    "fillet": "fillets",
    "countersink": "countersinks",
    "plate": "plates",
    "section_recess": "section_recesses",
}
# Inventory internals the census does not count (`feature_census` docs), and the legacy fields
# section recesses replace in it.
NOT_FEATURES = {
    "step_levels",
    "risers",
    "section_passages",
    "pockets",
    "prismatic_pockets",
    "channels",
    "rectangular_blind_slots",
    "round_bottom_blind_slots",
    "edge_open_circular_pockets",
    "edge_open_prismatic_recesses",
    "pocket_patterns",
}


def truth(root: Path, model_id: str) -> tuple[set[int], int]:
    """The classes a model's labels contain and its instance count (non-empty `inst` rows'
    equivalence classes, as quiddity's `_instance_components`)."""

    labels = json.loads((root / "labels" / f"{model_id}.json").read_text())[0][1]
    classes = set(labels["seg"].values())
    instances = {
        frozenset(i for i, linked in enumerate(row) if linked) for row in labels["inst"]
    }
    instances.discard(frozenset())
    return classes, len(instances)


def main() -> None:
    sweep_path, root, quiddity, out = (Path(a) for a in sys.argv[1:5])
    python = json.loads(
        (quiddity / "docs/benchmarks/mfinstseg-full-coverage-sweep-f1b02ab.json").read_text()
    )
    taxonomy = python["per_class"]
    lines = [json.loads(line) for line in sweep_path.read_text().splitlines()]
    families: Counter[str] = Counter()
    others: Counter[str] = Counter()
    failures = []
    per_class = {c: {"name": t["name"], "status": t["status"], "models": 0, "silent": 0}
                 for c, t in taxonomy.items()}
    ratios = []
    below_one = silent = ok = refusals = 0
    seconds = []
    for line in lines:
        classes, instances = truth(root, line["id"])
        for c in classes:
            per_class[str(c)]["models"] += 1
        if line["status"] != "ok":
            failures.append({"id": line["id"], "status": line["status"],
                             "message": line.get("message", "")})
            continue
        ok += 1
        seconds.append(line["seconds"])
        refusals += line["section_recess_refusal"] is not None
        counts = line["counts"]
        records = sum(counts[field] for field in CENSUS.values())
        for key, field in CENSUS.items():
            families[key] += counts[field]
        for field, n in counts.items():
            if field not in CENSUS.values() and field not in NOT_FEATURES:
                others[field] += n
        if records == 0:
            silent += 1
            for c in classes:
                per_class[str(c)]["silent"] += 1
        if instances:
            ratios.append(records / instances)
            below_one += records < instances
    report = {
        "format": "quiddity-coverage-sweep",
        "format_version": 1,
        "kind": "coverage-sweep",
        "not_an_effectiveness_baseline": True,
        "package": {"name": "quiddity-rust"},
        "dataset": {"name": "mfinstseg", "models": len(lines),
                    "selection": "every model, lexical order"},
        "summary": {
            "models": len(lines),
            "ok": ok,
            "failed": len(failures),
            "silent": silent,
            "total_records": sum(families.values()),
            "records_per_instance_median": round(statistics.median(ratios), 4),
            "ratio_denominator": len(ratios),
            "models_below_one": below_one,
            "section_recess_refusals": refusals,
            "median_seconds": round(statistics.median(seconds), 4),
        },
        "families": dict(sorted(families.items())),
        "families_outside_f1b02ab_census": dict(sorted((k, v) for k, v in others.items() if v)),
        "per_class": per_class,
        "failures": failures,
    }
    out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

    def row(name: str, py: object, rs: object) -> None:
        delta = rs - py if isinstance(py, (int, float)) and isinstance(rs, (int, float)) else ""
        print(f"{name:32} {py!s:>10} {rs!s:>10} {delta!s:>10}")

    print(f"{'':32} {'python':>10} {'rust':>10} {'delta':>10}")
    for key in ("models", "ok", "failed", "silent", "total_records",
                "records_per_instance_median", "models_below_one"):
        row(key, python["summary"].get(key), report["summary"][key])
    print()
    for key in sorted(set(python["families"]) | set(families)):
        row(key, python["families"].get(key, 0), families.get(key, 0))
    print()
    for c, t in sorted(per_class.items(), key=lambda kv: int(kv[0])):
        row(f"{c} {t['name']}"[:32] + " silent", taxonomy[c]["silent"], t["silent"])


if __name__ == "__main__":
    main()
