"""Record every volume probe the Python recognisers ask over the shared STEP corpus.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_probes.py

Runs the aggregate recognition (``build_recognition_result``) on each corpus part with
``quiddity._volume_probe._measure`` wrapped, and writes ``tests/fixtures/probes.json.gz``: per
distinct question, the corpus file, the indices of the solids asked about, the probe exported as
an axis-aligned box's corners or, for any other shape, its STEP text (so it crosses
exactly), the recogniser that asked, and the
volume Python answered. Thin walls and interior voids are skipped: they ask no probes and are
most of the run's time.
"""

from __future__ import annotations

import gzip
import inspect
import json
import os
import sys
import tempfile
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src")]

from build123d import export_step  # noqa: E402

import quiddity._volume_probe as volume_probe  # noqa: E402
import quiddity.interior_voids as interior_voids  # noqa: E402
import quiddity.thin_walls as thin_walls  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402
from quiddity.result import build_recognition_result  # noqa: E402

CORPUS = QUIDDITY / "tests" / "corpus"
OUT = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "probes.json.gz"

#: Modules a probe passes through on its way from the recogniser that asked it.
PLUMBING = {"quiddity._volume_probe", "quiddity._solid_properties"}

_current: dict = {}
_records: dict[tuple, dict] = {}


def _caller() -> str:
    for frame in inspect.stack()[2:]:
        module = frame.frame.f_globals.get("__name__", "")
        if module.startswith("quiddity.") and module not in PLUMBING:
            return module.removeprefix("quiddity.")
    return "?"


def _solid_indices(part) -> list[int]:
    bodies = volume_probe.probe_solids(part)
    solids = _current["solids"]
    return [
        next(i for i, s in enumerate(solids) if s.wrapped.IsSame(body.wrapped)) for body in bodies
    ]


def _box(probe) -> list[float] | None:
    """An axis-aligned box probe as its two corners; ``None`` for any other shape."""

    corners = [tuple(v) for v in probe.vertices()]
    if len(probe.faces()) != 6 or len(corners) != 8:
        return None
    if any(len({c[axis] for c in corners}) != 2 for axis in range(3)):
        return None
    low = [min(c[axis] for c in corners) for axis in range(3)]
    high = [max(c[axis] for c in corners) for axis in range(3)]
    # Six faces and eight corners also describe an obround prism; only a box fills its corners.
    size = (high[0] - low[0]) * (high[1] - low[1]) * (high[2] - low[2])
    if abs(size - float(probe.volume)) > 1e-9 * size:
        return None
    return low + high


def _step(probe) -> str:
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        export_step(probe, tmp.name)
        return Path(tmp.name).read_text()


_real_measure = volume_probe._measure


def _measure(part, probe, geometry, memo):
    answer = _real_measure(part, probe, geometry, memo)
    try:
        solids = _solid_indices(part)
        key = (_current["file"], tuple(solids), repr(sorted(tuple(v) for v in probe.vertices())),
               len(probe.faces()), float(probe.volume))
        if key not in _records:
            _records[key] = {
                "file": _current["file"],
                "solids": solids,
                "caller": _caller(),
                "probe_volume": float(probe.volume),
                "volume": answer,
                **({"box": box} if (box := _box(probe)) else {"step": _step(probe)}),
            }
    except Exception as error:  # noqa: BLE001 -- a probe we cannot record is reported, not fatal
        print("unrecorded probe", _current["file"], error, file=sys.stderr)
    return answer


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def main() -> None:
    volume_probe._measure = _measure
    thin_walls._discover_thin_wall_bodies = lambda *a, **k: []
    interior_voids._discover_interior_voids = lambda *a, **k: []
    for path in sorted(CORPUS.rglob("*")):
        if not path.name.lower().endswith((".step", ".stp", ".step.gz", ".stp.gz")):
            continue
        try:
            part = _load(path)
        except Exception as error:  # noqa: BLE001
            print("skip", path, error, file=sys.stderr)
            continue
        _current.update(file=str(path.relative_to(CORPUS)), solids=list(part.solids()))
        before = len(_records)
        try:
            build_recognition_result(part)
        except Exception as error:  # noqa: BLE001
            print("recognition failed", path, error, file=sys.stderr)
        print(_current["file"], len(_records) - before, file=sys.stderr)
    text = json.dumps({"probes": list(_records.values())}, allow_nan=False) + "\n"
    OUT.write_bytes(gzip.compress(text.encode(), mtime=0))


if __name__ == "__main__":
    main()
