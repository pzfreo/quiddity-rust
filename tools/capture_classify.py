"""Record OpenCascade's point classification over the shared STEP corpus.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_classify.py

For each corpus part, a deterministic set of points (seeded by the file's path): 200 uniform in
the part's bounding box, grown by a tenth, and 100 near its faces -- a point on a face (sampled
in the face's UV bounds, kept when it lies on the trimmed face) moved off it along the normal by
0.001, 0.01 or 0.1 mm to either side. Coordinates are rounded to 1e-5 mm before classifying, so
the fixture holds the exact points asked about. Each is classified by
``BRepClass3d_SolidClassifier`` on the whole imported shape at 1e-6, as ``quiddity._bevel``
asks. Writes ``tests/fixtures/classify.json.gz`` with the quiddity revision it ran against: per
file, the points and their states as one letter each (``I`` in, ``O`` out, ``N`` on, ``U``
unknown).
"""

from __future__ import annotations

import gzip
import json
import os
import random
import subprocess
import sys
import tempfile
import zlib
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src")]

from OCP.Bnd import Bnd_Box  # noqa: E402
from OCP.BRepAdaptor import BRepAdaptor_Surface  # noqa: E402
from OCP.BRepBndLib import BRepBndLib  # noqa: E402
from OCP.BRepClass import BRepClass_FaceClassifier  # noqa: E402
from OCP.BRepClass3d import BRepClass3d_SolidClassifier  # noqa: E402
from OCP.BRepLProp import BRepLProp_SLProps  # noqa: E402
from OCP.BRepTools import BRepTools  # noqa: E402
from OCP.gp import gp_Pnt, gp_Pnt2d  # noqa: E402
from OCP.TopAbs import TopAbs_IN, TopAbs_ON, TopAbs_OUT  # noqa: E402

from quiddity import import_step_geometry  # noqa: E402

CORPUS = QUIDDITY / "tests" / "corpus"
OUT = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "classify.json.gz"

BOX_POINTS = 200
FACE_POINTS = 100
OFFSETS = (1e-3, 1e-2, 1e-1)
#: The tolerance ``quiddity._bevel`` hands ``Perform``.
TOLERANCE = 1e-6


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def _round(p) -> list[float]:
    return [round(float(c), 5) for c in p]


def _box_points(shape, rng: random.Random) -> list[list[float]]:
    box = Bnd_Box()
    BRepBndLib.Add_s(shape.wrapped, box, False)
    lo = box.CornerMin()
    hi = box.CornerMax()
    lo, hi = (lo.X(), lo.Y(), lo.Z()), (hi.X(), hi.Y(), hi.Z())
    grow = [0.05 * (h - l) for l, h in zip(lo, hi)]
    return [
        _round(rng.uniform(l - g, h + g) for l, h, g in zip(lo, hi, grow))
        for _ in range(BOX_POINTS)
    ]


def _face_points(shape, rng: random.Random) -> list[list[float]]:
    faces = list(shape.faces())
    out: list[list[float]] = []
    tries = 0
    while faces and len(out) < FACE_POINTS and tries < 20 * FACE_POINTS:
        tries += 1
        face = faces[rng.randrange(len(faces))].wrapped
        u0, u1, v0, v1 = BRepTools.UVBounds_s(face)
        u, v = rng.uniform(u0, u1), rng.uniform(v0, v1)
        if BRepClass_FaceClassifier(face, gp_Pnt2d(u, v), 1e-9).State() != TopAbs_IN:
            continue
        props = BRepLProp_SLProps(BRepAdaptor_Surface(face), u, v, 1, 1e-9)
        if not props.IsNormalDefined():
            continue
        p, n = props.Value(), props.Normal()
        offset = rng.choice(OFFSETS) * rng.choice((-1.0, 1.0))
        out.append(_round((p.X() + offset * n.X(), p.Y() + offset * n.Y(), p.Z() + offset * n.Z())))
    return out


def _state(classifier, p: list[float]) -> str:
    classifier.Perform(gp_Pnt(*p), TOLERANCE)
    state = classifier.State()
    if state == TopAbs_IN:
        return "I"
    if state == TopAbs_OUT:
        return "O"
    if state == TopAbs_ON:
        return "N"
    return "U"


def main() -> None:
    revision = subprocess.run(
        ["git", "-C", str(QUIDDITY), "rev-parse", "HEAD"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    files = []
    for path in sorted(CORPUS.rglob("*")):
        if not path.name.lower().endswith((".step", ".stp", ".step.gz", ".stp.gz")):
            continue
        name = str(path.relative_to(CORPUS))
        try:
            shape = _load(path)
        except Exception as error:  # noqa: BLE001
            print("skip", name, error, file=sys.stderr)
            continue
        rng = random.Random(zlib.crc32(name.encode()))
        points = _box_points(shape, rng) + _face_points(shape, rng)
        classifier = BRepClass3d_SolidClassifier(shape.wrapped)
        states = "".join(_state(classifier, p) for p in points)
        files.append({"file": name, "points": points, "states": states})
        print(name, len(points), {s: states.count(s) for s in "IONU"}, file=sys.stderr)
    document = {"quiddity_revision": revision, "tolerance": TOLERANCE, "files": files}
    text = json.dumps(document, allow_nan=False, separators=(",", ":")) + "\n"
    OUT.write_bytes(gzip.compress(text.encode(), mtime=0))


if __name__ == "__main__":
    main()
