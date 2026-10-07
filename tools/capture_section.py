"""Record OpenCascade's section views of every corpus part, as draftwright builds them.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_section.py

For each corpus part, cuts its solids at the mid-plane ``y = centre`` exactly as draftwright's
section A–A does (``BRepAlgoAPI_Cut`` with a 1e-3 fuzzy value, removing everything at smaller
y, keeping only solids), projects the result in draftwright's front view (camera at -Y, Z up)
with ``tools/capture_hlr.py``'s projector, and records the cut faces (the faces of the result
made from the cutter's face, which draftwright hatches) as their boundary edges in world (x, z).
Writes ``tests/fixtures/section.json.gz``, leaving out the largest parts as
``tools/capture_hlr.py`` does.
"""

from __future__ import annotations

import gzip
import json
import math
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import capture_hlr  # noqa: E402  (sets up the quiddity path)

from build123d import Axis, Box, Compound, Face, Pos  # noqa: E402
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut  # noqa: E402
from OCP.TopTools import TopTools_ListOfShape  # noqa: E402

OUT = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "section.json.gz"


def _cut(body, cutter):
    """The solids kept, and the cut faces as draftwright finds them (``_cut_section``): the
    boolean's descendants of the cutter's face in the plane, less any that descend from the
    body's own faces."""

    args, tools = TopTools_ListOfShape(), TopTools_ListOfShape()
    args.Append(body.wrapped)
    tools.Append(cutter.wrapped)
    op = BRepAlgoAPI_Cut()
    op.SetArguments(args)
    op.SetTools(tools)
    op.SetFuzzyValue(1e-3)
    op.SetToFillHistory(True)
    op.Build()
    if not op.IsDone():
        return None
    solids = Compound(op.Shape()).solids()
    if not solids:
        return None
    cutting_face = cutter.faces().sort_by(Axis.Y)[-1]
    original = []
    for face in body.faces():
        original.append(face.wrapped)
        original.extend(op.Modified(face.wrapped))
    faces = [
        Face(f)
        for f in op.Modified(cutting_face.wrapped)
        if not any(f.IsSame(o) for o in original)
    ]
    return Compound(list(solids)), faces


def _boundary(face) -> list[list[list[float]]]:
    """A planar face's boundary edges, each sampled within 0.3 µm, as world (x, z) to 10 nm."""

    return [[[round(p[0], 5), round(p[2], 5)] for p in capture_hlr._polyline_3d(e)] for e in face.edges()]


def main() -> None:
    parts = []
    for path in sorted(capture_hlr.CORPUS.rglob("*")):
        if not path.name.lower().endswith((".step", ".stp", ".step.gz", ".stp.gz")):
            continue
        try:
            shape = capture_hlr._load(path)
        except Exception as error:  # noqa: BLE001
            print("skip", path, error, file=sys.stderr)
            continue
        solids = list(shape.solids())
        if not solids:
            continue
        body = Compound(solids)
        box = body.bounding_box()
        cut_y = box.center().Y
        big = 4.0 * max(box.size.X, box.size.Y, box.size.Z) + 10.0
        cutter = Pos(box.center().X, cut_y - big / 2, box.center().Z) * Box(big, big, big)
        try:
            cut = _cut(body, cutter)
        except Exception as error:  # noqa: BLE001
            print("cut failed", path, error, file=sys.stderr)
            continue
        if cut is None:
            print("cut empty", path, file=sys.stderr)
            continue
        cut, cut_faces = cut
        try:
            view = capture_hlr._project(cut, (0.0, -1.0, 0.0), (0.0, 0.0, 1.0))
        except Exception as error:  # noqa: BLE001
            print("projection failed", path, error, file=sys.stderr)
            continue
        record = {
            "file": str(path.relative_to(capture_hlr.CORPUS)),
            "cut_y": cut_y,
            "view": view,
            "cut_faces": [_boundary(f) for f in cut_faces],
            "cut_area": math.fsum(f.area for f in cut_faces),
        }
        if capture_hlr.kept(record):
            parts.append(record)
            print(record["file"], len(view["edges"]), len(cut_faces), file=sys.stderr)
    text = json.dumps({"parts": parts}, allow_nan=False) + "\n"
    OUT.write_bytes(gzip.compress(text.encode(), mtime=0))


if __name__ == "__main__":
    main()
