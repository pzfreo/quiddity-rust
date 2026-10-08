"""Record the part-relative frame Python's ``quiddity.frames.infer_part_frame`` derives.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_frames.py

For every STEP fixture at the top of ``tests/fixtures`` (the golden parts among them), every
corpus file in ``tests/fixtures/corpus.json``, and the hand-built parts of Python's frame tests
(written as STEP to ``tests/fixtures/captured/frames/``), writes
``tests/fixtures/captured/frames.json``: the frame (origin, axes, gauge) or the refusal reason,
for the part as imported and for the part under the generic motion ``tests/frames.rs`` and
``tests/invariance.rs`` use (37 degrees about (1, 2, 3), then a translation), with the quiddity
revision. Every part is the re-imported STEP, as the Rust reader sees it.

    ... tools/capture_frames.py --compare recognise_risers FILE...

instead runs that recogniser on each listed corpus file's framed working part
(``prepare_framed_part(part).part``), as imported and under that motion, and prints the records
found in only one: what Python's framed recognition does with each difference
``tests/invariance.rs`` lists for the generic motion in ``known_invariance.json``.
"""

from __future__ import annotations

import dataclasses
import gzip
import json
import math
import os
import sys
import tempfile
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src")]
sys.path.insert(0, str(Path(__file__).resolve().parent))

from _provenance import revision  # noqa: E402
from build123d import Axis, Box, Cylinder, Location, Pos, Sphere, export_step  # noqa: E402
from OCP.gp import gp_Ax1, gp_Dir, gp_Pnt, gp_Trsf, gp_Vec  # noqa: E402

from quiddity import import_step_geometry  # noqa: E402
from quiddity.frames import (  # noqa: E402
    PartFrame,
    PreparedFramedPart,
    infer_part_frame,
    prepare_framed_part,
)

CORPUS = QUIDDITY / "tests" / "corpus"
FIXTURES = Path(__file__).resolve().parent.parent / "tests" / "fixtures"
OUT = FIXTURES / "captured" / "frames.json"
BUILT = FIXTURES / "captured" / "frames"

# The generic motion: 37 degrees about (1, 2, 3), then the invariance test's translation.
ANGLE_DEGREES = 37.0
AXIS = (1.0, 2.0, 3.0)
TRANSLATION = (123.456, -78.9, 41.3)


def generic_motion() -> Location:
    rotation = gp_Trsf()
    rotation.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(*AXIS)), math.radians(ANGLE_DEGREES))
    translation = gp_Trsf()
    translation.SetTranslation(gp_Vec(*TRANSLATION))
    return Location(translation.Multiplied(rotation))


def built_parts() -> dict[str, object]:
    """The parts of Python's frame tests (test_experimental_frame.py and
    test_frame_handling_prototype.py), each as the test builds it."""

    asymmetric = Box(10, 20, 30) + Pos(9, 18, 28) * Box(2, 3, 4)
    return {
        "box_rot_x30.step": Box(10, 20, 30).rotate(Axis.X, 30),
        "box_moved.step": Pos(13, -7, 5) * Box(10, 20, 30).rotate(Axis.X, 30),
        "asymmetric.step": asymmetric,
        "asymmetric_moved.step": Pos(13, -7, 5) * asymmetric.rotate(Axis.X, 30),
        "asymmetric_bored_moved.step": Pos(13, -7, 5)
        * (asymmetric - Pos(3, 4, 0) * Cylinder(1, 30)).rotate(Axis.X, 30),
        "cylinder_rot_x37.step": Cylinder(10, 30).rotate(Axis.X, 37),
        "sphere.step": Sphere(10),
    }


def load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def frame(part) -> dict[str, object]:
    found = infer_part_frame(part)
    if not isinstance(found, PartFrame):
        return {"refused": found.reason.value}
    return {
        "origin": list(found.origin),
        "x": list(found.x),
        "y": list(found.y),
        "z": list(found.z),
        "gauge": found.gauge.value,
    }


def capture() -> None:
    BUILT.mkdir(parents=True, exist_ok=True)
    for name, part in built_parts().items():
        export_step(part, str(BUILT / name))
    paths = [("built", p) for p in sorted(BUILT.glob("*.step"))]
    paths += [("fixture", p) for p in sorted(FIXTURES.glob("*.step"))]
    corpus = json.loads((FIXTURES / "corpus.json").read_text())["files"]
    paths += [("corpus", CORPUS / entry["file"]) for entry in corpus]
    motion = generic_motion()
    entries = []
    for source, path in paths:
        part = load(path)
        name = path.relative_to(CORPUS) if source == "corpus" else path.name
        entries.append(
            {
                "source": source,
                "file": str(name),
                "frame": frame(part),
                "moved": frame(motion * part),
            }
        )
        print(source, name, entries[-1]["frame"].get("gauge", entries[-1]["frame"]), flush=True)
    out = {
        "quiddity_revision": revision(QUIDDITY),
        "motion": {
            "angle_degrees": ANGLE_DEGREES,
            "axis": list(AXIS),
            "translation": list(TRANSLATION),
        },
        "parts": entries,
    }
    OUT.write_text(json.dumps(out, indent=1) + "\n")


def _plain(value: object) -> object:
    """A record as plain data, floats rounded to 1e-6 (the port's comparison tolerance)."""

    if dataclasses.is_dataclass(value) and not isinstance(value, type):
        return {f.name: _plain(getattr(value, f.name)) for f in dataclasses.fields(value)}
    if isinstance(value, dict):
        return {str(k): _plain(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_plain(v) for v in value]
    if isinstance(value, float):
        return round(value, 6) + 0.0
    if hasattr(value, "value") and not isinstance(value, (int, str)):
        return _plain(value.value)
    if isinstance(value, (int, str, bool)) or value is None:
        return value
    return repr(type(value).__name__)


def compare(function: str, files: list[str]) -> None:
    """Python's *function* (a public recogniser) on each corpus file's framed working part
    (``prepare_framed_part(...).part``), as imported and under the generic motion."""

    import quiddity

    recognise = getattr(quiddity, function)
    motion = generic_motion()
    for name in files:
        part = load(CORPUS / name)
        prepared = [prepare_framed_part(part), prepare_framed_part(motion * part)]
        if not all(isinstance(p, PreparedFramedPart) for p in prepared):
            print(name, "refused", [getattr(p, "reason", None) for p in prepared])
            continue
        gauges = [p.frame.gauge.value for p in prepared]
        records = [recognise(p.part) for p in prepared]
        plain = [sorted(json.dumps(_plain(r), sort_keys=True) for r in rs) for rs in records]
        print(f"{name} {function} gauges {gauges}: {len(plain[0])} unmoved, {len(plain[1])} moved")
        for tag, mine, other in (("only unmoved", plain[0], plain[1]), ("only moved", plain[1], plain[0])):
            for record in mine:
                if record not in other:
                    print(f"  {tag}: {record}")
        print(flush=True)


if __name__ == "__main__":
    if sys.argv[1:2] == ["--compare"]:
        compare(sys.argv[2], sys.argv[3:])
    else:
        capture()
