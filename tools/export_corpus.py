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

from export_fixtures import _face_index, _inventory  # noqa: E402

from quiddity._adjacency import FaceGraph  # noqa: E402
from quiddity._candidates import FamilyId  # noqa: E402
from quiddity._claims import ClaimLedger  # noqa: E402
from quiddity.bosses import _discover_bosses  # noqa: E402
from quiddity.chamfers import _discover_chamfers  # noqa: E402
from quiddity.countersinks import _discover_countersinks  # noqa: E402
from quiddity.fillets import _discover_fillets  # noqa: E402
from quiddity.holes import _discover_holes  # noqa: E402

from quiddity import (  # noqa: E402
    recognise_bosses,
    recognise_fillets,
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


def _evidence(part, family, discover) -> dict:
    """Python's defining faces per record (indices into ``part.faces()``), or its refusal."""

    ledger = ClaimLedger(FaceGraph(part))
    try:
        discover(ledger)
    except ValueError as error:
        return {"evidence_error": str(error)}
    return {
        "defining": [
            sorted(_face_index(part, ledger.graph.face(node)) for node in ledger.defining_of(c))
            for c in ledger.candidate_set(family).candidates
        ]
    }


def _run(part, function, family, recognise, discover, options: dict) -> dict:
    return {
        "options": options,
        "result": _plain(recognise(part, options)),
        **_evidence(part, family, lambda ledger: discover(part, ledger, options)),
    }


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
        csinks = recognise_countersinks(part)
        holes = recognise_holes(part, csinks=csinks)

        def hole_kwargs(options):
            return {"csinks": csinks} if options.get("csinks") == "auto" else {}

        fillet = (FamilyId.FILLETS, lambda p, o: recognise_fillets(p, **o),
                  lambda p, ledger, o: _discover_fillets(p, min_radius=None, max_radius_frac=0.45,
                                                         face_edges=None, cyls=None, writer=ledger.writer,
                                                         include_cylindrical=o.get("include_cylindrical", True)))
        chamfer = (FamilyId.CHAMFERS, lambda p, o: recognise_chamfers(p, **o),
                   lambda p, ledger, o: _discover_chamfers(p, ledger=ledger.writer, **o))
        countersink = (FamilyId.COUNTERSINKS, lambda p, o: recognise_countersinks(p),
                       lambda p, ledger, o: _discover_countersinks(p, writer=ledger.writer))
        boss = (FamilyId.BOSSES, lambda p, o: recognise_bosses(p),
                lambda p, ledger, o: _discover_bosses(p, writer=ledger.writer))
        hole = (FamilyId.HOLES, lambda p, o: recognise_holes(p, **hole_kwargs(o)),
                lambda p, ledger, o: _discover_holes(p, writer=ledger.writer, **hole_kwargs(o)))
        entries.append(
            {
                "file": str(path.relative_to(CORPUS)),
                "inventory": _inventory(part),
                "results": {
                    "recognise_fillets": [
                        _run(part, "recognise_fillets", *fillet, o)
                        for o in ({"include_cylindrical": True}, {"include_cylindrical": False})
                    ],
                    "recognise_chamfers": [
                        _run(part, "recognise_chamfers", *chamfer, o) for o in ({}, {"include_planar": False})
                    ],
                    "recognise_countersinks": [_run(part, "recognise_countersinks", *countersink, {})],
                    "recognise_bosses": [_run(part, "recognise_bosses", *boss, {})],
                    "recognise_holes": [
                        _run(part, "recognise_holes", *hole, o) for o in ({}, {"csinks": "auto"})
                    ],
                    "recognise_hole_patterns": [
                        {"options": {"csinks": "auto"}, "result": _plain(recognise_hole_patterns(holes))}
                    ],
                },
            }
        )
        print(entries[-1]["file"], {k: [len(r["result"]) for r in v] for k, v in entries[-1]["results"].items()}, file=sys.stderr)
    OUT.write_text(json.dumps({"files": entries}, indent=1, allow_nan=False) + "\n")


if __name__ == "__main__":
    main()
