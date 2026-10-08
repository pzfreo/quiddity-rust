"""Record the section helpers' inputs and answers: ``quiddity._sections``,
``_section_passages``, ``_entry_treatments`` and ``_support_patches``, which have no public
``recognise_*`` entry point of their own.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_sections.py

Two phases, written to ``tests/fixtures/captured/sections/``:

1. The Python tests of the four modules (``test_sections.py``, ``test_section_passages.py``,
   ``test_passage_entry_treatments.py``, ``test_support_patches.py``) run in this process with
   the value helpers wrapped. Every distinct ``PlanarSection`` built, ``LocalFrame.canonical`` /
   ``principal`` call, ``validate_section_end_separation`` call and ``occurrence_geometry_dict``
   call is recorded with its answer or its refusal (``values.json.gz``); every
   ``covered_patch`` question whose faces are straight-edged polygons without holes is recorded
   as their corner loops (``patches.json.gz``); every part handed to
   ``section_ring_proposals`` is kept as STEP (``parts/<sha>.step.gz``).
2. Once the tests are done (so no test's monkeypatch is live), ``section_ring_proposals`` runs on
   each kept part, the golden fixtures and every corpus part, each re-imported from STEP as the
   port reads it, with ``prove_entry_treatments`` wrapped (``proposals.json.gz``): per part its
   proposals and every entry-treatment question with its answer.

Python's answers are not bit-stable between runs: it walks sets of run-local objects whose
order follows their addresses, and sums in that order, so a recapture moves last digits (and
with them which near-duplicate questions are kept, and the order of proposals whose sort keys
tie to round-off). ``tests/sections.rs`` compares to one part in a million and treats such ties
as one.

A ``covered_patch`` question, a value call refused for a body reference (Python's run-local
object identity, which has no value to carry) or a part that cannot be exported is counted, not
silently dropped: the counts are in each file's ``skipped``.
"""

from __future__ import annotations

import gzip
import hashlib
import inspect
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
from OCP.BRep import BRep_Tool  # noqa: E402
from OCP.BRepTools import BRepTools_WireExplorer  # noqa: E402
from OCP.TopExp import TopExp  # noqa: E402

import quiddity._section_passages as passages  # noqa: E402
import quiddity._sections as sections  # noqa: E402
import quiddity._support_patches as support_patches  # noqa: E402
from _provenance import revision  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402
from quiddity._adjacency import FaceGraph  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "captured" / "sections"
FIXTURES = ROOT / "tests" / "fixtures"
CORPUS = QUIDDITY / "tests" / "corpus"
TESTS = (
    "tests/test_sections.py",
    "tests/test_section_passages.py",
    "tests/test_passage_entry_treatments.py",
    "tests/test_support_patches.py",
)
GOLDEN = ("golden_hexagonal_passage.step", "golden_interrupted_and_cross_bores.step")

_values: dict[str, dict] = {}
_patches: dict[str, dict] = {}
_parts: dict[str, str] = {}
_skipped = {"values": 0, "patches": 0, "parts": 0}


def _key(record: dict) -> str:
    return json.dumps(record, sort_keys=True)


def _vertices(boundary) -> list[list[float]]:
    return [[float(v.point[0]), float(v.point[1]), float(v.bulge)] for v in boundary]


def _record_value(kind: str, given: dict, answer: dict) -> None:
    record = {"kind": kind, "given": given}
    _values.setdefault(_key(record), {**record, **answer})


def _refusal(error: Exception) -> dict:
    return {"refused": str(error)}


# --- the value helpers -----------------------------------------------------------------------

_real_post_init = sections.PlanarSection.__post_init__


def _post_init(self) -> None:
    try:
        given = {"boundary": _vertices(self.boundary)}
    except (AttributeError, TypeError):
        _skipped["values"] += 1
        return _real_post_init(self)
    try:
        _real_post_init(self)
    except ValueError as error:
        _record_value("section", given, _refusal(error))
        raise
    area, centroid = sections._moments(self.boundary)
    _record_value(
        "section",
        given,
        {"boundary": _vertices(self.boundary), "area": area, "centroid": list(centroid)},
    )


def _frame(frame) -> dict:
    return {key: list(getattr(frame, key)) for key in ("origin", "run", "u", "v")}


def _wrap_frame(name: str):
    real = getattr(sections.LocalFrame, name).__func__

    def wrapped(cls, first, centroid):
        given = {"first": first if isinstance(first, str) else list(first), "centroid": list(centroid)}
        try:
            frame = real(cls, first, centroid)
        except ValueError as error:
            _record_value(name, given, _refusal(error))
            raise
        _record_value(name, given, {"frame": _frame(frame)})
        return frame

    return classmethod(wrapped)


_real_separation = sections.validate_section_end_separation


def _separation(vertices, span, gradient, *, closed):
    given = {
        "vertices": _vertices(vertices),
        "span": span,
        "gradient": list(gradient),
        "closed": closed,
    }
    try:
        _real_separation(vertices, span, gradient, closed=closed)
    except ValueError as error:
        _record_value("separation", given, _refusal(error))
        raise
    _record_value("separation", given, {"accepted": True})


_real_geometry = sections.occurrence_geometry_dict


def _geometry(occurrence, *, body_refs):
    try:
        body_refs.validate(occurrence.body)
        given = {
            "frame": _frame(occurrence.frame),
            "run_interval": list(occurrence.run_interval),
            "boundary": _vertices(occurrence.section.boundary),
            "ends": [occurrence.ends.low_capped, occurrence.ends.high_capped],
        }
    except (AttributeError, TypeError, ValueError):
        _skipped["values"] += 1
        return _real_geometry(occurrence, body_refs=body_refs)
    try:
        answer = _real_geometry(occurrence, body_refs=body_refs)
    except ValueError as error:
        _record_value("geometry", given, _refusal(error))
        raise
    _record_value("geometry", given, {"geometry": answer})
    return answer


# --- support patches and parts ---------------------------------------------------------------


def _polygon(face) -> list[list[float]] | None:
    """A planar face bounded by one loop of straight edges, as its corners in loop order."""

    try:
        if face.geom_type.name != "PLANE" or len(face.inner_wires()) != 0:
            return None
        wire = face.outer_wire()
        if any(edge.geom_type.name != "LINE" for edge in wire.edges()):
            return None
        corners = []
        walk = BRepTools_WireExplorer(wire.wrapped, face.wrapped)
        while walk.More():
            point = BRep_Tool.Pnt_s(TopExp.FirstVertex_s(walk.Current(), True))
            corners.append([point.X(), point.Y(), point.Z()])
            walk.Next()
        return corners if len(corners) >= 3 else None
    except Exception:  # noqa: BLE001
        return None


_real_covered = support_patches.covered_patch


def _covered(patch, supports):
    answer = _real_covered(patch, supports)
    loops = [_polygon(face) for face in (patch, *supports)]
    if any(loop is None for loop in loops):
        _skipped["patches"] += 1
    else:
        record = {"patch": loops[0], "supports": loops[1:]}
        _patches.setdefault(_key(record), {**record, "covered": answer})
    return answer


def _step(shape) -> str:
    """The shape as STEP text, its export timestamp fixed so one part has one file name."""

    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        export_step(shape, tmp.name)
        text = Path(tmp.name).read_text()
    return re.sub(r"'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d'", "'2000-01-01T00:00:00'", text, count=1)


_real_proposals = passages.section_ring_proposals


def _keep_part(part, graph):
    try:
        text = _step(part)
    except Exception:  # noqa: BLE001
        _skipped["parts"] += 1
    else:
        _parts.setdefault(hashlib.sha256(text.encode()).hexdigest()[:16], text)
    return _real_proposals(part, graph)


class _Recorder:
    """Installs the wrappers for the test run and removes them after it."""

    def pytest_sessionstart(self, session) -> None:  # noqa: ARG002
        import quiddity.oriented_slots  # noqa: F401
        import quiddity.passages  # noqa: F401

        self._classes = [
            (sections.PlanarSection, "__post_init__", _real_post_init),
            (sections.LocalFrame, "canonical", sections.LocalFrame.__dict__["canonical"]),
            (sections.LocalFrame, "principal", sections.LocalFrame.__dict__["principal"]),
        ]
        sections.PlanarSection.__post_init__ = _post_init
        sections.LocalFrame.canonical = _wrap_frame("canonical")
        sections.LocalFrame.principal = _wrap_frame("principal")
        # Every module holding one of the real functions gets the wrapper, wherever it imported
        # it from; the test modules import them after this, from the wrapped modules.
        wrappers = {
            id(_real_separation): _separation,
            id(_real_geometry): _geometry,
            id(_real_proposals): _keep_part,
            id(_real_covered): _covered,
        }
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


# --- phase 2: proposals on re-imported parts --------------------------------------------------


def _proposal(proposal) -> dict:
    occurrence = proposal.occurrence
    return {
        "frame": _frame(occurrence.frame),
        "run_interval": list(occurrence.run_interval),
        "boundary": _vertices(occurrence.section.boundary),
        "ends": [occurrence.ends.low_capped, occurrence.ends.high_capped],
        "nodes": [node.index for node in proposal.nodes],
        "solid": proposal.solid.ordinal,
        "constituent": sorted(node.index for node in proposal.constituent),
        "low_gradient": list(proposal.low_gradient),
        "high_gradient": list(proposal.high_gradient),
    }


def _proposals(part) -> dict:
    treatments: list[dict] = []
    real = passages.prove_entry_treatments

    def wrapped(graph, seed, wire, run, at, far):
        opening = inspect.stack()[1].frame.f_locals["opening"]
        inner = graph.face(opening).inner_wires()
        position = next(i for i, w in enumerate(inner) if w.is_same(wire))
        proof = real(graph, seed, wire, run, at, far)
        treatments.append(
            {
                "opening": opening.index,
                "inner_wire": position,
                "seed": sorted(node.index for node in seed),
                "run": list(run),
                "at": at,
                "far": far,
                "proof": None
                if proof is None
                else {
                    "treatments": sorted(n.index for n in proof.treatments),
                    "stock": sorted(n.index for n in proof.stock),
                },
            }
        )
        return proof

    passages.prove_entry_treatments = wrapped
    try:
        found = _real_proposals(part, FaceGraph(part))
    except Exception as error:  # noqa: BLE001
        return {"refused": f"{type(error).__name__}: {error}", "entry_treatments": treatments}
    finally:
        passages.prove_entry_treatments = real
    return {"proposals": [_proposal(p) for p in found], "entry_treatments": treatments}


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
    # Three operation-count tests in test_support_patches.py fail under the recorder: they count
    # Face.cut calls made from covered_patch's own code object, which the recorder wraps.
    print("pytest exit", status, file=sys.stderr)
    _write("values.json.gz", {"skipped": _skipped["values"], "values": list(_values.values())})
    _write("patches.json.gz", {"skipped": _skipped["patches"], "patches": list(_patches.values())})

    parts_dir = OUT / "parts"
    parts_dir.mkdir(exist_ok=True)
    for old in parts_dir.glob("*.step.gz"):
        old.unlink()
    runs: list[dict] = []
    for sha, text in sorted(_parts.items()):
        path = parts_dir / f"{sha}.step.gz"
        path.write_bytes(gzip.compress(text.encode(), mtime=0))
        runs.append({"source": "test", "file": f"parts/{path.name}", **_proposals(_load(path))})
        print(path.name, file=sys.stderr)
    for name in GOLDEN:
        runs.append({"source": "fixture", "file": name, **_proposals(_load(FIXTURES / name))})
    corpus = json.loads((FIXTURES / "corpus.json").read_text())["files"]
    for name in sorted(entry["file"] for entry in corpus):
        runs.append({"source": "corpus", "file": name, **_proposals(_load(CORPUS / name))})
        print(name, len(runs[-1].get("proposals", [])), file=sys.stderr)
    _write("proposals.json.gz", {"skipped": _skipped["parts"], "runs": runs})


if __name__ == "__main__":
    main()
