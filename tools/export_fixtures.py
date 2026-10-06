"""Export the Python fillet test parts as STEP files plus the Python recogniser's answers.

Run from the quiddity checkout's environment:

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/export_fixtures.py

Every case is exported to STEP, re-imported through ``import_step_geometry`` and recognised
there, because the Rust port only ever sees the STEP file. The in-memory answer is recorded too,
so a case where the STEP round trip itself changes the Python answer is visible rather than
blamed on the port.
"""

from __future__ import annotations

import gzip
import importlib
import json
import math
import os
import sys
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY), str(QUIDDITY / "tests"), str(QUIDDITY / "src")]

from build123d import (  # noqa: E402
    Axis,
    Box,
    Cylinder,
    GeomType,
    Pos,
    Rot,
    Shell,
    Sphere,
    Torus,
    export_step,
    fillet,
)

import tests.test_fillet_attribution as fa  # noqa: E402
import tests.test_turned_chamfers as tc  # noqa: E402
from quiddity import import_step_geometry, recognise_fillets  # noqa: E402
from quiddity._adjacency import FaceGraph  # noqa: E402
from quiddity._candidates import FamilyId  # noqa: E402
from quiddity._claims import ClaimLedger  # noqa: E402
from quiddity._features import analyse_cylinders  # noqa: E402
from quiddity.fillets import _discover_fillets  # noqa: E402

OUT = Path(__file__).resolve().parent.parent / "tests" / "fixtures"
CORPUS = QUIDDITY / "tests" / "corpus"


def _record(record) -> dict:
    return {
        "axis": record.axis,
        "radius": record.radius,
        "at": list(record.at),
        "turned": record.turned,
        "side": record.side,
    }


def _face_index(part, face) -> int:
    faces = list(part.faces())
    return next(i for i, other in enumerate(faces) if other.wrapped.IsSame(face.wrapped))


def _solid_index(part, face) -> int | None:
    for i, solid in enumerate(part.solids()):
        if any(other.wrapped.IsSame(face.wrapped) for other in solid.faces()):
            return i
    return None


def _inventory(part) -> list[dict]:
    """A per-face fingerprint: proves the Rust reader walks faces in OCCT's order."""

    out = []
    for face in part.faces():
        bb = face.bounding_box()
        out.append(
            {
                "type": face.geom_type.name,
                "edges": len(face.edges()),
                "min": [round(bb.min.X, 3), round(bb.min.Y, 3), round(bb.min.Z, 3)],
                "max": [round(bb.max.X, 3), round(bb.max.Y, 3), round(bb.max.Z, 3)],
            }
        )
    return out


def _run(part, opts: dict) -> dict:
    call = {
        "min_radius": opts.get("min_radius"),
        "max_radius_frac": opts.get("max_radius_frac", 0.45),
        "include_cylindrical": opts.get("include_cylindrical", True),
    }
    plain = recognise_fillets(part, **call)
    out: dict = {"records": [_record(r) for r in plain]}
    ledger = ClaimLedger(FaceGraph(part))
    try:
        measured = _discover_fillets(
            part, face_edges=None, cyls=analyse_cylinders(part), writer=ledger.writer, **call
        )
    except ValueError as error:
        out["evidence_error"] = str(error)
        return out
    assert [_record(r) for r in measured] == out["records"]
    defining = []
    for candidate in ledger.candidate_set(FamilyId.FILLETS).candidates:
        (node,) = ledger.defining_of(candidate)
        face = ledger.graph.face(node)
        defining.append({"face": _face_index(part, face), "solid": _solid_index(part, face)})
    out["defining"] = defining
    return out


def _turned_with_sphere():
    return fa._turned().fuse(Pos(0, 0, 70) * Sphere(3))


def _shell():
    return Shell(fa._prismatic().faces()[:-1])


THRESHOLDS = [
    (1.999, 0.45),
    (2.0, 0.45),
    (2.001, 0.45),
    (None, 0.05001),
    (None, 0.05),
    (None, 0.04999),
]


def built_cases():
    """(name, builder, [option sets]) for every part the Python fillet tests build."""

    default = [{}]
    both = [{"include_cylindrical": True}, {"include_cylindrical": False}]
    cases = [
        ("prismatic_axis_x", lambda: fa._prismatic_axis(Axis.X), default),
        ("prismatic_axis_y", lambda: fa._prismatic_axis(Axis.Y), default),
        ("prismatic_axis_z", lambda: fa._prismatic_axis(Axis.Z), default),
        ("prismatic_mirror", lambda: fa._prismatic().mirror(), default),
        ("prismatic_rot_y90", lambda: Rot(0, 90, 0) * fa._prismatic(), default),
        ("prismatic_rot_x90", lambda: Rot(90, 0, 0) * fa._prismatic(), default),
        ("prismatic_scale3", lambda: fa._prismatic().scale(3), default),
        ("prismatic_translated", lambda: Pos(13, -7, 5) * fa._prismatic(), default),
        (
            "prismatic",
            fa._prismatic,
            [{}]
            + [{"min_radius": m, "max_radius_frac": f} for m, f in THRESHOLDS]
            + [
                {"min_radius": 2.0, "max_radius_frac": 0.05},
                {"min_radius": 2.1, "max_radius_frac": 0.049},
            ],
        ),
        ("turned", fa._turned, both),
        ("turned_with_sphere", _turned_with_sphere, both),
        ("mixed", lambda: Pos(-70, 0, 0) * fa._prismatic() + Pos(70, 0, 0) * fa._turned(), both),
        (
            "unequal_radii",
            lambda: Pos(-50, 0, 0) * fa._prismatic_axis(Axis.Z, 1.0)
            + Pos(50, 0, 0) * fa._prismatic_axis(Axis.Z, 2.0),
            default,
        ),
        ("blend_r5", lambda: fa._prismatic_axis(Axis.Z, 5.0), [{"max_radius_frac": 0.1}]),
        (
            "blend_r5_enlarged",
            lambda: fa._prismatic_axis(Axis.Z, 5.0) + Pos(200, 0, 0) * Box(100, 100, 100),
            [{"max_radius_frac": 0.1}],
        ),
        ("nonprincipal_37", lambda: fa._prismatic().rotate(Axis.X, 37), default),
        ("open_shell", _shell, default),
        ("rejected_cylinder", lambda: Cylinder(10, 20), default),
        ("rejected_bored_box", lambda: Box(30, 30, 20) - Cylinder(5, 20), default),
        ("rejected_internal_pocket", fa._internal_pocket_round, default),
        ("rejected_internal_pocket_rot", lambda: Rot(0, 90, 0) * fa._internal_pocket_round(), default),
        ("rejected_through_slot", fa._through_slot, default),
        ("rejected_through_slot_rot", lambda: Rot(90, 0, 0) * fa._through_slot(), default),
        ("rejected_through_slot_mirror", lambda: fa._through_slot().mirror(), default),
        ("interrupted_corner", fa._interrupted_corner_round, default),
        ("interrupted_corner_rot", lambda: Rot(90, 0, 0) * fa._interrupted_corner_round(), default),
        ("interrupted_corner_mirror", lambda: fa._interrupted_corner_round().mirror(), default),
        ("split_coplanar_support", fa._split_coplanar_fillet_support, default),
        ("toroidal_bead", lambda: Cylinder(10, 20) + Torus(10, 2), both),
        ("full_torus", lambda: Torus(10, 2), both),
        (
            "internal_bore_round",
            lambda: fillet(
                (Box(60, 60, 20) - Cylinder(5, 20)).edges().filter_by(GeomType.CIRCLE)[0], 1.0
            ),
            both,
        ),
        ("turned_rot37", lambda: fa._turned().rotate(Axis.X, 37), both),
        ("two_solids", lambda: Pos(0, -60, 0) * fa._prismatic() + Pos(0, 60, 0) * fa._prismatic(), default),
        ("filleted_stepped_shaft", tc._filleted_stepped_shaft, both),
        (
            "rounded_box_r1",
            lambda: fillet(list(Box(40, 30, 20).edges().filter_by(Axis.Z)), 1.0),
            default,
        ),
        ("slanted_bead", lambda: (Cylinder(10, 20) + Torus(10, 2)).rotate(Axis.X, 45), default),
        (
            "large_block_small_fillets",
            lambda: fillet(
                Box(200.0, 200.0, 60).edges().filter_by(GeomType.LINE).group_by()[0], 1.0
            ),
            default,
        ),
        ("filleted_plate_with_holes", _filleted_plate, default),
        ("memo_part", _memo_part, default),
        ("filleted_box_one_edge", _filleted_box, default),
    ]
    for golden in sorted((QUIDDITY / "tests" / "golden").iterdir()):
        if (golden / "fixture.py").exists():
            module = importlib.import_module(f"tests.golden.{golden.name}.fixture")
            cases.append((f"golden_{golden.name}", module.build_fixture, both))
    return cases


def _filleted_plate():
    from tests.test_recognition_edge_cases import _filleted_plate as build

    return build()


def _memo_part():
    from build123d import chamfer

    part = Box(60, 40, 12) - Pos(-18, 0, 0) * Cylinder(4, 12) - Pos(15, 0, 0) * Box(24, 8, 12)
    part = chamfer(part.edges().filter_by(Axis.Z).group_by(Axis.X)[0], 1.5)
    return fillet(part.edges().filter_by(Axis.Z).group_by(Axis.X)[-1], 2.0)


def _filleted_box():
    from tests.test_recogniser_contract import _filleted_box as build

    return build()


def corpus_files():
    for path in sorted(CORPUS.rglob("*")):
        if path.suffix.lower() in (".step", ".stp") or path.name.lower().endswith(
            (".step.gz", ".stp.gz")
        ):
            yield path


def _load_corpus(path: Path, scratch: Path):
    if path.suffix == ".gz":
        scratch.write_bytes(gzip.decompress(path.read_bytes()))
        return import_step_geometry(str(scratch))
    return import_step_geometry(str(path))


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    manifest = {"built": [], "corpus": []}
    for name, build, option_sets in built_cases():
        part = build()
        target = OUT / f"{name}.step"
        assert export_step(part, str(target)), name
        reread = import_step_geometry(str(target))
        entry = {"name": name, "file": target.name, "inventory": _inventory(reread), "runs": []}
        for opts in option_sets:
            run = _run(reread, opts)
            in_memory = _run(part, opts)
            run["roundtrip_equal"] = in_memory["records"] == run["records"]
            entry["runs"].append({"options": opts, **run})
        manifest["built"].append(entry)
        print(name, [len(r["records"]) for r in entry["runs"]], file=sys.stderr)

    scratch = OUT / "_scratch.step"
    for path in corpus_files():
        try:
            part = _load_corpus(path, scratch)
        except Exception as error:  # noqa: BLE001 -- record unreadable corpus parts, don't stop
            print("skip", path, error, file=sys.stderr)
            continue
        rel = str(path.relative_to(CORPUS))
        entry = {"file": rel, "inventory": _inventory(part), "runs": []}
        for opts in [{"include_cylindrical": True}, {"include_cylindrical": False}]:
            entry["runs"].append({"options": opts, **_run(part, opts)})
        manifest["corpus"].append(entry)
        print(rel, [len(r["records"]) for r in entry["runs"]], file=sys.stderr)
    scratch.unlink(missing_ok=True)

    def finite(value):
        if isinstance(value, float) and not math.isfinite(value):
            raise ValueError("non-finite value in manifest")
        return value

    (OUT / "manifest.json").write_text(json.dumps(manifest, indent=1, default=finite) + "\n")


if __name__ == "__main__":
    main()
