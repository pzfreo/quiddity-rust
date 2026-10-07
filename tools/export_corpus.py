"""Record the Python answers of every ported recogniser over the shared STEP corpus.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/export_corpus.py

Writes ``tests/fixtures/corpus.json``: per corpus file, a face inventory (proves the Rust reader
walks faces in OpenCascade's order) and, per recogniser and option set, Python's records. The
corpus itself stays in the quiddity checkout; ``tests/corpus.rs`` finds it through
``QUIDDITY_CORPUS`` or ``../quiddity/tests/corpus``.
"""

from __future__ import annotations

import dataclasses
import gzip
import json
import os
import sys
import tempfile
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY), str(QUIDDITY / "tests"), str(QUIDDITY / "src")]
sys.path.insert(0, str(Path(__file__).resolve().parent))

from export_fixtures import _inventory, _run  # noqa: E402

from quiddity import (  # noqa: E402
    recognise_chamfers,
    import_step_geometry,
    recognise_countersinks,
    recognise_hole_patterns,
    recognise_holes,
)

CORPUS = QUIDDITY / "tests" / "corpus"
OUT = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "corpus.json"


def _plain(value):
    if dataclasses.is_dataclass(value) and not isinstance(value, type):
        return {f.name: _plain(getattr(value, f.name)) for f in dataclasses.fields(value)}
    if isinstance(value, (list, tuple)):
        return [_plain(v) for v in value]
    return value


def _files():
    for path in sorted(CORPUS.rglob("*")):
        name = path.name.lower()
        if name.endswith((".step", ".stp", ".step.gz", ".stp.gz")):
            yield path


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def main() -> None:
    entries = []
    for path in _files():
        try:
            part = _load(path)
        except Exception as error:  # noqa: BLE001 -- record unreadable parts, don't stop
            print("skip", path, error, file=sys.stderr)
            continue
        holes = recognise_holes(part, csinks=recognise_countersinks(part))
        fillets = [
            {"options": opts, **_run(part, opts)}
            for opts in ({"include_cylindrical": True}, {"include_cylindrical": False})
        ]
        for run in fillets:
            run["result"] = run.pop("records")
        entries.append(
            {
                "file": str(path.relative_to(CORPUS)),
                "inventory": _inventory(part),
                "results": {
                    "recognise_fillets": fillets,
                    "recognise_chamfers": [
                        {"options": opts, "result": _plain(recognise_chamfers(part, **opts))}
                        for opts in ({}, {"include_planar": False})
                    ],
                    "recognise_countersinks": [
                        {"options": {}, "result": _plain(recognise_countersinks(part))}
                    ],
                    "recognise_holes": [
                        {"options": {}, "result": _plain(recognise_holes(part))},
                        {"options": {"csinks": "auto"}, "result": _plain(holes)},
                    ],
                    "recognise_hole_patterns": [
                        {"options": {"holes": "csinks auto"}, "result": _plain(recognise_hole_patterns(holes))}
                    ],
                },
            }
        )
        print(entries[-1]["file"], {k: [len(r["result"]) for r in v] for k, v in entries[-1]["results"].items()}, file=sys.stderr)
    OUT.write_text(json.dumps({"files": entries}, indent=1) + "\n")


if __name__ == "__main__":
    main()
