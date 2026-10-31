#!/usr/bin/env python3
"""Compare slice-1 quantities with Draftwright (reports go to stdout, never committed).

Build `cargo build --release --example dimension_plan` first. Run:
  python3 tools/compare_dimensioning.py --draftwright /path/to/draftwright \
    --quality-list /path/to/dw/tools/quality_parts.txt --corpus /path/to/corpus \
    --plate /path/to/plate.step > /path/to/scratchpad/dimensioning-comparison.json

Each executable runs sequentially. The comparison expands compound hole callouts into
atomic quantities, ignores placement, and lists EVERY unmatched quantity. Counts include
unplaced wants: sheet space is outside the shared planner's responsibility.
"""
import argparse
import collections
import hashlib
import json
import pathlib
import re
import subprocess


def run(args):
    p = subprocess.run([str(a) for a in args], capture_output=True, text=True)
    if p.returncode:
        raise RuntimeError(f"{args[0]} exited {p.returncode}: {p.stderr[:1200]} {p.stdout[:600]}")
    return json.loads(p.stdout)


def axis(parameter):
    if parameter.startswith("location.length."):
        return parameter.rsplit(".", 1)[1]
    if "step_height" in parameter:
        return "z"
    if "step_position" in parameter:
        return parameter.rsplit(".", 1)[1][0]
    return {"width.length": "x", "depth.length": "y", "height.length": "z"}.get(parameter)


def dw_quantities(plan):
    result = []
    statements = 0
    dimensions = list(plan.get("dimensions", []))
    stated = {(m["feature"], m["parameter"]) for d in dimensions for m in d["measurements"]}
    omitted = {o["id"]: o for o in plan.get("omitted", [])}
    # A table changes representation, not which quantities the planner requested.
    # Count its covered pitch/location wants even though they left `dimensions`.
    for table in plan.get("hole_tables", []):
        table_locations = set()
        for m in table.get("covered", []):
            claim = (m["feature"], m["parameter"])
            if claim in stated:
                continue
            signature = (m["parameter"], round(m["value"], 3))
            if m["parameter"].startswith("location.length."):
                if signature in table_locations:
                    continue
                table_locations.add(signature)
            id_ = f"d:{m['feature']}/{m['parameter']}"
            dimensions.append({"id": id_, "label": omitted.get(id_, {}).get("label", str(m["value"])), "measurements": [m]})
            stated.add(claim)
    for d in dimensions:
        ms = [m for m in d["measurements"] if m["feature"] in ("envelope", "step_level") or m["feature"].startswith(("f:hole:", "f:hole_pattern:"))]
        if not ms:
            continue
        statements += 1
        groups = collections.defaultdict(list)
        for m in ms:
            p = m["parameter"]
            if p.startswith(("location.length.", "step_height", "step_position")) or (m["feature"] == "envelope" and p in ("width.length", "depth.length", "height.length")):
                role = "location"
            elif "pitch" in p:
                role = "pitch"
            else:
                role = p
            groups[(role, round(m["value"], 3), axis(p) if role == "location" else None)].append(m)
        # Coincident claims are one statement, while a compound callout carries several sizes.
        for (role, value, along), members in groups.items():
            count = len({m["feature"] for m in members if m["feature"].startswith("f:hole:")}) if role not in ("location", "pitch", "bolt_circle.diameter") else 1
            count = max(1, count)
            if role == "pitch":
                repeat = re.match(r"^(\d+)×", d["label"])
                count = int(repeat.group(1)) if repeat else 1
            result.append({"key": [role, value, along, count], "id": d["id"], "label": d["label"]})
    return statements, result


def table_coordinate_discrepancies(plan):
    """Keep evidence when a schedule's actual coordinates disagree with its want credits."""
    differences = []
    for table in plan.get("hole_tables", []):
        covered = {(m["feature"], m["parameter"]): m["value"] for m in table.get("covered", [])}
        for row in table.get("rows", []):
            for m in row.get("measurements", []):
                key = (m["feature"], m["parameter"])
                if m["parameter"].startswith("location.length.") and key in covered and abs(covered[key] - m["value"]) > 0.001:
                    differences.append({"table": table["id"], "feature": m["feature"],
                                        "parameter": m["parameter"], "row": m["value"], "covered": covered[key]})
    return differences


def q_quantities(doc):
    result = []
    for r in doc["required"]:
        if "size" in r["kind"]:
            role = r["kind"]["size"]["role"]
            along = None
            count = r["count"]
        else:
            role = "pitch" if isinstance(r["pattern"], dict) and "pitch" in r["pattern"] else "location"
            local_axis = r["kind"].get("location", {}).get("axis", 0)
            direction = doc["input"]["frame"]["xyz"[local_axis]]
            world_axis = max(range(3), key=lambda k: abs(direction[k]))
            along = "xyz"[world_axis] if role == "location" else None
            count = r["pattern"]["pitch"]["intervals"] if role == "pitch" else 1
        result.append({"key": [role, round(r["nominal"], 3), along, count], "id": r["id"]})
    return result


def difference(qs, ds):
    remaining = list(ds)
    extra = []
    for q in qs:
        found = next((i for i, d in enumerate(remaining) if q["key"] == d["key"]), None)
        if found is None:
            extra.append(q)
        else:
            remaining.pop(found)
    return extra, remaining


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--draftwright", type=pathlib.Path, required=True)
    p.add_argument("--quality-list", type=pathlib.Path, required=True)
    p.add_argument("--corpus", type=pathlib.Path, required=True)
    p.add_argument("--plate", type=pathlib.Path, required=True)
    p.add_argument("--cache", type=pathlib.Path, help="optional raw JSON cache outside the repository")
    p.add_argument("--planner", type=pathlib.Path, default=pathlib.Path("target/release/examples/dimension_plan"))
    p.add_argument("--check", action="store_true", help="fail on excess atomic requirements, size/count differences, or execution errors")
    args = p.parse_args()
    files = [("plate", args.plate)] + [(s, args.corpus / s) for s in args.quality_list.read_text().splitlines() if s and not s.startswith("#")]
    rows = []
    for name, path in files:
        try:
            q = run([args.planner.resolve(), path])
            cache_key = hashlib.sha256(path.read_bytes() + str(args.draftwright.stat().st_mtime_ns).encode()).hexdigest()
            cache_file = args.cache / (cache_key + ".dw.json") if args.cache else None
            if cache_file and cache_file.exists():
                d = json.loads(cache_file.read_text())
            else:
                d = run([args.draftwright.resolve(), "plan", path])
            if args.cache:
                args.cache.mkdir(parents=True, exist_ok=True)
                cache_file.write_text(json.dumps(d))
                (args.cache / (cache_key + ".q.json")).write_text(json.dumps(q))
            n, ds = dw_quantities(d)
            qs = q_quantities(q)
            extra, missing = difference(qs, ds)
            rows.append({"part": name, "holes": len(q["input"]["holes"]), "planes": len(q["input"]["planes"]), "required": len(qs), "deferred": len(q["input"].get("deferred", [])), "dw_statements": n, "dw_quantities": len(ds), "planner_only": extra, "draftwright_only": missing, "table_coordinate_discrepancies": table_coordinate_discrepancies(d)})
        except (RuntimeError, OSError, KeyError) as e:
            rows.append({"part": name, "error": str(e)})
        print(json.dumps(rows[-1]), flush=True)
    if args.check:
        failures = [row for row in rows if "error" in row or row["required"] > row["dw_quantities"]
                    or any(q["key"][0] not in ("location", "pitch", "od.diameter")
                           for q in row["planner_only"] + row["draftwright_only"])]
        if failures:
            raise SystemExit(1)


if __name__ == "__main__":
    main()
