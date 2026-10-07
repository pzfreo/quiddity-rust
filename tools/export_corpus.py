"""Record the Python answers of every ported recogniser over the shared STEP corpus.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/export_corpus.py

Writes ``tests/fixtures/corpus.json``: per corpus file, a face inventory (proves the Rust reader
walks faces in OpenCascade's order), the kernel answers the recognisers lean on (each face's
``BRepTools::UVBounds`` and the arc between each pair of neighbours) and, per recogniser and
option set, Python's records. The
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
from quiddity.angled_steps import _discover_angled_steps  # noqa: E402
from quiddity.bosses import _discover_bosses  # noqa: E402
from quiddity.chamfers import _discover_chamfers  # noqa: E402
from quiddity._cylinder_substrate import analyse_cylinders  # noqa: E402
from quiddity._effective_surfaces import effective_faces_for_graph  # noqa: E402
from quiddity.circular_blind_steps import _discover_circular_blind_steps  # noqa: E402
from quiddity.circular_face_patterns import _discover_circular_face_patterns  # noqa: E402
from quiddity.countersinks import _discover_countersinks  # noqa: E402
from quiddity.fillets import _discover_fillets  # noqa: E402
from quiddity.flats import _discover_flats  # noqa: E402
from quiddity.holes import _discover_holes  # noqa: E402
from quiddity.oblique_through_steps import _discover_oblique_through_steps  # noqa: E402
from quiddity.interior_voids import _claim_records as _claim_voids  # noqa: E402
from quiddity.interior_voids import _discover_interior_voids  # noqa: E402
from quiddity.oriented_chamfers import _discover_oriented_chamfers  # noqa: E402
from quiddity.paired_ramp_steps import _discover_paired_ramp_steps  # noqa: E402
from quiddity.round_bottom_slots import (  # noqa: E402
    _discover_round_bottom_blind_slots,
    recognise_round_bottom_blind_slots,
)
from quiddity.thin_walls import _claim_records as _claim_walls  # noqa: E402
from quiddity.thin_walls import _discover_thin_wall_bodies  # noqa: E402
from quiddity.through_steps import _discover_through_steps  # noqa: E402
from quiddity.turned import _discover_turned_steps  # noqa: E402

from quiddity import (  # noqa: E402
    recognise_flats,
    recognise_angled_steps,
    recognise_bosses,
    recognise_fillets,
    recognise_chamfers,
    recognise_circular_blind_steps,
    recognise_circular_face_patterns,
    import_step_geometry,
    recognise_countersinks,
    recognise_hole_patterns,
    recognise_holes,
    recognise_interior_voids,
    recognise_oblique_through_steps,
    recognise_oriented_chamfers,
    recognise_paired_ramp_steps,
    recognise_thin_wall_bodies,
    recognise_through_steps,
    recognise_turned_steps,
)
from OCP.BRepTools import BRepTools  # noqa: E402

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


def _kernel(part) -> dict:
    """Per face, ``BRepTools::UVBounds``; per neighbouring pair (both ways round), the arc; per
    solid, its volume and area."""

    graph = FaceGraph(part)
    return {
        "solids": [[float(s.volume), float(s.area)] for s in part.solids()],
        "uv_bounds": [list(BRepTools.UVBounds_s(face.wrapped)) for face in part.faces()],
        "arcs": [
            [a.index, b.index, graph.arc(a, b)] for a in graph.nodes for b in graph.neighbours(a)
        ],
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
        angled = (FamilyId.ANGLED_STEPS, lambda p, o: recognise_angled_steps(p),
                  lambda p, ledger, o: _discover_angled_steps(p, face_edges=None, graph=ledger.graph,
                                                              sink=ledger.writer.sink))
        flat = (FamilyId.FLATS, lambda p, o: recognise_flats(p),
                lambda p, ledger, o: _discover_flats(p, cyls=None, face_edges=None, writer=ledger.writer))
        ramp = (FamilyId.PAIRED_RAMP_STEPS, lambda p, o: recognise_paired_ramp_steps(p),
                lambda p, ledger, o: _discover_paired_ramp_steps(p, graph=ledger.graph, sink=ledger.writer.sink))
        oriented = (FamilyId.ORIENTED_CHAMFERS, lambda p, o: recognise_oriented_chamfers(p),
                    lambda p, ledger, o: _discover_oriented_chamfers(p, graph=ledger.graph, sink=ledger.writer.sink))
        circular = (FamilyId.CIRCULAR_FACE_PATTERNS, lambda p, o: recognise_circular_face_patterns(p),
                    lambda p, ledger, o: _discover_circular_face_patterns(p, writer=ledger.writer))
        oblique = (FamilyId.OBLIQUE_THROUGH_STEPS, lambda p, o: recognise_oblique_through_steps(p),
                   lambda p, ledger, o: _discover_oblique_through_steps(p, graph=ledger.graph,
                                                                        sink=ledger.writer.sink))

        def _circular_blind(p, ledger, o):
            effective = effective_faces_for_graph(ledger.graph)
            return _discover_circular_blind_steps(
                p, graph=ledger.graph, cylinders=analyse_cylinders(p, face_surfaces=effective),
                effective=effective, sink=ledger.writer.sink)

        circular_blind = (FamilyId.CIRCULAR_BLIND_STEPS, lambda p, o: recognise_circular_blind_steps(p),
                          _circular_blind)
        round_bottom = (FamilyId.ROUND_BOTTOM_BLIND_SLOTS, lambda p, o: recognise_round_bottom_blind_slots(p),
                        lambda p, ledger, o: _discover_round_bottom_blind_slots(p, graph=ledger.graph,
                                                                                sink=ledger.writer.sink))
        hole = (FamilyId.HOLES, lambda p, o: recognise_holes(p, **hole_kwargs(o)),
                lambda p, ledger, o: _discover_holes(p, writer=ledger.writer, **hole_kwargs(o)))
        walls = (FamilyId.THIN_WALL_BODIES, lambda p, o: recognise_thin_wall_bodies(p),
                 lambda p, ledger, o: _claim_walls(_discover_thin_wall_bodies(p, graph=ledger.graph), p,
                                                   ledger.writer))
        voids = (FamilyId.INTERIOR_VOIDS, lambda p, o: recognise_interior_voids(p),
                 lambda p, ledger, o: _claim_voids(_discover_interior_voids(p, graph=ledger.graph), p,
                                                   ledger.writer))
        through = (FamilyId.THROUGH_STEPS, lambda p, o: recognise_through_steps(p),
                   lambda p, ledger, o: _discover_through_steps(p, graph=ledger.graph, sink=ledger.writer.sink))
        turned_steps = (FamilyId.TURNED_STEPS, lambda p, o: recognise_turned_steps(p),
                        lambda p, ledger, o: _discover_turned_steps(p, ledger=ledger.writer))
        entries.append(
            {
                "file": str(path.relative_to(CORPUS)),
                "inventory": _inventory(part),
                "kernel": _kernel(part),
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
                    "recognise_angled_steps": [_run(part, "recognise_angled_steps", *angled, {})],
                    "recognise_flats": [_run(part, "recognise_flats", *flat, {})],
                    "recognise_paired_ramp_steps": [_run(part, "recognise_paired_ramp_steps", *ramp, {})],
                    "recognise_oriented_chamfers": [_run(part, "recognise_oriented_chamfers", *oriented, {})],
                    "recognise_circular_face_patterns": [
                        _run(part, "recognise_circular_face_patterns", *circular, {})
                    ],
                    "recognise_oblique_through_steps": [
                        _run(part, "recognise_oblique_through_steps", *oblique, {})
                    ],
                    "recognise_circular_blind_steps": [
                        _run(part, "recognise_circular_blind_steps", *circular_blind, {})
                    ],
                    "recognise_holes": [
                        _run(part, "recognise_holes", *hole, o) for o in ({}, {"csinks": "auto"})
                    ],
                    "recognise_thin_wall_bodies": [_run(part, "recognise_thin_wall_bodies", *walls, {})],
                    "recognise_interior_voids": [_run(part, "recognise_interior_voids", *voids, {})],
                    "recognise_through_steps": [_run(part, "recognise_through_steps", *through, {})],
                    "recognise_turned_steps": [_run(part, "recognise_turned_steps", *turned_steps, {})],
                    "recognise_round_bottom_blind_slots": [
                        _run(part, "recognise_round_bottom_blind_slots", *round_bottom, {})
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
