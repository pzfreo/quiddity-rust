"""For #123: compare every available STEP input with an unmodified main binary.

Run --capture BASELINE --output /tmp/main.json.gz before editing, then
--compare CANDIDATE --baseline /tmp/main.json.gz --output /tmp/audit.json.
No inputs or recognition captures are written into the repository.
"""

import argparse
import concurrent.futures
import gzip
import json
import os
from pathlib import Path
import subprocess


def inputs():
    root = Path(__file__).resolve().parent.parent
    fixtures = root / "tests/fixtures"
    paths = {
        str(p.resolve())
        for p in fixtures.rglob("*")
        if p.name.endswith((".step", ".stp", ".step.gz", ".stp.gz"))
    }
    corpus = Path(os.environ["QUIDDITY_CORPUS"])
    for row in json.loads((fixtures / "corpus.json").read_text())["files"]:
        path = corpus / row["file"]
        assert path.is_file(), path
        paths.add(str(path.resolve()))
    bearing = root / "repro/bearing-608zz.step"
    if bearing.exists():
        paths.add(str(bearing))
    return sorted(paths)


def recognise(binary, path):
    run = subprocess.run([binary, path], capture_output=True, text=True)
    if run.returncode:
        return {"error": run.stderr}
    result = json.loads(run.stdout)
    return {
        "records": result["document"]["records"],
        "fingerprints": result["fingerprints"]["features"],
        "accounting": result["document"]["face_accounting"],
    }


def main():
    parser = argparse.ArgumentParser(__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--capture")
    mode.add_argument("--compare")
    parser.add_argument("--baseline")
    parser.add_argument("--output", required=True)
    parser.add_argument("--workers", type=int, default=4)
    args = parser.parse_args()
    paths = inputs()
    results = {}
    with concurrent.futures.ThreadPoolExecutor(args.workers) as pool:
        pending = {
            pool.submit(recognise, args.capture or args.compare, p): p for p in paths
        }
        for n, future in enumerate(concurrent.futures.as_completed(pending), 1):
            results[pending[future]] = future.result()
            if n % 100 == 0:
                print(f"{n}/{len(paths)}", flush=True)
    if args.capture:
        with gzip.open(args.output, "wt") as out:
            json.dump(results, out, sort_keys=True)
        return
    with gzip.open(args.baseline, "rt") as source:
        baseline = json.load(source)
    exceptions, grooves = [], []
    assert results.keys() == baseline.keys(), "audit input set changed"
    body = {"thin_wall_bodies", "sheet_metal_bodies", "plates", "polygonal_stock"}
    for path, current in sorted(results.items()):
        old = baseline[path]
        if "error" in old or "error" in current:
            if old != current:
                exceptions.append({"part": path, "error_change": [old, current]})
            continue
        new = [r for r in current["records"] if r["family"] == "face_grooves"]
        for key in ("records", "fingerprints"):
            retained = [r for r in current[key] if r["family"] != "face_grooves"]
            if old[key] != retained:
                exceptions.append({"part": path, "changed": key})
        claimed = {
            f for r in old["records"] if r["family"] not in body for f in r["faces"]
        }
        for record in new:
            overlap = sorted(claimed.intersection(record["faces"]))
            if overlap:
                exceptions.append({"part": path, "previously_claimed": overlap})
            refused = [f for f in record["faces"] if old["accounting"][f]["group"] == "refused"]
            if refused:
                exceptions.append({"part": path, "previously_refused": refused})
            grooves.append({"part": path, "previous_accounting": [old["accounting"][f] for f in record["faces"]], **record})
    report = {
        "baseline_revision": "42649a2",
        "inputs": len(paths),
        "readable": sum("error" not in r for r in results.values()),
        "unchanged_refusals": sum("error" in r for r in results.values()),
        "new_grooves": grooves,
        "exceptions": exceptions,
    }
    Path(args.output).write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k != "new_grooves"}, indent=2))
    print(f"new grooves: {len(grooves)}")
    raise SystemExit(bool(exceptions))


if __name__ == "__main__":
    main()
