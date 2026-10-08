"""Evidence for the verdicts in ``tests/fixtures/known_drawings.json`` that OpenCascade can give
independently of the boolean draftwright's section view relies on, and of its hidden-line output.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/drawing_evidence.py cut <corpus file>...
    ... drawing_evidence.py ray <corpus file> <dx,dy,dz> <x,y,z>...
    ... drawing_evidence.py classify <corpus file> <x,y,z>...

``cut``: for each corpus part, at the mid-plane ``y = centre`` that ``tools/capture_section.py``
cuts:

* draftwright's cut as captured (``BRepAlgoAPI_Cut`` against a box, fuzzy value 1e-3), and the
  same cut and its complement (``BRepAlgoAPI_Common`` with the kept side's box) at other fuzzy
  values: how many solids, their volume and least y (a cut that worked keeps nothing below the
  plane);
* the plane's section of the part (``BRepAlgoAPI_Section``, no solid built): its edges sampled
  as the captures sample them, and the area they bound by the even-odd rule, as draftwright
  hatches and ``crates/haecceity/tests/common/drawing.rs`` measures (``EVIDENCE_SECTION_OUT``
  names a file to write the edges to, as world (x, z) polylines); and the sections ``EPS`` to
  either side, with the area they have in common: the material on both sides of the plane, the
  cut face, where faces of the part lie in the plane;
* OpenCascade's classifier (``BRepClass3d_SolidClassifier``) at the centres of an ``N`` by ``N``
  grid of cells across the part's box in the plane (``EVIDENCE_GRID``, 200 by default): the
  area of the cells whose centre is in the solid, with the area of the cells the section's
  edges pass through as its bound.

``ray``: every face OpenCascade's intersector meets along the ray from each point in the
direction given (towards the viewer), faces by their index (haecceity's). ``classify``:
OpenCascade's classifier at each point.

``crates/haecceity/tests/drawings.rs`` (``failed_cuts_match_their_reference``) holds the port's
side of the cut areas against the reference listed from here.
"""

from __future__ import annotations

import json
import math
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import capture_hlr  # noqa: E402  (sets up the quiddity path)
import capture_section  # noqa: E402

from build123d import Box, Compound, Edge, Pos  # noqa: E402
from OCP.BRepAlgoAPI import BRepAlgoAPI_Common, BRepAlgoAPI_Cut, BRepAlgoAPI_Section  # noqa: E402
from OCP.BRepClass3d import BRepClass3d_SolidClassifier  # noqa: E402
from OCP.gp import gp_Dir, gp_Lin, gp_Pln, gp_Pnt  # noqa: E402
from OCP.IntCurvesFace import IntCurvesFace_ShapeIntersector  # noqa: E402
from OCP.TopAbs import TopAbs_EDGE, TopAbs_FACE, TopAbs_IN, TopAbs_ON  # noqa: E402
from OCP.TopExp import TopExp, TopExp_Explorer  # noqa: E402
from OCP.TopoDS import TopoDS  # noqa: E402
from OCP.TopTools import TopTools_IndexedMapOfShape, TopTools_ListOfShape  # noqa: E402

N = int(os.environ.get("EVIDENCE_GRID", "200"))
EPS = 1e-4


def intervals(segs: list, y: float) -> list[tuple[float, float]]:
    """Where the scanline at *y* lies inside *segs* by the even-odd rule."""

    xs = sorted(
        a[0] + (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) for a, b in segs if (a[1] > y) != (b[1] > y)
    )
    return [(xs[k], xs[k + 1]) for k in range(0, len(xs) - 1, 2)]


def even_odd_area(*outlines: list) -> float:
    """The area the outlines all bound by the even-odd rule (one outline: the area it bounds,
    as ``drawing.rs`` measures it), over 4000 scanlines."""

    ys = [p[1] for segs in outlines for s in segs for p in s]
    lo, hi = min(ys), max(ys)
    n = 4000
    h = (hi - lo) / n
    area = 0.0
    for i in range(n):
        y = lo + h * (i + 0.5)
        common = intervals(outlines[0], y)
        for segs in outlines[1:]:
            other = intervals(segs, y)
            common = [
                (max(a, c), min(b, d)) for a, b in common for c, d in other if max(a, c) < min(b, d)
            ]
        area += sum(b - a for a, b in common) * h
    return area


def booleans(body: Compound, cut_y: float) -> None:
    box = body.bounding_box()
    big = 4.0 * max(box.size.X, box.size.Y, box.size.Z) + 10.0
    cutter = Pos(box.center().X, cut_y - big / 2, box.center().Z) * Box(big, big, big)
    keeper = Pos(box.center().X, cut_y + big / 2, box.center().Z) * Box(big, big, big)
    captured = capture_section._cut(body, cutter)
    if captured is None:
        print("  draftwright's cut: no solid")
    else:
        kept, faces = captured
        print(
            f"  draftwright's cut: {len(kept.solids())} solid(s), volume {kept.volume:.4f}, "
            f"least y {kept.bounding_box().min.Y:.4f}, {len(faces)} cut face(s)"
        )
    for fuzzy in (0.0, 1e-5, 1e-2):
        for label, cls, tool in (("cut", BRepAlgoAPI_Cut, cutter), ("common", BRepAlgoAPI_Common, keeper)):
            args, tools = TopTools_ListOfShape(), TopTools_ListOfShape()
            args.Append(body.wrapped)
            tools.Append(tool.wrapped)
            op = cls()
            op.SetArguments(args)
            op.SetTools(tools)
            if fuzzy:
                op.SetFuzzyValue(fuzzy)
            op.Build()
            if not op.IsDone():
                print(f"  {label} (fuzzy {fuzzy:g}): failed")
                continue
            solids = Compound(op.Shape()).solids()
            if not solids:
                print(f"  {label} (fuzzy {fuzzy:g}): no solid")
                continue
            least = min(s.bounding_box().min.Y for s in solids)
            volume = math.fsum(s.volume for s in solids)
            print(f"  {label} (fuzzy {fuzzy:g}): {len(solids)} solid(s), volume {volume:.4f}, least y {least:.4f}")


def plane_section(body: Compound, y: float) -> list:
    """The edges of the part's section by the plane at *y* (``BRepAlgoAPI_Section``, no solid
    built), sampled as the captures sample them, as world (x, z) polylines."""

    op = BRepAlgoAPI_Section(body.wrapped, gp_Pln(gp_Pnt(0.0, y, 0.0), gp_Dir(0.0, 1.0, 0.0)))
    op.Build()
    polylines = []
    explorer = TopExp_Explorer(op.Shape(), TopAbs_EDGE)
    while explorer.More():
        pts = capture_hlr._polyline_3d(Edge(TopoDS.Edge_s(explorer.Current())))
        polylines.append([(p[0], p[2]) for p in pts])
        explorer.Next()
    return polylines


def segments(polylines: list) -> list:
    return [(a, b) for pts in polylines for a, b in zip(pts, pts[1:])]


def section_area(body: Compound, cut_y: float) -> list:
    polylines = plane_section(body, cut_y)
    segs = segments(polylines)
    if os.environ.get("EVIDENCE_SECTION_OUT"):
        # The section's edges as world (x, z) polylines, as the capture records cut faces.
        rounded = [[[round(x, 5), round(z, 5)] for x, z in pts] for pts in polylines]
        Path(os.environ["EVIDENCE_SECTION_OUT"]).write_text(json.dumps(rounded))
    if segs:
        print(f"  plane section: {len(polylines)} edge(s), area {even_odd_area(segs):.4f} (even-odd)")
    else:
        print("  plane section: no edge")
    # Where the part's own faces lie in the plane, its section also has their boundaries,
    # though material lies on one side of them only. The cut face (material on both sides) is
    # what the sections a hair to either side have in common.
    either = [segments(plane_section(body, cut_y + d)) for d in (-EPS, EPS)]
    if all(either):
        print(
            f"  plane sections at y -/+ {EPS:g}: areas {even_odd_area(either[0]):.4f}, "
            f"{even_odd_area(either[1]):.4f}; in common {even_odd_area(*either):.4f}"
        )
    return segs


def classified_area(body: Compound, cut_y: float, segs: list) -> None:
    box = body.bounding_box()
    (x0, x1), (z0, z1) = (box.min.X, box.max.X), (box.min.Z, box.max.Z)
    hx, hz = (x1 - x0) / N, (z1 - z0) / N
    # The cells an edge of the section passes through: where the cell's own answer may be off.
    edge_cells = set()
    for a, b in segs:
        steps = max(1, int(math.ceil(max(abs(b[0] - a[0]) / hx, abs(b[1] - a[1]) / hz) * 2)))
        for k in range(steps + 1):
            t = k / steps
            x, z = a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t
            edge_cells.add((int((x - x0) // hx), int((z - z0) // hz)))
    inside = on = 0
    for solid in body.solids():
        classifier = BRepClass3d_SolidClassifier(solid.wrapped)
        for i in range(N):
            for j in range(N):
                p = gp_Pnt(x0 + hx * (i + 0.5), cut_y, z0 + hz * (j + 0.5))
                classifier.Perform(p, 1e-7)
                state = classifier.State()
                if state == TopAbs_IN:
                    inside += 1
                elif state == TopAbs_ON:
                    on += 1
    cell = hx * hz
    print(
        f"  classifier ({N}x{N} cells): area {inside * cell:.4f}, {on} cell(s) on the boundary, "
        f"bound ±{len(edge_cells) * cell:.4f} ({len(edge_cells)} cells crossed by the section)"
    )


def cut(names: list[str]) -> None:
    for name in names:
        shape = capture_hlr._load(capture_hlr.CORPUS / name)
        body = Compound(list(shape.solids()))
        cut_y = body.bounding_box().center().Y
        print(f"{name}: plane y = {cut_y!r}, part volume {body.volume:.4f}")
        booleans(body, cut_y)
        segs = section_area(body, cut_y)
        classified_area(body, cut_y, segs)


def ray(name: str, args: list[str]) -> None:
    """Every face OpenCascade's intersector (``IntCurvesFace_ShapeIntersector``, as its
    classifier casts) meets along the ray from each point towards the viewer: the faces by their
    index in OpenCascade's traversal order, which is haecceity's."""

    shape = capture_hlr._load(capture_hlr.CORPUS / name)
    body = Compound(list(shape.solids()))
    faces = TopTools_IndexedMapOfShape()
    TopExp.MapShapes_s(shape.wrapped, TopAbs_FACE, faces)
    direction = gp_Dir(*(float(c) for c in args[0].split(",")))
    for point in args[1:]:
        p = gp_Pnt(*(float(c) for c in point.split(",")))
        intersector = IntCurvesFace_ShapeIntersector()
        intersector.Load(body.wrapped, 1e-7)
        intersector.Perform(gp_Lin(p, direction), 0.0, 1e9)
        hits = sorted(
            (intersector.WParameter(i), faces.FindIndex(intersector.Face(i)) - 1, str(intersector.State(i)).split(".")[-1])
            for i in range(1, intersector.NbPnt() + 1)
        )
        print(f"{name} from {point} towards {args[0]}: " + (", ".join(f"t {t:.6g} face {f} ({state})" for t, f, state in hits) or "no face"))


def classify(name: str, points: list[str]) -> None:
    """OpenCascade's classifier (``BRepClass3d_SolidClassifier``, tolerance 1e-7) at each point."""

    shape = capture_hlr._load(capture_hlr.CORPUS / name)
    for point in points:
        p = gp_Pnt(*(float(c) for c in point.split(",")))
        states = []
        for solid in shape.solids():
            classifier = BRepClass3d_SolidClassifier(solid.wrapped)
            classifier.Perform(p, 1e-7)
            states.append(str(classifier.State()).split(".")[-1])
        print(f"{name} at {point}: {', '.join(states)}")


def main() -> None:
    if len(sys.argv) >= 3 and sys.argv[1] == "cut":
        cut(sys.argv[2:])
    elif len(sys.argv) >= 5 and sys.argv[1] == "ray":
        ray(sys.argv[2], sys.argv[3:])
    elif len(sys.argv) >= 4 and sys.argv[1] == "classify":
        classify(sys.argv[2], sys.argv[3:])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
