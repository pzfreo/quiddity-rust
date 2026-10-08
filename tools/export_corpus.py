"""Record the Python answers of every ported recogniser over the shared STEP corpus.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/export_corpus.py

Writes ``tests/fixtures/corpus.json``: the quiddity revision it ran at and, per corpus file, its
sha256, a face inventory (proves the Rust reader walks faces in OpenCascade's order), the kernel
answers the recognisers lean on (each face's ``BRepTools::UVBounds`` and the arc between each
pair of neighbours) and, per recogniser and option set, Python's records. The
corpus itself stays in the quiddity checkout; ``tests/corpus.rs`` finds it through
``QUIDDITY_CORPUS`` or ``../quiddity/tests/corpus``, and ``tests/corpus_files.rs`` checks it is
the corpus these answers are for. Pin CI's checkout (``.github/workflows/ci.yml``) to the
recorded revision.
"""

from __future__ import annotations

import dataclasses
import gzip
import json
import os
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY), str(QUIDDITY / "tests"), str(QUIDDITY / "src")]
sys.path.insert(0, str(Path(__file__).resolve().parent))

from _provenance import revision, sha256  # noqa: E402
from export_fixtures import _face_index, _inventory  # noqa: E402

from quiddity._adjacency import FaceGraph  # noqa: E402
from quiddity._candidates import FamilyId  # noqa: E402
from quiddity._claims import ClaimLedger  # noqa: E402
from quiddity.angled_steps import _discover_angled_steps  # noqa: E402
from quiddity.blends import _discover_blends, recognise_blends  # noqa: E402
from quiddity.bosses import _discover_bosses  # noqa: E402
from quiddity.chamfers import _discover_chamfers  # noqa: E402
from quiddity._cylinder_substrate import analyse_cylinders  # noqa: E402
from quiddity._effective_surfaces import effective_faces_for_graph  # noqa: E402
from quiddity.circular_blind_steps import _discover_circular_blind_steps  # noqa: E402
from quiddity.circular_face_patterns import _discover_circular_face_patterns  # noqa: E402
from quiddity.countersinks import _discover_countersinks  # noqa: E402
from quiddity.profiled_bores import _discover_double_d_bores, recognise_double_d_bores  # noqa: E402
from quiddity.edge_open_circular_recesses import (  # noqa: E402
    _discover_edge_open_circular_pockets,
    recognise_edge_open_circular_pockets,
)
from quiddity.edge_open_prismatic_recesses import (  # noqa: E402
    _discover_edge_open_prismatic_recesses,
    recognise_edge_open_prismatic_recesses,
)
from quiddity.fillets import _discover_fillets  # noqa: E402
from quiddity.flats import _discover_flats  # noqa: E402
from quiddity.freeform_surfaces import _records as _freeform_records  # noqa: E402
from quiddity.freeform_surfaces import recognise_freeform_surfaces  # noqa: E402
from quiddity.grooves import _discover_grooves  # noqa: E402
from quiddity.gussets import _discover_gusset_ribs  # noqa: E402
from quiddity.holes import _discover_holes  # noqa: E402
from quiddity.oblique_through_steps import _discover_oblique_through_steps  # noqa: E402
from quiddity.interior_voids import _claim_records as _claim_voids  # noqa: E402
from quiddity.interior_voids import _discover_interior_voids  # noqa: E402
from quiddity.oriented_chamfers import _discover_oriented_chamfers  # noqa: E402
from quiddity._section_passages import section_ring_proposals  # noqa: E402
from quiddity.oriented_slots import (  # noqa: E402
    _from_proposals as _oriented_slots_from_proposals,
    recognise_oriented_slot_patterns,
    recognise_oriented_slots,
)
from quiddity.paired_ramp_steps import _discover_paired_ramp_steps  # noqa: E402
from quiddity.passages import (  # noqa: E402
    _discover_section_passages,
    recognise_passages,
    recognise_section_passages,
)
from quiddity.plates import _discover_plates  # noqa: E402
from quiddity.prismatic_pockets import (  # noqa: E402
    _discover_prismatic_pockets,
    recognise_prismatic_pockets,
)
from quiddity.repeating_profiles import (  # noqa: E402
    _discover_repeating_radial_profiles,
    recognise_repeating_radial_profiles,
)
from quiddity.experimental_geometry import GeometryGraph  # noqa: E402
from quiddity.polygonal_bosses import (  # noqa: E402
    _discover_polygonal_bosses,
    _discover_polygonal_stock,
    recognise_polygonal_bosses,
    recognise_polygonal_stock,
)
from quiddity.rectangular_blind_slots import (  # noqa: E402
    _discover_rectangular_blind_slots,
    recognise_rectangular_blind_slots,
)
from quiddity._recess_features import (  # noqa: E402
    _discover_channels,
    _discover_pockets,
    _discover_slots,
    recognise_channels,
    recognise_pockets,
    recognise_slots,
)
from quiddity._recess_patterns import (  # noqa: E402
    recognise_pocket_patterns,
    recognise_slot_patterns,
)
from quiddity.round_bottom_slots import (  # noqa: E402
    _discover_round_bottom_blind_slots,
    recognise_round_bottom_blind_slots,
)
from quiddity.sheet_metal import _discover as _discover_sheet_metal  # noqa: E402
from quiddity.sheet_metal import recognise_sheet_metal_bodies  # noqa: E402
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
    recognise_grooves,
    recognise_gusset_rib_patterns,
    recognise_gusset_ribs,
    recognise_hole_patterns,
    recognise_holes,
    recognise_interior_voids,
    recognise_oblique_through_steps,
    recognise_oriented_chamfers,
    recognise_paired_ramp_steps,
    recognise_plates,
    recognise_thin_wall_bodies,
    recognise_through_steps,
    recognise_turned_steps,
)
from OCP.BRep import BRep_Tool  # noqa: E402
from OCP.BRepTools import BRepTools  # noqa: E402
from OCP.Geom import Geom_BSplineSurface  # noqa: E402

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


def _sheet_metal_evidence(part, ledger, options):
    """``sheet_metal._discover`` as the recognition run calls it, over the part's thin-wall
    bodies."""

    walls = _discover_thin_wall_bodies(part, graph=ledger.graph)
    services = SimpleNamespace(context=SimpleNamespace(part=part, graph=ledger.graph),
                               writer=ledger.writer)
    return _discover_sheet_metal(services, SimpleNamespace(records=lambda family, kind: walls))


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


def _discover_freeform(part, ledger) -> None:
    """``freeform_surfaces._discover`` without the pipeline: each record claims its face."""

    faces = tuple(part.faces())
    if not any(isinstance(BRep_Tool.Surface_s(face.wrapped), Geom_BSplineSurface) for face in faces):
        return  # recognise_freeform_surfaces' own shortcut: no native B-spline face, no record.
    walls = tuple(_discover_thin_wall_bodies(part, graph=ledger.graph))
    for record in _freeform_records(faces, ledger.graph, walls):
        node = ledger.graph.require_node(faces[record.face])
        ledger.writer.add_defining(record, (node,), family=FamilyId.FREEFORM_SURFACES)


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
        rectangular = (FamilyId.RECTANGULAR_BLIND_SLOTS, lambda p, o: recognise_rectangular_blind_slots(p),
                       lambda p, ledger, o: _discover_rectangular_blind_slots(p, graph=ledger.graph,
                                                                              sink=ledger.writer.sink))
        double_d = (FamilyId.DOUBLE_D_BORES, lambda p, o: recognise_double_d_bores(p),
                    lambda p, ledger, o: _discover_double_d_bores(p, writer=ledger.writer))
        open_circular = (FamilyId.EDGE_OPEN_CIRCULAR_POCKETS,
                         lambda p, o: recognise_edge_open_circular_pockets(p),
                         lambda p, ledger, o: _discover_edge_open_circular_pockets(p, graph=ledger.graph,
                                                                                   ledger=ledger.writer))
        open_prismatic = (FamilyId.EDGE_OPEN_PRISMATIC_RECESSES,
                          lambda p, o: recognise_edge_open_prismatic_recesses(p),
                          lambda p, ledger, o: _discover_edge_open_prismatic_recesses(p, graph=ledger.graph,
                                                                                      ledger=ledger.writer))
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
        gusset = (FamilyId.GUSSET_RIBS, lambda p, o: recognise_gusset_ribs(p),
                  lambda p, ledger, o: _discover_gusset_ribs(p, graph=ledger.graph, face_edges=None,
                                                             sink=ledger.writer.sink))
        grooves = (FamilyId.GROOVES, lambda p, o: recognise_grooves(p),
                   lambda p, ledger, o: _discover_grooves(p, ledger=ledger.writer))
        plates = (FamilyId.PLATES, lambda p, o: recognise_plates(p, **o),
                  lambda p, ledger, o: _discover_plates(p, writer=ledger.writer, **o))
        blends = (FamilyId.BLENDS, lambda p, o: recognise_blends(p),
                  lambda p, ledger, o: _discover_blends(p, graph=ledger.graph, writer=ledger.writer))
        sheet_metal = (FamilyId.SHEET_METAL_BODIES, lambda p, o: recognise_sheet_metal_bodies(p, **o),
                       _sheet_metal_evidence)
        slots = (FamilyId.SLOTS, lambda p, o: recognise_slots(p),
                 lambda p, ledger, o: _discover_slots(p, writer=ledger.writer))
        pockets = (FamilyId.POCKETS, lambda p, o: recognise_pockets(p),
                   lambda p, ledger, o: _discover_pockets(p, writer=ledger.writer))
        channels = (FamilyId.CHANNELS, lambda p, o: recognise_channels(p),
                    lambda p, ledger, o: _discover_channels(p, writer=ledger.writer))
        repeating = (FamilyId.REPEATING_RADIAL_PROFILES,
                     lambda p, o: recognise_repeating_radial_profiles(p),
                     lambda p, ledger, o: _discover_repeating_radial_profiles(p, writer=ledger.writer))
        freeform = (FamilyId.FREEFORM_SURFACES, lambda p, o: recognise_freeform_surfaces(p),
                    lambda p, ledger, o: _discover_freeform(p, ledger))
        polygonal_bosses = (FamilyId.POLYGONAL_BOSSES, lambda p, o: recognise_polygonal_bosses(p),
                            lambda p, ledger, o: _discover_polygonal_bosses(
                                p, graph=GeometryGraph._from_graph(ledger.graph), writer=ledger.writer))
        polygonal_stock = (FamilyId.POLYGONAL_STOCK, lambda p, o: recognise_polygonal_stock(p),
                           lambda p, ledger, o: _discover_polygonal_stock(
                               p, graph=GeometryGraph._from_graph(ledger.graph), writer=ledger.writer))
        section_passages = (FamilyId.PASSAGES, lambda p, o: recognise_section_passages(p),
                            lambda p, ledger, o: _discover_section_passages(p, ledger.graph,
                                                                            ledger.writer.sink))
        prismatic_pockets = (FamilyId.PRISMATIC_POCKETS, lambda p, o: recognise_prismatic_pockets(p),
                             lambda p, ledger, o: _discover_prismatic_pockets(p, graph=ledger.graph,
                                                                              ledger=ledger))
        oriented_slots = (FamilyId.ORIENTED_SLOTS, lambda p, o: recognise_oriented_slots(p),
                          lambda p, ledger, o: _oriented_slots_from_proposals(
                              ledger.graph, section_ring_proposals(p, ledger.graph), ledger.writer.sink))
        entries.append(
            {
                "file": str(path.relative_to(CORPUS)),
                "sha256": sha256(path),
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
                    "recognise_rectangular_blind_slots": [
                        _run(part, "recognise_rectangular_blind_slots", *rectangular, {})
                    ],
                    "recognise_double_d_bores": [_run(part, "recognise_double_d_bores", *double_d, {})],
                    "recognise_gusset_ribs": [_run(part, "recognise_gusset_ribs", *gusset, {})],
                    "recognise_grooves": [_run(part, "recognise_grooves", *grooves, {})],
                    "recognise_plates": [_run(part, "recognise_plates", *plates, {})],
                    "recognise_edge_open_circular_pockets": [
                        _run(part, "recognise_edge_open_circular_pockets", *open_circular, {})
                    ],
                    "recognise_edge_open_prismatic_recesses": [
                        _run(part, "recognise_edge_open_prismatic_recesses", *open_prismatic, {})
                    ],
                    "recognise_blends": [_run(part, "recognise_blends", *blends, {})],
                    "recognise_sheet_metal_bodies": [
                        _run(part, "recognise_sheet_metal_bodies", *sheet_metal, {})
                    ],
                    "recognise_slots": [_run(part, "recognise_slots", *slots, {})],
                    "recognise_pockets": [_run(part, "recognise_pockets", *pockets, {})],
                    "recognise_channels": [_run(part, "recognise_channels", *channels, {})],
                    "recognise_repeating_radial_profiles": [
                        _run(part, "recognise_repeating_radial_profiles", *repeating, {})
                    ],
                    "recognise_freeform_surfaces": [
                        _run(part, "recognise_freeform_surfaces", *freeform, {})
                    ],
                    "recognise_polygonal_bosses": [
                        _run(part, "recognise_polygonal_bosses", *polygonal_bosses, {})
                    ],
                    "recognise_polygonal_stock": [
                        _run(part, "recognise_polygonal_stock", *polygonal_stock, {})
                    ],
                    "recognise_section_passages": [
                        _run(part, "recognise_section_passages", *section_passages, {})
                    ],
                    "recognise_prismatic_pockets": [
                        _run(part, "recognise_prismatic_pockets", *prismatic_pockets, {})
                    ],
                    # The legacy roster: Python refuses its evidence path (PassageCompatibilityError).
                    "recognise_passages": [{"options": {}, "result": _plain(recognise_passages(part))}],
                    "recognise_gusset_rib_patterns": [
                        {"options": {}, "result": _plain(recognise_gusset_rib_patterns(recognise_gusset_ribs(part)))}
                    ],
                    "recognise_hole_patterns": [
                        {"options": {"csinks": "auto"}, "result": _plain(recognise_hole_patterns(holes))}
                    ],
                    "recognise_slot_patterns": [
                        {"options": {}, "result": _plain(recognise_slot_patterns(recognise_slots(part)))}
                    ],
                    "recognise_pocket_patterns": [
                        {"options": {}, "result": _plain(recognise_pocket_patterns(recognise_pockets(part)))}
                    ],
                    "recognise_oriented_slots": [
                        _run(part, "recognise_oriented_slots", *oriented_slots, {})
                    ],
                    "recognise_oriented_slot_patterns": [
                        {"options": {}, "result": _plain(
                            recognise_oriented_slot_patterns(recognise_oriented_slots(part)))}
                    ],
                },
            }
        )
        print(entries[-1]["file"], {k: [len(r["result"]) for r in v] for k, v in entries[-1]["results"].items()}, file=sys.stderr)
    OUT.write_text(
        json.dumps({"quiddity_revision": revision(QUIDDITY), "files": entries}, indent=1, allow_nan=False)
        + "\n"
    )


if __name__ == "__main__":
    main()
