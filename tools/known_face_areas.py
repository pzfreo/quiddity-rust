"""Write ``tests/fixtures/known_face_areas.json``: a verdict for every face area the port gives
that differs from OpenCascade's, with the evidence.

    FACE_AREAS_DUMP=/tmp/found.json QUIDDITY_CORPUS=../quiddity/tests/corpus \\
        cargo test --release -p haecceity --test face_areas
    python tools/known_face_areas.py /tmp/found.json

The differences come from ``crates/haecceity/tests/face_areas.rs``; the references from
``tests/fixtures/face_areas.json.gz`` (``tools/capture_face_areas.py``: ``face.area``,
OpenCascade's adaptive rule, its adaptive area once the face's pcurves are projected again
from the 3D edges, and the edges' tolerance strip) and ``tests/fixtures/face_area_evidence.json.gz``
(``tools/face_area_evidence.py``: independent integrations over the region the 3D edges bound).

Verdicts, by the first rule that holds (agreement is 1e-6 relative, as in the test):

- the port agrees with OpenCascade's adaptive rule: rust-correct (``face.area``'s fixed Gauss
  rule is coarse there, an approximation README's "What parity means" does not reproduce);
- every independent integration agrees with the port, or one does and so does OpenCascade's area
  with rebuilt pcurves: rust-correct (OpenCascade integrates the face's pcurves, approximations
  of its edges, where the port integrates the edges);
- there is no independent integration and OpenCascade's area with rebuilt pcurves agrees:
  rust-correct, on that evidence;
- the independent integrations agree with each other and none with the port, and either there
  are two of them or one agrees with an OpenCascade reference: rust-wrong;
- otherwise undetermined.

A few faces have their own reasons (``SPECIAL``).
"""

from __future__ import annotations

import gzip
import json
import sys
from pathlib import Path

FIXTURES = Path(__file__).resolve().parent.parent / "tests" / "fixtures"
OUT = FIXTURES / "known_face_areas.json"

PLACEMENT = (
    "Under the test's motion (a translation and 37 degrees about (1, 2, 3)) this B-spline face's "
    "area changes by {change:.1e} relative (pinned: {rust!r} unmoved, {moved!r} moved); all but one "
    "corpus face agree to 1e-12. Not traced. Whether a pin at this scale holds on Linux is a "
    "maintainer question (docs/review-2026-10-08.md)."
)

SPECIAL = {
    ("inventory_refusal/14052.step.gz", 1, "occ"): (
        "rust-correct",
        "Analytic: the plate is the 22.110602 x 11.932120 rectangle less the 20.716942 x 5.125514 "
        "notch, less a triangular hole (1.502501) and a circular hole (r 1.648290): 147.6036119780, "
        "the port's area to 1e-15. OpenCascade gives {occ} (fixed and adaptive rules alike), which "
        "adds the triangle: this deliberately malformed file orients the triangle's loop as an outer "
        "boundary, which the port reads from the geometry instead.",
    ),
    ("cadgenbench_inputs/cgb217.step.gz", 34, "occ"): (
        "rust-wrong",
        "tools/face_area_evidence.py gives 28.16983104 by Green's theorem and by direct slices; the "
        "port gives {rust}. Traced to the boundary walk: the port's answer moves with the reference "
        "its inner integral starts from (28.169842 from u = 0, 28.169781 from u = 1), so its path "
        "does not close in v, by 2.2e-6, all of it on edge 89 between t = 0.8328 and 0.7427, where "
        "the edge dips 1.6e-3 into the face from the side v = 1 it otherwise runs outside; the panel "
        "sums there do not add up to the foot point's change in v. Not yet resolved. OpenCascade "
        "gives {occ} (fixed rule), {fine} (adaptive) and {rebuilt} (pcurves projected from the edges).",
    ),
    ("cadgenbench_inputs/cgb207.step", 125, "occ"): (
        "rust-wrong",
        "tools/face_area_evidence.py gives 47.45338085 (Green's theorem) and 47.45337956 (direct "
        "slices); the port gives {rust}, 1e-6 more. Unlike the faces fixed with it, the port's walk "
        "closes here (its answer does not move with the inner integral's reference, to 6e-12), so "
        "the difference is in the path the foot points take, not in the quadrature; not traced "
        "further. OpenCascade gives {occ} (fixed rule), {fine} (adaptive) and {rebuilt} (pcurves "
        "projected from the edges).",
    ),
    ("cadgenbench_inputs/cgb242.step.gz", 483, "occ"): (
        "rust-wrong",
        "A degenerate seam-to-seam sliver: OpenCascade bounds it by one B-spline edge used twice and "
        "a degenerate edge (a triangulation of it has area 1e-21; face.area {occ}, adaptive {fine}). "
        "The port reads its two edges as the two sides of a closed v range and integrates the cap "
        "between them, {rust}; negligible, but not the face's area.",
    ),
}


def g(x) -> str:
    return "none" if x is None else "%.10g" % x


def near(a, b) -> bool:
    return a is not None and b is not None and abs(a - b) <= 1e-6 * abs(b) + 1e-9


def rel(a, b) -> str:
    return "%.0e" % (abs(a - b) / max(abs(b), 1e-300))


def worst(a, values) -> str:
    return rel(a, max(values, key=lambda v: abs(a - v) / max(abs(v), 1e-300)))


def occ_text(ref: dict) -> str:
    text = f"face.area {g(ref['occ'])}, adaptive {g(ref['fine'])}"
    if ref["rebuilt"] is not None:
        text += f", and {g(ref['rebuilt'])} with the pcurves projected again from the edges"
    return text


def strip_text(rust: float, ref: dict) -> str:
    d = abs(rust - ref["occ"])
    where = "within" if d <= ref["band"] else "beyond"
    return (
        f"The difference from face.area, {d:.2g}, is {where} the strip the edges' tolerances "
        f"allow the boundary ({ref['band']:.2g})."
    )


def evidence_text(ev: dict) -> str:
    names = {"green": "Green's theorem", "slices": "direct slices"}
    return " and ".join(f"{g(v)} ({names[k]})" for k, v in ev.items() if v is not None)


def verdict(row: dict, ref: dict, ev: dict) -> tuple[str, str]:
    rust = row["rust"]
    values = [v for v in ev.values() if v is not None]
    if near(rust, ref["fine"]):
        return "rust-correct", (
            f"face.area's fixed Gauss rule is coarse here: OpenCascade's own integration of the same "
            f"face driven to a relative error of 1e-9 gives {g(ref['fine'])}, {rel(rust, ref['fine'])} "
            f"from the port's {g(rust)}; face.area gives {g(ref['occ'])}."
        )
    backed = [v for v in values if near(rust, v)]
    if values and (len(backed) == len(values) or (backed and near(rust, ref["rebuilt"]))):
        names = {"green": "Green's theorem", "slices": "direct slices"}
        odd = [f"{names[k]} gives {g(v)}" for k, v in ev.items() if v is not None and not near(rust, v)]
        also = (
            f"; so does OpenCascade's area with the pcurves projected again from the edges, "
            f"{g(ref['rebuilt'])} ({'; '.join(odd)}, off the boundary its other method follows)"
            if odd
            else ""
        )
        return "rust-correct", (
            f"The port integrates the region the face's 3D edge curves bound; "
            f"tools/face_area_evidence.py gives {evidence_text({k: v for k, v in ev.items() if v is not None and near(rust, v)})}, "
            f"within {worst(rust, backed)} of the port's {g(rust)}{also}. OpenCascade "
            f"integrates the face's pcurves, approximations of those edges: {occ_text(ref)}. "
            + strip_text(rust, ref)
        )
    if not values and near(rust, ref["rebuilt"]):
        return "rust-correct", (
            f"OpenCascade's own adaptive area of the face with its pcurves projected again from its 3D "
            f"edges is {g(ref['rebuilt'])}, {rel(rust, ref['rebuilt'])} from the port's {g(rust)}; "
            f"face.area ({g(ref['occ'])}) and the adaptive rule ({g(ref['fine'])}) integrate the "
            f"face's own pcurves, approximations of those edges (tools/face_area_evidence.py cannot "
            f"place this boundary). " + strip_text(rust, ref)
        )
    refs = {"face.area": ref["occ"], "the adaptive rule": ref["fine"], "rebuilt pcurves": ref["rebuilt"]}
    agree = [name for name, r in refs.items() if any(near(v, r) for v in values)]
    settled = values and all(near(v, values[0]) for v in values)
    if rust is not None and settled and not backed and (agree or len(values) > 1):
        with_occ = (
            f", agreeing with OpenCascade ({' and '.join(agree)})" if agree else ", agreeing with each other"
        )
        return "rust-wrong", (
            f"tools/face_area_evidence.py gives {evidence_text(ev)}{with_occ}, where the port's "
            f"{g(rust)} differs by {rel(rust, values[0])}; OpenCascade gives {occ_text(ref)}. Not traced."
        )
    found = evidence_text(ev) or "nothing (it cannot place this boundary)"
    return "undetermined", (
        f"The port gives {g(rust)}; tools/face_area_evidence.py gives {found}; OpenCascade gives "
        f"{occ_text(ref)}. The answers spread beyond 1e-6 and none is confirmed."
    )


def order_entry(row: dict, captured: dict, evidence: dict) -> tuple[str, str]:
    """A file whose faces the reader does not walk in OpenCascade's order: matched by area."""

    areas = json.loads(row["report"].split("; areas ")[1].replace("Some(", "").replace(")", ""))
    occ = captured["area"]
    pairs, odd = [], []
    for i, a in enumerate(areas):
        j = min(range(len(occ)), key=lambda k: abs(occ[k] - a))
        pairs.append(f"{i}->{j}")
        if not near(a, occ[j]):
            ev = evidence.get(f"{row['file']}#{j}", {})
            odd.append(
                f"the port's face {i} ({g(a)}) is OpenCascade's face {j} (face.area {g(occ[j])}, "
                f"adaptive {g(captured['area_fine'][j])}, pcurves rebuilt {g(captured['area_3d'][j])}; "
                f"tools/face_area_evidence.py gives {evidence_text(ev) or 'nothing'}"
                + (
                    ", agreeing with the port)"
                    if any(near(a, v) for v in ev.values() if v is not None)
                    else ", which does not settle it: this difference is unexplained)"
                )
            )
    head = row["report"].split("; areas ")[0]
    return "not-applicable", (
        f"OpenCascade's import healing reorders this malformed part's faces (tests/corpus.rs's "
        f"'is OTHER' divergence; here {head}), so its faces are not compared by index. Matched by "
        f"area ({', '.join(pairs)}), each agrees to 1e-6"
        + (" but these: " + "; ".join(odd) if odd else "")
        + "."
    )


def main() -> None:
    rows = json.loads(Path(sys.argv[1]).read_text())
    captured = {
        f["file"]: f for f in json.loads(gzip.decompress((FIXTURES / "face_areas.json.gz").read_bytes()))["files"]
    }
    evidence = json.loads(gzip.decompress((FIXTURES / "face_area_evidence.json.gz").read_bytes()))["faces"]
    out = []
    for row in rows:
        file, face, check = row["file"], row["face"], row["check"]
        cap = captured[file]
        entry = {"file": file, "face": face, "check": check}
        if check == "order":
            entry["verdict"], entry["reason"] = order_entry(row, cap, evidence)
        elif check == "placement":
            change = abs(row["rust"] - row["moved"]) / max(abs(row["rust"]), abs(row["moved"]))
            entry["verdict"] = "rust-wrong"
            entry["reason"] = PLACEMENT.format(change=change, rust=row["rust"], moved=row["moved"])
            entry["moved"] = row["moved"]
        else:
            ref = {
                "occ": cap["area"][face],
                "fine": cap["area_fine"][face],
                "rebuilt": cap["area_3d"][face],
                "band": cap["band"][face],
            }
            special = SPECIAL.get((file, face, check))
            if special:
                entry["verdict"] = special[0]
                entry["reason"] = special[1].format(
                    occ=g(ref["occ"]), fine=g(ref["fine"]), rebuilt=g(ref["rebuilt"]), rust=g(row["rust"])
                )
            else:
                entry["verdict"], entry["reason"] = verdict(row, ref, evidence.get(f"{file}#{face}", {}))
        entry["rust"] = row["rust"]
        out.append(entry)
    OUT.write_text(json.dumps(out, indent=2, ensure_ascii=False) + "\n")
    counts: dict[str, int] = {}
    for e in out:
        counts[e["verdict"]] = counts.get(e["verdict"], 0) + 1
    print(counts, file=sys.stderr)


if __name__ == "__main__":
    main()
