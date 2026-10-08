"""Record OpenCascade's hidden-line projection of every corpus part, as draftwright draws it.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_hlr.py

For each corpus part and each of draftwright's views (front, plan and side cameras from its
builder, and an isometric), runs ``HLRBRep_Algo`` exactly as build123d's
``project_to_viewport`` does, but keeps each output compound apart: sharp edges, smooth (G1)
edges and silhouettes ("outlines"), visible and hidden. Writes ``tests/fixtures/hlr.json.gz``:
per part and view, the camera, and each projected edge as a polyline in view coordinates. Only
the part's solids are projected.

A part whose record would take more than ``LIMIT`` bytes compressed (the fifteen largest models,
some 27 MB between them) is left out of the committed fixture; ``CAPTURE_ALL=1``
keeps every part, for a local run of ``crates/haecceity/examples/hlr_compare.rs``.
"""

from __future__ import annotations

import gzip
import json
import math
import os
import sys
import tempfile
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src")]

from build123d import Compound, Edge, GeomType  # noqa: E402
from OCP.BRepLib import BRepLib  # noqa: E402
from OCP.gp import gp_Ax1, gp_Ax2, gp_Dir, gp_Pnt  # noqa: E402
from OCP.HLRAlgo import HLRAlgo_Projector  # noqa: E402
from OCP.HLRBRep import HLRBRep_Algo, HLRBRep_HLRToShape  # noqa: E402
from OCP.TopAbs import TopAbs_EDGE  # noqa: E402
from OCP.TopExp import TopExp_Explorer  # noqa: E402
from OCP.TopoDS import TopoDS  # noqa: E402

from _provenance import revision  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402

CORPUS = QUIDDITY / "tests" / "corpus"
OUT = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "hlr.json.gz"
LIMIT = 500_000


def kept(record: dict) -> bool:
    """Whether *record* goes in the fixture: everything under ``CAPTURE_ALL``, else only a
    record within ``LIMIT`` compressed."""

    if os.environ.get("CAPTURE_ALL"):
        return True
    size = len(gzip.compress(json.dumps(record, allow_nan=False).encode(), mtime=0))
    if size > LIMIT:
        print("left out", record["file"], size, file=sys.stderr)
    return size <= LIMIT

#: draftwright's views: direction from the part's centre to the camera, and the up vector.
VIEWS = {
    "front": ((0.0, -1.0, 0.0), (0.0, 0.0, 1.0)),
    "plan": ((0.0, 0.0, 1.0), (0.0, 1.0, 0.0)),
    "side": ((1.0, 0.0, 0.0), (0.0, 0.0, 1.0)),
    "iso": ((1.0, -1.0, 1.0), (0.0, 0.0, 1.0)),
}
#: Output compounds by (class, visible).
KINDS = {
    ("sharp", True): "VCompound",
    ("smooth", True): "Rg1LineVCompound",
    ("outline", True): "OutLineVCompound",
    ("sharp", False): "HCompound",
    ("smooth", False): "Rg1LineHCompound",
    ("outline", False): "OutLineHCompound",
}


def _polyline_3d(edge: Edge) -> list[tuple[float, float, float]]:
    """An edge's points, its chords within 0.3 µm of it: a line by its ends, a curve halved
    wherever its middle strays further from the chord."""

    def at(t):
        p = edge.position_at(t)
        return (p.X, p.Y, p.Z)

    if edge.geom_type == GeomType.LINE:
        ts = [0.0, 1.0]
    else:
        ts = [i / 16 for i in range(17)]
        for _ in range(14):
            refined, split = [ts[0]], False
            for a, b in zip(ts, ts[1:]):
                pa, pb, pm = at(a), at(b), at(0.5 * (a + b))
                if math.dist([(x + y) / 2 for x, y in zip(pa, pb)], pm) > 3e-4:
                    refined.append(0.5 * (a + b))
                    split = True
                refined.append(b)
            ts = refined
            if not split:
                break
    return [at(t) for t in ts]


def _polyline(edge: Edge) -> list[list[float]]:
    """The projected edge (in the view's xy plane), sampled as `_polyline_3d`, to 10 nm (a
    hundredth of the comparison's tolerance)."""

    return [[round(x, 5), round(y, 5)] for x, y, _ in _polyline_3d(edge)]


def _project(shape, direction, up) -> dict:
    centre = shape.bounding_box().center()
    box = shape.bounding_box()
    distance = 2.0 * math.sqrt(box.size.X**2 + box.size.Y**2 + box.size.Z**2) + 1.0
    norm = math.sqrt(sum(c * c for c in direction))
    d = [c / norm for c in direction]
    camera = [centre.X + d[0] * distance, centre.Y + d[1] * distance, centre.Z + d[2] * distance]
    axes = gp_Ax2()
    axes.SetAxis(gp_Ax1(gp_Pnt(*camera), gp_Dir(*d)))
    axes.SetYDirection(gp_Dir(*up))
    projector = HLRAlgo_Projector(axes)
    # The view's map, pinned: where the projector sends the centre and the unit axes from it.
    def image(p):
        x, y, _ = projector.Project(gp_Pnt(*p))
        return [x, y]

    origin = [centre.X, centre.Y, centre.Z]
    frame = [image(origin)] + [
        image([origin[i] + (1.0 if i == k else 0.0) for i in range(3)]) for k in range(3)
    ]
    algo = HLRBRep_Algo()
    algo.Add(shape.wrapped)
    algo.Projector(projector)
    algo.Update()
    algo.Hide()
    shapes = HLRBRep_HLRToShape(algo)
    edges = []
    for (kind, visible), method in KINDS.items():
        compound = getattr(shapes, method)()
        if compound.IsNull():
            continue
        explorer = TopExp_Explorer(compound, TopAbs_EDGE)
        while explorer.More():
            raw = TopoDS.Edge_s(explorer.Current())
            # As build123d does: HLR's edges carry only 2D curves until given 3D ones.
            BRepLib.BuildCurves3d_s(raw, 1e-6)
            edge = Edge(raw)
            try:
                points = _polyline(edge)
            except Exception:  # noqa: BLE001 -- an edge HLR leaves without 3D curve
                points = None
            if points:
                edges.append({"kind": kind, "visible": visible, "points": points})
            explorer.Next()
    return {"camera": camera, "direction": d, "up": list(up), "centre": origin, "frame": frame, "edges": edges}


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def main() -> None:
    parts = []
    for path in sorted(CORPUS.rglob("*")):
        if not path.name.lower().endswith((".step", ".stp", ".step.gz", ".stp.gz")):
            continue
        try:
            shape = _load(path)
        except Exception as error:  # noqa: BLE001
            print("skip", path, error, file=sys.stderr)
            continue
        # The solids alone, as draftwright draws them: loose construction edges (NIST datum
        # lines) would project too, and draftwright discards them.
        solids = list(shape.solids())
        if solids:
            shape = Compound(solids)
        views = {}
        for name, (direction, up) in VIEWS.items():
            try:
                views[name] = _project(shape, direction, up)
            except Exception as error:  # noqa: BLE001
                print("view failed", path, name, error, file=sys.stderr)
        record = {"file": str(path.relative_to(CORPUS)), "views": views}
        if kept(record):
            parts.append(record)
            print(record["file"], {k: len(v["edges"]) for k, v in views.items()}, file=sys.stderr)
    text = json.dumps({"quiddity_revision": revision(QUIDDITY), "parts": parts}, allow_nan=False) + "\n"
    OUT.write_bytes(gzip.compress(text.encode(), mtime=0))


if __name__ == "__main__":
    main()
