"""Record OpenCascade's area of every face of every part in the shared STEP corpus.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_face_areas.py

For each file in ``tests/fixtures/corpus.json``, imports the part as Python quiddity does and
writes ``tests/fixtures/face_areas.json.gz``: per face, in ``part.faces()`` order (OpenCascade's
traversal, the order of the Rust reader's face indices, as the corpus inventory proves), the
area ``BRepGProp::SurfaceProperties`` gives (what ``face.area`` returns, so what Python's
recognisers read), the same integration driven to a relative error of 1e-9 (the adaptive rule,
evidence where the fixed rule is coarse), and that adaptive area of a copy of the face whose
pcurves are dropped and projected again from the 3D edge curves (evidence where the file's or
the import's pcurves stray from the edges they stand for; ``null`` where ShapeFix cannot
rebuild them), and the strip the edges' tolerances allow the boundary (the sum over the face's
edge uses of length times tolerance: two readings of the face that differ by less are both
within what the file declares), with the quiddity revision.
"""

from __future__ import annotations

import gzip
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src")]

from OCP.BRep import BRep_Builder, BRep_Tool  # noqa: E402
from OCP.BRepAdaptor import BRepAdaptor_Curve  # noqa: E402
from OCP.BRepBuilderAPI import BRepBuilderAPI_Copy  # noqa: E402
from OCP.GCPnts import GCPnts_AbscissaPoint  # noqa: E402
from OCP.BRepGProp import BRepGProp  # noqa: E402
from OCP.GProp import GProp_GProps  # noqa: E402
from OCP.ShapeAnalysis import ShapeAnalysis_Edge  # noqa: E402
from OCP.ShapeFix import ShapeFix_Face  # noqa: E402
from OCP.TopAbs import TopAbs_EDGE  # noqa: E402
from OCP.TopExp import TopExp_Explorer  # noqa: E402
from OCP.TopoDS import TopoDS  # noqa: E402

from quiddity import import_step_geometry  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent))
from export_fixtures import _inventory  # noqa: E402

CORPUS = QUIDDITY / "tests" / "corpus"
FIXTURES = Path(__file__).resolve().parent.parent / "tests" / "fixtures"
OUT = FIXTURES / "face_areas.json.gz"


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def _area(face, eps: float | None = None) -> float:
    props = GProp_GProps()
    if eps is None:
        BRepGProp.SurfaceProperties_s(face, props)
    else:
        BRepGProp.SurfaceProperties_s(face, props, eps)
    return props.Mass()


def _edges(face):
    explorer = TopExp_Explorer(face, TopAbs_EDGE)
    while explorer.More():
        yield TopoDS.Edge_s(explorer.Current())
        explorer.Next()


def _band(face) -> float:
    """Each edge use's length times the edge's tolerance, summed over the face."""

    return sum(
        GCPnts_AbscissaPoint.Length_s(BRepAdaptor_Curve(edge)) * BRep_Tool.Tolerance_s(edge)
        for edge in _edges(face)
        if not BRep_Tool.Degenerated_s(edge)
    )


def _area_3d(face) -> float | None:
    """The adaptive area of a copy of *face* with its pcurves projected again from the edges."""

    copy = TopoDS.Face_s(BRepBuilderAPI_Copy(face, True, False).Shape())
    builder = BRep_Builder()
    for edge in _edges(copy):
        tolerance = BRep_Tool.Tolerance_s(edge)
        if BRep_Tool.IsClosed_s(edge, copy):
            builder.UpdateEdge(edge, None, None, copy, tolerance)
        else:
            builder.UpdateEdge(edge, None, copy, tolerance)
    fix = ShapeFix_Face(copy)
    fix.Perform()
    rebuilt = fix.Face()
    analysis = ShapeAnalysis_Edge()
    if not all(analysis.HasPCurve(edge, rebuilt) for edge in _edges(rebuilt)):
        return None
    return _area(rebuilt, 1e-9)


def main() -> None:
    corpus = json.loads((FIXTURES / "corpus.json").read_text())
    revision = subprocess.run(
        ["git", "-C", str(QUIDDITY), "rev-parse", "HEAD"], capture_output=True, text=True, check=True
    ).stdout.strip()
    files = []
    for entry in corpus["files"]:
        part = _load(CORPUS / entry["file"])
        faces = list(part.faces())
        # The traversal must be the inventory's, which the Rust reader is checked against face by
        # face (tests/corpus.rs): the same kinds, edge counts and bounds in the same order.
        assert _inventory(part) == entry["inventory"], entry["file"]
        files.append(
            {
                "file": entry["file"],
                "types": [face.geom_type.name for face in faces],
                "area": [_area(face.wrapped) for face in faces],
                "area_fine": [_area(face.wrapped, 1e-9) for face in faces],
                "area_3d": [_area_3d(face.wrapped) for face in faces],
                "band": [_band(face.wrapped) for face in faces],
            }
        )
        print(entry["file"], len(faces), file=sys.stderr)
    text = json.dumps({"quiddity": revision, "files": files}, allow_nan=False) + "\n"
    OUT.write_bytes(gzip.compress(text.encode(), mtime=0))


if __name__ == "__main__":
    main()
