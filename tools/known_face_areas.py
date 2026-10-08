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
        "rust-correct",
        "tools/face_area_evidence.py gives 28.16983104 by Green's theorem and by direct slices; the "
        "port gives {rust}. It gave 28.16978113 while its boundary walk split edge 89 at the knot "
        "line u = 0.25 instead of at the corner 1.2e-5 before it, where the foot point leaves the "
        "side v = 1 (the edge dips 1.6e-3 mm into the face from outside it): the step off the side "
        "lay in a panel's last sliver, short of its last node, and the path did not close in v by "
        "2.2e-6 (crates/haecceity/src/mass.rs now finds the corner first). OpenCascade gives {occ} "
        "(fixed rule), {fine} (adaptive) and {rebuilt} (pcurves projected from the edges).",
    ),
    ("cadgenbench_inputs/cgb207.step", 125, "occ"): (
        "rust-correct",
        "tools/kernel_evidence.py faces gives 47.45337973 (32 and 64 panels per interval; change "
        "8.6e-7) and tools/face_area_evidence.py 47.45338085 (Green's theorem) and 47.45337956 "
        "(direct slices); the port gives {rust}, 1.7e-7 from the first. It was 4.95e-5 over while "
        "crates/haecceity/src/geom.rs Curve::parameter took the eccentric angle of a point off an "
        "ellipse for its parameter: the face's ellipse edge (edge 360, shared with face 154) "
        "starts at a vertex 0.012 mm off the ellipse, whose foot is at 3.988586 (OpenCascade's "
        "edge range). OpenCascade gives {occ} (fixed rule), {fine} (adaptive) and {rebuilt} "
        "(pcurves projected from the edges).",
    ),
    ("cadgenbench_inputs/cgb242.step.gz", 483, "occ"): (
        "rust-wrong",
        "A degenerate seam-to-seam sliver: OpenCascade bounds it by one B-spline edge used twice and "
        "a degenerate edge (a triangulation of it has area 1e-21; face.area {occ}, adaptive {fine}). "
        "The port reads its two edges as the two sides of a closed v range and integrates the cap "
        "between them, {rust}; negligible, but not the face's area.",
    ),
    ("cadgenbench_inputs/cgb202.step.gz", 397, "occ"): (
        "rust-correct",
        "The port gives 8.084128851. tools/kernel_evidence.py faces (the exact surface integrated"
        " over the region the 3D edges bound by Green's theorem along the edges' foot points, "
        "12-point Gauss-Lobatto panels) gives 8.084128744 (its 8-to-16-panel change 2.4e-6; the "
        "edges' tolerances reach 7.3e-3 mm), and tools/face_area_evidence.py's direct slices "
        "8.08412885: the port to 1.3e-8 and 1e-10. Its polygon Green's theorem, 8.084148605, is "
        "the outlier (2.4e-6, its tabulated inner integral). OpenCascade gives face.area "
        "8.088062341, adaptive 8.084112104 and 8.084097068 with the pcurves projected again from "
        "the edges (2e-6 to 5e-4 off).",
    ),
    ("cadgenbench_inputs/cgb203.step", 117, "occ"): (
        "rust-correct",
        "A spherical lune: the face runs pole to pole between two great circles through the poles"
        " (each of its four arcs has the sphere's centre and radius 2), whose longitudes "
        "tools/kernel_evidence.py uv gives as -0.0193861 and 3.0094160, 3.0288021 apart; its area"
        " 2r²Δ = 8 x 3.0288021 = 24.2304171 is the port's 24.230417097, and "
        "tools/kernel_evidence.py faces gives 24.230417097002 (the port's to 1.3e-15). "
        "tools/face_area_evidence.py's 26.03506536 is the complementary lune, 8 x (2π - "
        "3.0288021): it unwraps the longitude across the pole, where it is arbitrary. OpenCascade"
        " gives face.area and adaptive 24.20589821 (1e-3 off) and 24.2304171 with the pcurves "
        "projected again from the edges.",
    ),
    ("cadgenbench_inputs/cgb207.step", 154, "occ"): (
        "rust-correct",
        "A planar face bounded by two lines and an ellipse arc (edge 360) whose vertices lie up "
        "to 0.012 mm off it. tools/kernel_evidence.py faces gives 0.7055622740 over the region the "
        "edges bound between their feet on the curves (32-to-64-panel change 1e-13), and a "
        "200001-point polygon in the plane the same to 1e-11; the port gives {rust}, the same to "
        "3e-13, now that crates/haecceity/src/geom.rs Curve::parameter places a point off an "
        "ellipse at its foot (3.988586 for the vertex 0.0119 mm off it, OpenCascade's edge range, "
        "where its eccentric angle, 3.987837, left the face 4.0e-5 short). OpenCascade gives "
        "{occ} with every rule (its pcurves).",
    ),
    ("cadgenbench_inputs/cgb242.step.gz", 69, "occ"): (
        "rust-correct",
        "The port gives 6.23700257; tools/face_area_evidence.py's direct slices (no Green's "
        "theorem, no tabulation) give 6.23700257, the port's to 1e-10. tools/kernel_evidence.py "
        "faces gives 6.2370296, 6.2369907 and 6.2370074 with 8/16, 16/32 and 32/64 panels (the "
        "edges' tolerances reach 0.018 mm): its polynomial panels do not converge on this "
        "boundary, scattering by 4e-6 about the port's value, as tools/face_area_evidence.py's "
        "polygon Green's theorem (6.236970847) does. OpenCascade gives face.area 6.237775961, "
        "adaptive 6.236954501 and 6.236955256 with the pcurves projected again from the edges "
        "(7.6e-6 below).",
    ),
    ("cadgenbench_inputs/cgb242.step.gz", 790, "occ"): (
        "rust-correct",
        "Analytic: a quarter of a unit hemisphere (longitude π to 2π, latitude 0 to π/2) less the"
        " spherical triangle cut off by a great circle (its arc has the sphere's centre and "
        "radius) from the equator at longitude 3π/2 to the meridian at 2π, which it meets at "
        "latitude 1.4127315469 (the edge's end point). That point of the equator is the pole of "
        "the meridian's plane, so the triangle's angles are π/2, π/2 and 1.4127315469 and its "
        "area (Girard) is 1.4127315469: the face is π - 1.4127315469 = 1.7288611067, the port's "
        "1.728861107 to 1e-12. tools/kernel_evidence.py faces gives the same to 1.3e-15. "
        "tools/face_area_evidence.py's 4.5543242 unwraps the longitude across the pole. "
        "OpenCascade gives face.area 1.728857251 and 1.728856813 adaptive and with the pcurves "
        "projected again (2.2e-6 to 2.5e-6 low).",
    ),
    ("cadgenbench_inputs/cgb242.step.gz", 822, "occ"): (
        "rust-correct",
        "Analytic: face 790's mirror image on the sphere centred 17 mm away along x (the same "
        "quarter hemisphere of radius 1 less the triangle a great circle cuts off from the "
        "equator to latitude 1.4127315469), so π - 1.4127315469 = 1.7288611067, the port's "
        "1.728861107 to 1e-12; tools/kernel_evidence.py faces gives the same to 5e-16. "
        "tools/face_area_evidence.py's 4.5543242 unwraps the longitude across the pole. "
        "OpenCascade gives face.area 1.728856893 and 1.728856813 adaptive and with the pcurves "
        "projected again (2.5e-6 low).",
    ),
    ("cadgenbench_inputs/cgb243.step.gz", 225, "occ"): (
        "rust-correct",
        "The port gives 1456.702593448; tools/kernel_evidence.py faces (Green's theorem along the"
        " edges' foot points, Gauss-Lobatto panels) gives 1456.702593447, 4.7e-13 apart (its "
        "8-to-16-panel change 8e-11). tools/face_area_evidence.py's polygon Green's theorem, "
        "1456.69016, is 8.5e-6 off (its tabulated inner integral on a face this large; its slices"
        " were not recorded for this face). OpenCascade gives face.area 1455.444739, adaptive "
        "1456.697361 and 1456.69736 with the pcurves projected again from the edges (3.6e-6 low).",
    ),
    ("cadgenbench_inputs/cgb243.step.gz", 233, "occ"): (
        "rust-correct",
        "The port gives 1194.066369150; tools/kernel_evidence.py faces gives 1194.066367891, "
        "1.1e-9 apart (its 8-to-16-panel change 1.3e-6). tools/face_area_evidence.py's polygon "
        "Green's theorem, 1194.056177, is 8.5e-6 off (its tabulated inner integral; its slices "
        "were not recorded for this face). OpenCascade gives face.area 1193.035367, adaptive "
        "1194.062166 and 1194.062079 with the pcurves projected again from the edges (3.5e-6 "
        "low).",
    ),
    ("cadgenbench_inputs/cgb243.step.gz", 234, "occ"): (
        "rust-correct",
        "The port gives 1194.012578; tools/kernel_evidence.py faces gives 1194.012565, 1.1e-8 "
        "apart, within its 8-to-16-panel change (3.4e-5). tools/face_area_evidence.py's polygon "
        "Green's theorem gives 1194.010347 (1.9e-6; its slices were not recorded for this face). "
        "OpenCascade gives face.area 1194.290029, adaptive 1194.304844 and 1193.959579 with the "
        "pcurves projected again from the edges (2.4e-4 and 4.5e-5 off).",
    ),
    ("cadgenbench_inputs/cgb243.step.gz", 238, "occ"): (
        "rust-correct",
        "The port gives 955.2531626877; tools/kernel_evidence.py faces gives 955.2531626875, "
        "1.5e-13 apart (its 8-to-16-panel change 7e-11). tools/face_area_evidence.py's polygon "
        "Green's theorem, 955.2507191, is 2.6e-6 off (its tabulated inner integral; its slices "
        "were not recorded for this face). OpenCascade gives face.area 954.1343023, adaptive "
        "955.2454773 and 955.2454773 with the pcurves projected again from the edges (8.0e-6 "
        "low).",
    ),
    ("cadgenbench_inputs/cgb243.step.gz", 337, "occ"): (
        "rust-correct",
        "The port gives 201.3538125637; tools/kernel_evidence.py faces gives 201.3538125637, "
        "2.8e-13 apart (its 8-to-16-panel change 1.3e-11). tools/face_area_evidence.py's polygon "
        "Green's theorem, 201.3533061, is 2.5e-6 off (its tabulated inner integral; its slices "
        "were not recorded for this face). OpenCascade gives face.area 201.1174989, adaptive "
        "201.3521879 and 201.352191 with the pcurves projected again from the edges (8.1e-6 low).",
    ),
    ("nist/nist_ctc_05_asme1_rd.stp", 79, "occ"): (
        "rust-correct",
        "The port gives 3.707685937; tools/face_area_evidence.py's direct slices give 3.707685937"
        " (1e-10) and tools/kernel_evidence.py faces 3.707685791 (3.9e-8, its 8-to-16-panel "
        "change 1.5e-11), a residual far inside the strip the edges' tolerances (up to 0.026 mm) "
        "allow the boundary, which also has a 0.018 mm gap at a degenerate edge with distinct "
        "ends that each method closes straight in its own parameters. "
        "tools/face_area_evidence.py's polygon Green's theorem gives 3.707672064 (3.7e-6). "
        "OpenCascade gives face.area 3.707306489, adaptive 3.707115934 and 2.060052229 with the "
        "pcurves projected again from the edges.",
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
