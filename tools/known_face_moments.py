"""Write ``tests/fixtures/known_face_moments.json``: a verdict for every face whose
``Part::face_moments`` area or centroid differs from OpenCascade's adaptive integration, with
the evidence (upstream need U11 of specify-core-rust).

    FACE_MOMENTS_DUMP=/tmp/found.json QUIDDITY_CORPUS=../quiddity/tests/corpus \\
        cargo test --release -p haecceity --test face_moments -- face_moments_match
    FACE_MOMENTS_DUMP=/tmp/found.json FACE_MOMENTS_EVIDENCE=/tmp/evidence.json \\
        QUIDDITY_CORPUS=../quiddity/tests/corpus \\
        cargo test --release -p haecceity --test face_moments -- --ignored face_moment_evidence
    python tools/known_face_moments.py /tmp/found.json /tmp/evidence.json

The differences come from ``crates/haecceity/tests/face_moments.rs``; the references from
``tests/fixtures/face_moments.json.gz`` (``tools/capture_face_moments.py``: OpenCascade's
adaptive area and centroid, the same once the face's pcurves are projected again from the 3D
edges, and the edges' tolerance strip); the evidence from the test's ``face_moment_evidence``
(an integration independent of ``mass.rs``'s boundary walk: Green's theorem along each edge
curve sampled densely, extrapolated, with its own error estimate) and
``tests/fixtures/face_area_evidence.json.gz`` (``tools/face_area_evidence.py``'s independent
areas, by OpenCascade's geometry).

Agreement is the test's: the area within 1e-6 relative, each centroid coordinate within 1e-6 of
the larger of its magnitude and 1 mm. Verdicts, by the first rule that holds:

- the independent integration agrees with the port (to 1e-6, or three times its own error
  estimate if larger) and not with OpenCascade: rust-correct;
- OpenCascade's integration with the pcurves rebuilt from the 3D edges agrees with the port:
  rust-correct (OpenCascade integrates the region the file's pcurves bound, approximations of
  the edges; the port integrates the region the edges bound);
- the independent area agrees with the port and not with OpenCascade, and the centroids differ
  by no more than OpenCascade's extra (or missing) area at the face's far side could move them
  (|Δarea| over the area, times the diagonal of the face's box): rust-correct, on that
  evidence;
- the independent integration agrees with OpenCascade and not with the port: rust-wrong;
- ``tools/face_area_evidence.py --moments`` (the area and centroid by ``tools/kernel_evidence.py``'s
  Green's theorem along the edges' foot points, OpenCascade's evaluator and high-order panels,
  for the faces the rules above leave open) agrees with the port (to 1e-6, or three times its
  8-to-16-panel change if larger) and not with OpenCascade: rust-correct; the reverse:
  rust-wrong;
- otherwise undetermined, with all the numbers.

A face with its own reason (``SPECIAL``) has it appended to the numbers, and takes its verdict
where it gives one.
"""

from __future__ import annotations

import gzip
import json
import sys
from pathlib import Path

FIXTURES = Path(__file__).resolve().parent.parent / "tests" / "fixtures"
OUT = FIXTURES / "known_face_moments.json"
TOLERANCE = 1e-6

THIN = (
    "face_moment_evidence misses this sliver (4.4 mm long, about 1.4e-5 mm wide) by 1%: its "
    "inner integral runs from the B-spline domain's lower u, so the face's area is a small "
    "difference of large integrals whose quadrature error its estimate (the chords only) does "
    "not measure; started at the face's own lowest u it gives 6.157355e-5 on face 36, 1.2e-6 "
    "from the port."
)

SPECIAL = {
    ("cadgenbench/threaded_connector_109.step", 36): (None, THIN),
    ("cadgenbench/threaded_connector_109.step", 75): (None, THIN),
    ("cadgenbench/threaded_connector_109.step", 132): (None, THIN),
    ("cadgenbench_inputs/cgb242.step.gz", 483): (
        "equivalent",
        "The file does not define this face to the precision compared: a sliver 0.0138 mm long at "
        "the tip of a B-spline surface that nearly closes there (its sides v = 0 and v = 1 lie "
        "1.2e-5 mm apart), bounded by two edges that stray up to 2.0e-5 mm off the surface "
        "(tools/kernel_evidence.py boxes; their tolerances 1.6e-5 and 2.1e-5), so wider than the "
        "tip itself; the strip their tolerances allow the boundary is 4.98e-7 mm², nine times the "
        "face. Every reading lies inside it: the port's, the edges along the sides bounding the "
        "whole tip (5.4618e-8 by the exact surface over u < 0.0016898, every v, "
        "known_face_areas.json); the edges' foot points (5.4455e-8 at 32 and 64 panels too, "
        "change 2.7e-4: the feet of points straying farther than the tip is wide land on either "
        "side, so no refinement converges; tools/face_area_evidence.py's slices 5.4459e-8 and "
        "polygon 5.4399e-8); and OpenCascade's pcurves, a thin triangle near v = 0.042. Negligible "
        "either way (5.5e-13 of the solid's area); a single answer needs a rule for slivers "
        "narrower than their edges' tolerance, which is a maintainer decision, not evidence.",
    ),
}


def rel(a, b) -> float:
    """The test's measure of how far [area, cx, cy, cz] *a* is from *b*."""

    return max(
        [abs(a[0] - b[0]) / abs(b[0])] + [abs(a[k] - b[k]) / max(abs(b[k]), 1.0) for k in (1, 2, 3)]
    )


def fmt(m) -> str:
    return "area {:.10g}, centroid ({:.10g}, {:.10g}, {:.10g})".format(*m)


def main(dump_path: str, evidence_path: str) -> None:
    capture = {f["file"]: f for f in json.loads(gzip.decompress((FIXTURES / "face_moments.json.gz").read_bytes()))["files"]}
    areas = json.loads(gzip.decompress((FIXTURES / "face_area_evidence.json.gz").read_bytes()))["faces"]
    evidence = json.loads(Path(evidence_path).read_text())
    out = []
    counts: dict[str, int] = {}
    for row in json.loads(Path(dump_path).read_text()):
        file, face, rust = row["file"], row["face"], row["rust"]
        entry = capture[file]
        kind, occ, rebuilt = entry["types"][face], entry["fine"][face], entry["fine_3d"][face]
        head = f"{kind} face. OpenCascade's adaptive integral (eps 1e-9): {fmt(occ)}"
        if rebuilt is not None:
            head += f"; with its pcurves rebuilt from the 3D edges: {fmt(rebuilt)}"
        if rust is None:
            verdict, reason = "rust-wrong", head + ". The port gives no moments."
        else:
            head += f". The port: {fmt(rust)}."
            found = evidence[f"{file}#{face}"]
            area = areas.get(f"{file}#{face}", {})
            kernel, change = area.get("moments"), area.get("moments_change")
            area = area.get("slices") or area.get("green")
            # An area 1e-3 from both the port's and OpenCascade's is that tool's failure (it
            # unwraps a longitude across a pole, cgb242 face 790), not evidence.
            if area is not None and min(abs(area - rust[0]), abs(area - occ[0])) > 1e-3 * area:
                area = None
            verdict = "undetermined"
            reason = head
            if found["green"] is not None:
                ev, err = found["green"], found["error"]
                tol = max(TOLERANCE, 3 * err)
                said = (
                    f" An independent integration (Green's theorem along the edge curves, "
                    f"face_moment_evidence) gives {fmt(ev)} (estimated error {err:.1e}): "
                    f"{rel(rust, ev):.1e} from the port, {rel(occ, ev):.1e} from OpenCascade."
                )
                if area is not None and abs(ev[0] - area) > TOLERANCE * area:
                    # Two independent integrations disagree: neither decides.
                    reason = head + said + (
                        f" It is not used: tools/face_area_evidence.py's independent area is {area:.10g}."
                    )
                elif rel(rust, ev) <= tol < rel(occ, ev):
                    verdict, reason = "rust-correct", head + said
                elif rel(occ, ev) <= tol < rel(rust, ev):
                    verdict, reason = "rust-wrong", head + said
                elif rel(rust, ev) <= tol / 10 or 100 * rel(rust, ev) <= rel(occ, ev):
                    verdict = "rust-correct"
                    reason = head + said + (
                        " The port is a hundred times nearer it than OpenCascade, but outside 1e-6 "
                        "(a face this small is beyond what the relative test resolves)."
                        if rel(rust, ev) > tol
                        else ""
                    )
                else:
                    reason = head + said
            if verdict == "undetermined" and rebuilt is not None and rel(rust, rebuilt) <= TOLERANCE:
                verdict = "rust-correct"
                reason += (
                    f" OpenCascade with the pcurves rebuilt agrees with the port ({rel(rust, rebuilt):.1e}): "
                    "its own answer integrates the region the file's pcurves bound, approximations of "
                    "the edges the port integrates."
                )
            if verdict == "undetermined" and area:
                d_area = abs(occ[0] - rust[0])
                shift = max(abs(occ[k] - rust[k]) for k in (1, 2, 3))
                # The area one integration has and the other has not lies on the face, so it moves
                # the centroid by at most its share of the area times the face's extent.
                size = found["diagonal"]
                ok_area = abs(rust[0] - area) <= TOLERANCE * area < abs(occ[0] - area)
                if ok_area and shift <= d_area / rust[0] * size:
                    verdict = "rust-correct"
                    reason += (
                        f" tools/face_area_evidence.py's independent area {area:.10g} agrees with the "
                        f"port; OpenCascade's area differs by {d_area:.2g}, which at the face's extent "
                        f"accounts for its centroid's {shift:.2g} mm."
                    )
                elif f"{area:.10g}" not in reason:
                    reason += f" tools/face_area_evidence.py's independent area: {area:.10g}."
            if verdict == "undetermined" and kernel is not None:
                tol = max(TOLERANCE, 3 * change)
                reason += (
                    f" tools/face_area_evidence.py --moments (Green's theorem along the edges' foot "
                    f"points, tools/kernel_evidence.py's panels) gives {fmt(kernel)} (8-to-16-panel "
                    f"change {change:.1e}): {rel(rust, kernel):.1e} from the port, "
                    f"{rel(occ, kernel):.1e} from OpenCascade."
                )
                if rel(rust, kernel) <= tol < rel(occ, kernel):
                    verdict = "rust-correct"
                elif rel(occ, kernel) <= tol < rel(rust, kernel):
                    verdict = "rust-wrong"
            if (file, face) in SPECIAL:
                special, extra = SPECIAL[(file, face)]
                verdict = special or verdict
                reason += " " + extra
        counts[verdict] = counts.get(verdict, 0) + 1
        out.append({"file": file, "face": face, "verdict": verdict, "reason": reason, "rust": rust})
    OUT.write_text(json.dumps(out, indent=2) + "\n")
    print(counts, file=sys.stderr)


if __name__ == "__main__":
    main(*sys.argv[1:])
