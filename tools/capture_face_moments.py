"""Record OpenCascade's adaptive area and centroid of every face of the parts specify-core
compares face by face (upstream need U11).

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_face_moments.py

Writes ``tests/fixtures/face_moments.json.gz``: for each file of ``FILES`` (imported as Python
quiddity does; faces in ``part.faces()`` order, the Rust reader's face indices, as
``tools/capture_face_areas.py`` asserts against the corpus inventory), per face the area and
centre of mass ``BRepGProp::SurfaceProperties`` gives with eps 1e-9 (its adaptive integration,
the reference specify-core judges ``Part::face_moments`` against), the same for a copy of the
face whose pcurves are dropped and projected again from the 3D edge curves (``null`` where
ShapeFix cannot rebuild them: evidence where the file's pcurves stray from the edges, which
bound the region the port integrates), and the strip the edges' tolerances allow the boundary
(``tools/capture_face_areas.py``'s ``band``), with the quiddity revision.
"""

from __future__ import annotations

import gzip
import json
import os
import sys
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src")]
sys.path.insert(0, str(Path(__file__).resolve().parent))

from OCP.BRep import BRep_Builder, BRep_Tool  # noqa: E402
from OCP.BRepBuilderAPI import BRepBuilderAPI_Copy  # noqa: E402
from OCP.BRepGProp import BRepGProp  # noqa: E402
from OCP.GProp import GProp_GProps  # noqa: E402
from OCP.ShapeAnalysis import ShapeAnalysis_Edge  # noqa: E402
from OCP.ShapeFix import ShapeFix_Face  # noqa: E402
from OCP.TopoDS import TopoDS  # noqa: E402

from _provenance import revision, sha256  # noqa: E402
from capture_face_areas import _band, _edges, _load  # noqa: E402

CORPUS = QUIDDITY / "tests" / "corpus"
OUT = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "face_moments.json.gz"

# specify-core's U11 parts (B-spline, torus, cylinder and sphere faces off by 1e-6 to 1e-3), and
# cgb217, whose face 29 had no moments.
FILES = [
    "cadgenbench/threaded_connector_109.step",
    "cadgenbench_inputs/cgb202.step.gz",
    "cadgenbench_inputs/cgb203.step",
    "cadgenbench_inputs/cgb207.step",
    "cadgenbench_inputs/cgb217.step.gz",
    "cadgenbench_inputs/cgb242.step.gz",
    "cadgenbench_inputs/cgb243.step.gz",
    "nist/nist_ctc_01_asme1_rd.stp",
    "nist/nist_ctc_02_asme1_rc.stp",
    "nist/nist_ctc_03_asme1_rc.stp",
    "nist/nist_ctc_04_asme1_rd.stp",
    "nist/nist_ctc_05_asme1_rd.stp",
    "nist/nist_ftc_06_asme1_rd.stp",
    "nist/nist_ftc_07_asme1_rd.stp",
    "nist/nist_ftc_08_asme1_rc.stp",
    "nist/nist_ftc_09_asme1_rd.stp",
    "nist/nist_ftc_10_asme1_rb.stp",
]


def _moments(face) -> list[float]:
    """[area, cx, cy, cz] by the adaptive integration (eps 1e-9)."""

    props = GProp_GProps()
    BRepGProp.SurfaceProperties_s(face, props, 1e-9)
    c = props.CentreOfMass()
    return [props.Mass(), c.X(), c.Y(), c.Z()]


def _moments_3d(face) -> list[float] | None:
    """[_moments] of a copy of *face* with its pcurves projected again from the edges."""

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
    return _moments(rebuilt)


def main() -> None:
    files = []
    for name in FILES:
        path = CORPUS / name
        faces = list(_load(path).faces())
        files.append(
            {
                "file": name,
                "sha256": sha256(path),
                "types": [face.geom_type.name for face in faces],
                "fine": [_moments(face.wrapped) for face in faces],
                "fine_3d": [_moments_3d(face.wrapped) for face in faces],
                "band": [_band(face.wrapped) for face in faces],
            }
        )
        print(name, len(faces), file=sys.stderr, flush=True)
    text = json.dumps({"quiddity": revision(QUIDDITY), "files": files}, allow_nan=False) + "\n"
    OUT.write_bytes(gzip.compress(text.encode(), mtime=0))


if __name__ == "__main__":
    main()
