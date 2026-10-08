"""Record every support-coverage question (``covered_patch``) the Python recognisers ask over the
shared STEP corpus.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_patches.py

Runs the aggregate recognition on each corpus part with ``covered_patch`` wrapped wherever it is
imported, and writes ``tests/fixtures/patches.json.gz``: per question, the corpus file, the patch
and its supports as STEP text, the module that asked, and Python's answer.
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

import quiddity  # noqa: E402
import quiddity._support_patches as support_patches  # noqa: E402
import quiddity.interior_voids as interior_voids  # noqa: E402
import quiddity.thin_walls as thin_walls  # noqa: E402
from _provenance import revision  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402
from quiddity.result import build_recognition_result  # noqa: E402

CORPUS = QUIDDITY / "tests" / "corpus"
OUT = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "patches.json.gz"

_current: dict = {}
_records: list[dict] = []


def _step(shape) -> str:
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        export_step(shape, tmp.name)
        return Path(tmp.name).read_text()


def _wrap(real):
    def covered_patch(patch, supports):
        answer = real(patch, supports)
        caller = inspect.stack()[1].frame.f_globals.get("__name__", "?").removeprefix("quiddity.")
        _records.append(
            {
                "file": _current["file"],
                "caller": caller,
                "covered": answer,
                "patch": _step(patch),
                "supports": [_step(s) for s in supports],
            }
        )
        return answer

    return covered_patch


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def main() -> None:
    real = support_patches.covered_patch
    for module in list(sys.modules.values()):
        if getattr(module, "__name__", "").startswith("quiddity") and getattr(module, "covered_patch", None) is real:
            module.covered_patch = _wrap(real)
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
        _current["file"] = str(path.relative_to(CORPUS))
        before = len(_records)
        try:
            build_recognition_result(part)
        except Exception as error:  # noqa: BLE001
            print("recognition failed", path, error, file=sys.stderr)
        print(_current["file"], len(_records) - before, file=sys.stderr)
    text = json.dumps({"quiddity_revision": revision(QUIDDITY), "patches": _records}, allow_nan=False) + "\n"
    OUT.write_bytes(gzip.compress(text.encode(), mtime=0))


if __name__ == "__main__":
    main()
