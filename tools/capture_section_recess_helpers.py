"""Record the section-recess helpers' inputs and answers: ``quiddity._cylindrical_seats``,
``_cylindrical_end_surface`` and ``_plane_envelope_passages``, which have no public
``recognise_*`` entry point of their own.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_section_recess_helpers.py

Two phases, written to ``tests/fixtures/captured/section_recess_helpers/``:

1. The Python tests that reach these modules (their own tests and the section-recess tests that
   build documents through them) run in this process with the helpers wrapped. Every distinct
   ``CylindricalEndSurface`` built, and every ``height`` and ``polygon_height_bounds`` question
   asked of one, is recorded with its answer or its refusal (``values.json.gz``); every part
   handed to ``cylindrical_seat_proofs`` or ``plane_envelope_passage_proofs`` is kept as STEP
   (``parts/<sha>.step.gz``).
2. Once the tests are done (so no test's monkeypatch is live), both proof functions run on each
   kept part, the golden fixtures and every corpus part, each re-imported from STEP as the port
   reads it, with their private ``_prove`` wrapped (``proofs.json.gz``): per part the proofs,
   and every ``_prove`` question asked on the way with its answer (a proof, ``null``, or the
   exception the public function swallows). A test part is kept when it came from the helpers'
   own tests (``OWN_TESTS``) or Python proves something on it, and a sample of the rest, on
   which Python proves nothing, is kept so the port must prove nothing there too
   (``NOTHING_PER_TEST``); the others are deleted again.

A value whose input the port's types cannot carry (a boolean or a wrongly shaped tuple where a
float or a pair belongs, which Python refuses by type) or a part that cannot be exported is
counted, not silently dropped: the counts are in each file's ``skipped``, and the test parts
left out by the selection in ``proofs.json.gz``'s ``unselected``.
"""

from __future__ import annotations

import gzip
import hashlib
import json
import math
import os
import re
import sys
import tempfile
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src"), str(QUIDDITY)]

import pytest  # noqa: E402
from build123d import export_step  # noqa: E402

import quiddity._cylindrical_end_surface as end_surface  # noqa: E402
import quiddity._cylindrical_seats as seats  # noqa: E402
import quiddity._plane_envelope_passages as envelopes  # noqa: E402
from _provenance import revision  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402
from quiddity._adjacency import FaceGraph  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "captured" / "section_recess_helpers"
FIXTURES = ROOT / "tests" / "fixtures"
CORPUS = QUIDDITY / "tests" / "corpus"
TESTS = (
    "tests/test_cylindrical_seats.py",
    "tests/test_cylindrical_end_surface.py",
    "tests/test_plane_envelope_passages.py",
    "tests/test_plane_envelope_contract.py",
    "tests/test_section_recess_invariants.py",
    "tests/test_cylindrical_channel_contract.py",
    "tests/test_cylindrical_channel_public.py",
    "tests/test_cylindrical_passage_contract.py",
    "tests/test_cylindrical_passage_public.py",
    "tests/test_cylindrical_pocket_proofs.py",
    "tests/test_mixed_section_pockets.py",
    "tests/test_section_recesses.py",
    "tests/test_section_recess_cutover.py",
    "tests/test_section_recess_geometry_golden.py",
    "tests/test_corner_notch_guards.py",
    "tests/test_open_channel_section.py",
    "tests/test_polygonal_split_mouths.py",
    "tests/test_section_pattern_projection.py",
    "tests/test_support_apertures.py",
    "tests/test_passage_publication_ties.py",
)
# Parts from these tests are all kept; a part only other tests build is kept when Python proves a
# seat or an envelope passage on it, or as one of ``NOTHING_PER_TEST`` parts per test (the first
# test in ``TESTS`` that builds it) on which Python proves nothing: those that asked ``_prove``
# something first, then by name. The rest are counted in ``unselected``: all 474 would add about
# 4 MB of STEP, and their questions repeat refusals the kept ones and the corpus already ask.
NOTHING_PER_TEST = 4
OWN_TESTS = (
    "tests/test_cylindrical_seats.py",
    "tests/test_plane_envelope_passages.py",
    "tests/test_plane_envelope_contract.py",
)
GOLDEN = ("golden_hexagonal_passage.step", "golden_interrupted_and_cross_bores.step")

_values: dict[str, dict] = {}
_parts: dict[str, tuple[str, set[str]]] = {}
_skipped = {"values": 0, "parts": 0}


def _key(record: dict) -> str:
    return json.dumps(record, sort_keys=True)


def _record_value(kind: str, given: dict, answer: dict) -> None:
    record = {"kind": kind, "given": given}
    _values.setdefault(_key(record), {**record, **answer})


def _float(value) -> bool:
    return isinstance(value, float | int) and not isinstance(value, bool)


def _floats(value, n: int | None = None) -> bool:
    return (
        isinstance(value, tuple | list)
        and (n is None or len(value) == n)
        and all(_float(v) for v in value)
    )


def _surface_given(surface) -> dict | None:
    """The surface's fields as the port's types can carry them, or None."""

    fields = {
        "type": surface.type,
        "axis_point": surface.axis_point,
        "axis_direction": surface.axis_direction,
        "radius": surface.radius,
        "branch": surface.branch,
    }
    if not (
        isinstance(fields["type"], str)
        and isinstance(fields["branch"], str)
        and _floats(fields["axis_point"], 3)
        and _floats(fields["axis_direction"], 2)
        and _float(fields["radius"])
    ):
        return None
    return {
        **fields,
        "axis_point": [float(v) for v in fields["axis_point"]],
        "axis_direction": [float(v) for v in fields["axis_direction"]],
        "radius": float(fields["radius"]),
    }


# --- the end-surface value -------------------------------------------------------------------

_real_post_init = end_surface.CylindricalEndSurface.__post_init__
_real_height = end_surface.CylindricalEndSurface.height
_real_bounds = end_surface.CylindricalEndSurface.polygon_height_bounds


def _post_init(self) -> None:
    given = _surface_given(self)
    if given is None:
        _skipped["values"] += 1
        return _real_post_init(self)
    try:
        _real_post_init(self)
    except ValueError as error:
        _record_value("surface", given, {"refused": str(error)})
        raise
    _record_value("surface", given, {"accepted": True, "dict": self.to_dict()})


def _height(self, point):
    given = _surface_given(self)
    if given is None or not _floats(point, 2):
        _skipped["values"] += 1
        return _real_height(self, point)
    given = {"surface": given, "point": [float(v) for v in point]}
    try:
        answer = _real_height(self, point)
    except ValueError as error:
        _record_value("height", given, {"refused": str(error)})
        raise
    _record_value("height", given, {"height": answer})
    return answer


def _bounds(self, points):
    given = _surface_given(self)
    if given is None or not isinstance(points, tuple | list) or not all(_floats(p) for p in points):
        _skipped["values"] += 1
        return _real_bounds(self, points)
    given = {"surface": given, "points": [[float(v) for v in p] for p in points]}
    try:
        answer = _real_bounds(self, points)
    except ValueError as error:
        _record_value("bounds", given, {"refused": str(error)})
        raise
    _record_value("bounds", given, {"bounds": list(answer)})
    return answer


# --- parts -----------------------------------------------------------------------------------


def _step(shape) -> str:
    """The shape as STEP text, its export timestamp fixed so one part has one file name."""

    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        export_step(shape, tmp.name)
        text = Path(tmp.name).read_text()
    return re.sub(r"'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d'", "'2000-01-01T00:00:00'", text, count=1)


def _keep(graph) -> None:
    try:
        text = _step(graph._part)
    except Exception:  # noqa: BLE001
        _skipped["parts"] += 1
    else:
        test = os.environ.get("PYTEST_CURRENT_TEST", "").split("::")[0]
        _parts.setdefault(hashlib.sha256(text.encode()).hexdigest()[:16], (text, set()))[1].add(test)


_real_seats = seats.cylindrical_seat_proofs
_real_envelopes = envelopes.plane_envelope_passage_proofs


def _seats(graph):
    _keep(graph)
    return _real_seats(graph)


def _envelopes(graph):
    _keep(graph)
    return _real_envelopes(graph)


class _Recorder:
    """Installs the wrappers for the test run and removes them after it."""

    def pytest_sessionstart(self, session) -> None:  # noqa: ARG002
        import quiddity._section_recess_geometry  # noqa: F401

        cls = end_surface.CylindricalEndSurface
        self._classes = [
            (cls, "__post_init__", _real_post_init),
            (cls, "height", _real_height),
            (cls, "polygon_height_bounds", _real_bounds),
        ]
        cls.__post_init__ = _post_init
        cls.height = _height
        cls.polygon_height_bounds = _bounds
        # Every module holding one of the real functions gets the wrapper, wherever it imported
        # it from; the test modules import them after this, from the wrapped modules.
        wrappers = {id(_real_seats): _seats, id(_real_envelopes): _envelopes}
        self._modules = []
        for module in list(sys.modules.values()):
            if not getattr(module, "__name__", "").startswith("quiddity"):
                continue
            for name, value in list(vars(module).items()):
                if id(value) in wrappers:
                    self._modules.append((module, name, value))
                    setattr(module, name, wrappers[id(value)])

    def pytest_sessionfinish(self, session, exitstatus) -> None:  # noqa: ARG002
        for owner, name, value in self._classes + self._modules:
            setattr(owner, name, value)


# --- phase 2: proofs on re-imported parts ----------------------------------------------------


def _frame(frame) -> dict:
    return {key: list(getattr(frame, key)) for key in ("origin", "run", "u", "v")}


def _vertices(boundary) -> list[list[float]]:
    return [[float(v.point[0]), float(v.point[1]), float(v.bulge)] for v in boundary]


def _seat(proof) -> dict:
    return {
        "walls": [n.index for n in proof.walls],
        "context": [n.index for n in proof.context],
        "owner": proof.owner.ordinal,
        "frame": _frame(proof.frame),
        "boundary": _vertices(proof.boundary),
        "run_interval": list(proof.run_interval),
    }


def _envelope(proof) -> dict:
    return {
        "walls": [n.index for n in proof.walls],
        "planar_context": proof.planar_context.index,
        "roof_contexts": [n.index for n in proof.roof_contexts],
        "owner": proof.owner.ordinal,
        "frame": _frame(proof.frame),
        "section": _vertices(proof.section.boundary),
        "run_interval": list(proof.run_interval),
        "envelope_end": proof.envelope_end,
        "terms": [[h, list(g)] for h, g in proof.terms],
        "volume": proof.volume,
    }


def _proofs(part) -> dict:
    seat_calls: list[dict] = []
    envelope_calls: list[dict] = []
    real_seat, real_envelope = seats._prove, envelopes._prove

    def seat(graph, walls, cylinder):
        call = {
            "walls": [n.index for n in walls],
            "radius": cylinder.radius,
            "axis": list(cylinder.axis),
            "origin": list(cylinder.origin),
        }
        seat_calls.append(call)
        try:
            proof = real_seat(graph, walls, cylinder)
        except Exception as error:
            call.update(proof=None, raised=f"{type(error).__name__}: {error}")
            raise
        call["proof"] = None if proof is None else _seat(proof)
        return proof

    def envelope(graph, mouth, walls, base):
        call = {"mouth": mouth.index, "walls": [n.index for n in walls], "base": _frame(base)}
        envelope_calls.append(call)
        try:
            proof = real_envelope(graph, mouth, walls, base)
        except Exception as error:
            call.update(proof=None, raised=f"{type(error).__name__}: {error}")
            raise
        call["proof"] = None if proof is None else _envelope(proof)
        return proof

    seats._prove, envelopes._prove = seat, envelope
    out: dict = {}
    try:
        graph = FaceGraph(part)
        for name, run, shape in (
            ("seats", _real_seats, _seat),
            ("envelopes", _real_envelopes, _envelope),
        ):
            try:
                out[name] = [shape(p) for p in run(graph)]
            except Exception as error:  # noqa: BLE001
                out[name] = {"refused": f"{type(error).__name__}: {error}"}
    finally:
        seats._prove, envelopes._prove = real_seat, real_envelope
    return {**out, "seat_calls": seat_calls, "envelope_calls": envelope_calls}


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def _finite(value):
    """JSON without NaN or infinities: a non-finite float (a refused input) as its string."""

    if isinstance(value, float) and not math.isfinite(value):
        return str(value)
    if isinstance(value, dict):
        return {key: _finite(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_finite(item) for item in value]
    return value


def _write(name: str, payload: dict) -> None:
    payload = _finite({"quiddity_revision": revision(QUIDDITY), **payload})
    text = json.dumps(payload, allow_nan=False)
    (OUT / name).write_bytes(gzip.compress((text + "\n").encode(), mtime=0))


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    cwd = os.getcwd()
    os.chdir(QUIDDITY)
    try:
        status = pytest.main(
            ["-q", "-p", "no:cacheprovider", "-p", "no:randomly", *TESTS], plugins=[_Recorder()]
        )
    finally:
        os.chdir(cwd)
    print("pytest exit", status, file=sys.stderr)
    _write("values.json.gz", {"skipped": _skipped["values"], "values": list(_values.values())})

    parts_dir = OUT / "parts"
    parts_dir.mkdir(exist_ok=True)
    for old in parts_dir.glob("*.step.gz"):
        old.unlink()
    runs: list[dict] = []
    nothing: dict[str, list[tuple[bool, str, dict, Path]]] = {}
    for sha, (text, tests) in sorted(_parts.items()):
        path = parts_dir / f"{sha}.step.gz"
        path.write_bytes(gzip.compress(text.encode(), mtime=0))
        run = {"source": "test", "file": f"parts/{path.name}", **_proofs(_load(path))}
        proved = any(isinstance(run[k], list) and run[k] for k in ("seats", "envelopes"))
        if proved or tests & set(OWN_TESTS):
            runs.append(run)
            print(path.name, file=sys.stderr)
        else:
            first = min(tests, key=lambda t: (TESTS.index(t) if t in TESTS else len(TESTS), t))
            asked = bool(run["seat_calls"] or run["envelope_calls"])
            nothing.setdefault(first, []).append((not asked, sha, run, path))
    unselected = 0
    for test in sorted(nothing):
        for i, (_, _, run, path) in enumerate(sorted(nothing[test], key=lambda c: c[:2])):
            if i < NOTHING_PER_TEST:
                runs.append(run)
                print(path.name, "(nothing proved)", file=sys.stderr)
            else:
                path.unlink()
                unselected += 1
    runs.sort(key=lambda run: run["file"])
    for name in GOLDEN:
        runs.append({"source": "fixture", "file": name, **_proofs(_load(FIXTURES / name))})
    corpus = json.loads((FIXTURES / "corpus.json").read_text())["files"]
    for name in sorted(entry["file"] for entry in corpus):
        runs.append({"source": "corpus", "file": name, **_proofs(_load(CORPUS / name))})
        print(name, len(runs[-1]["seats"]), len(runs[-1]["envelopes"]), file=sys.stderr)
    _write(
        "proofs.json.gz", {"skipped": _skipped["parts"], "unselected": unselected, "runs": runs}
    )


if __name__ == "__main__":
    main()
