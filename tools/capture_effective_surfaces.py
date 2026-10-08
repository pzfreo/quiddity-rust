"""Record ``quiddity._effective_surfaces``'s answers for every face of the parts its consumers'
tests build, the golden fixtures and the corpus: the effective surface fact (or its refusal),
``recovery_nominal`` / ``recovery_tolerance`` (or the refusal) with the face's area (``face.area`` and the adaptive
rule) and physical perimeter it is made of (``Edge.length``, and each edge integrated to 1e-10),
and the surface use with a
material-side certificate (``use(face, material_side=True)``: the certificate, or its refusal).

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_effective_surfaces.py

Two phases, written to ``tests/fixtures/captured/effective_surfaces/``:

1. The Python tests in ``TESTS`` run in this process with ``EffectiveSurfaceIndex.__init__``
   and ``recovery_nominal`` wrapped: every part an index is built over, and every face handed to
   ``recovery_nominal`` on its own, is kept as STEP (``parts/<sha>.step.gz``). Many of those tests
   monkeypatch the kernel to force a refusal; their answers are not recorded, only their parts.
   At most ``MAX_PER_TEST`` distinct parts are kept per test function and none above
   ``MAX_TEST_FACES`` faces, so parametrised sweeps do not swell the fixture (both counted).
2. Once the tests are done (so no monkeypatch is live), each kept part, the golden fixtures and
   every corpus part, each re-imported from STEP as the port reads it, is queried face by face
   through one ``FaceGraph`` and ``effective_faces_for_graph`` (``faces.json.gz``).

The certificate's sample points come from OpenCascade's mesh, which the port does not
reproduce, so only what consumers decide from is recorded: the sign, the outward direction (the
plane normal, or a cylinder's first radial sample), the number of samples and the probe
distance. A part that cannot be exported is counted in ``skipped``, and a face whose certificate
OpenCascade takes more than ``FACE_TIMEOUT`` seconds to mesh has its use recorded as timed out:
neither is dropped silently.
"""

from __future__ import annotations

import gzip
import hashlib
import json
import math
import os
import re
import sys
import multiprocessing
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src"), str(QUIDDITY)]

import pytest  # noqa: E402
from build123d import export_step  # noqa: E402
from OCP.BRep import BRep_Tool  # noqa: E402
from OCP.BRepAdaptor import BRepAdaptor_Curve  # noqa: E402
from OCP.BRepGProp import BRepGProp  # noqa: E402
from OCP.GCPnts import GCPnts_AbscissaPoint  # noqa: E402
from OCP.GProp import GProp_GProps  # noqa: E402

import quiddity._effective_surfaces as effective  # noqa: E402
from _provenance import revision  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402
from quiddity._adjacency import FaceGraph  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "captured" / "effective_surfaces"
FIXTURES = ROOT / "tests" / "fixtures"
CORPUS = QUIDDITY / "tests" / "corpus"
# The helper's own tests and those of the consumers the brief names (pads, the cylindrical
# channel/pocket/passage proofs, circular blind steps through cylinder_surface_dependency) and
# the NURBS conversion sweep, which builds exact B-spline primitives.
TESTS = (
    "tests/test_effective_surfaces.py",
    "tests/test_pad_attribution.py",
    "tests/test_cylindrical_channel_proof.py",
    "tests/test_cylindrical_passage_proof.py",
    "tests/test_cylindrical_pocket_proofs.py",
    "tests/test_circular_blind_steps.py",
    "tests/test_nurbs_conversion_sweep.py",
)
GOLDEN = ("golden_hexagonal_passage.step", "golden_interrupted_and_cross_bores.step")
# Parts above this many faces are left out of phase 1 (counted): the corpus covers large parts.
MAX_TEST_FACES = 400
# Distinct parts kept per test function (its parametrisations together), in the order the test
# builds them, so a sweep over hundreds of variants does not swell the fixture (counted).
MAX_PER_TEST = 3

# Seconds a child may take over one face's answer before that face's use is recorded as timed
# out, and how many parts are answered at once.
FACE_TIMEOUT = 120
WORKERS = max(1, (os.cpu_count() or 2) // 2)

_parts: dict[str, tuple[str, str]] = {}
_per_test: dict[str, int] = {}
_skipped = {"export": 0, "large": 0, "per_test": 0}


def _test() -> str:
    """The running test function, without its parametrisation."""

    current = os.environ.get("PYTEST_CURRENT_TEST", "").split(" ")[0]
    return current.split("[")[0]


def _step(shape) -> str:
    """The shape as STEP text, its export timestamp fixed so one part has one file name."""

    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        if not export_step(shape, tmp.name):
            raise RuntimeError("export_step refused")
        text = Path(tmp.name).read_text()
    return re.sub(r"'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d'", "'2000-01-01T00:00:00'", text, count=1)


def _keep(shape) -> None:
    try:
        if len(shape.faces()) > MAX_TEST_FACES:
            _skipped["large"] += 1
            return
        text = _step(shape)
    except Exception:  # noqa: BLE001
        _skipped["export"] += 1
        return
    sha = hashlib.sha256(text.encode()).hexdigest()[:16]
    if sha in _parts:
        return
    test = _test()
    if _per_test.get(test, 0) >= MAX_PER_TEST:
        _skipped["per_test"] += 1
        return
    _per_test[test] = _per_test.get(test, 0) + 1
    _parts[sha] = (test, text)


_real_index_init = effective.EffectiveSurfaceIndex.__init__
_real_nominal = effective.recovery_nominal


def _index_init(self, graph) -> None:
    _keep(graph._part)
    _real_index_init(self, graph)


def _nominal(face):
    _keep(face)
    return _real_nominal(face)


class _Recorder:
    """Installs the wrappers for the test run and removes them after it."""

    def pytest_sessionstart(self, session) -> None:  # noqa: ARG002
        effective.EffectiveSurfaceIndex.__init__ = _index_init
        self._modules = []
        for module in list(sys.modules.values()):
            if not getattr(module, "__name__", "").startswith("quiddity"):
                continue
            for name, value in list(vars(module).items()):
                if value is _real_nominal:
                    self._modules.append((module, name, value))
                    setattr(module, name, _nominal)

    def pytest_sessionfinish(self, session, exitstatus) -> None:  # noqa: ARG002
        effective.EffectiveSurfaceIndex.__init__ = _real_index_init
        for owner, name, value in self._modules:
            setattr(owner, name, value)


# --- phase 2: every face of each re-imported part ---------------------------------------------


def _fact(fact) -> dict:
    if isinstance(fact, effective.RefusedSurfaceFact):
        return {"refused": fact.reason.value}
    return {
        "kind": fact.kind.value,
        "provenance": fact.provenance.value,
        "parameters": list(fact.parameters),
        "requested_tolerance": fact.requested_tolerance,
        "gap": fact.kernel_reported_gap,
    }


def _use(use) -> dict:
    if isinstance(use, effective.SurfaceUseRefusal):
        return {"refused": use.reason.value}
    side = use.material_side
    return {
        "sign": side.candidate_outward_sign,
        "outward": list(side.outward),
        "samples": len(side.sample_points),
        "probe_distance": side.probe_distance,
    }


def _adaptive_area(face) -> float:
    props = GProp_GProps()
    BRepGProp.SurfaceProperties_s(face, props, 1e-9)
    return float(props.Mass())


def _precise_perimeter(face) -> float:
    """``_physical_boundary_length`` with each edge's length integrated to 1e-10
    (``Edge.length`` uses ``GCPnts_AbscissaPoint``'s default tolerance)."""

    return math.fsum(
        GCPnts_AbscissaPoint.Length_s(BRepAdaptor_Curve(edge.wrapped), 1e-10)
        for edge in face.edges()
        if not BRep_Tool.IsClosed_s(edge.wrapped, face.wrapped)
        and not BRep_Tool.Degenerated_s(edge.wrapped)
    )


def _child(path: str, start: int, conn) -> None:
    """Answer the faces of one part from *start* on, sending each face's fact and lengths, then
    its use, as soon as each is known (the parent times out a face that meshes too long)."""

    try:
        part = _load(Path(path))
        graph = FaceGraph(part)
        query = effective.effective_faces_for_graph(graph)
        conn.send(("count", len(graph.nodes)))
        for node in graph.nodes[start:]:
            face = graph.face(node)
            try:
                lengths = {
                    "nominal": effective.recovery_nominal(face),
                    "tolerance": effective.recovery_tolerance(face),
                }
            except ValueError as error:
                lengths = {"nominal_refused": str(error)}
            # What the nominal is made of, and the evidence where face.area's fixed Gauss rule
            # is coarse (the adaptive rule, as tools/capture_face_areas.py records it).
            lengths["area"] = float(face.area)
            lengths["adaptive_area"] = _adaptive_area(face.wrapped)
            lengths["perimeter"] = float(effective._physical_boundary_length(face))
            lengths["precise_perimeter"] = _precise_perimeter(face)
            conn.send(("head", {"face": node.index, "fact": _fact(query.fact(face)), **lengths}))
            conn.send(("use", _use(query.use(face, material_side=True))))
        conn.send(("done", None))
    except Exception as error:  # noqa: BLE001
        conn.send(("refused", f"{type(error).__name__}: {error}"))


def _faces(path: Path) -> dict:
    """Every face of the part at *path*, each in a child process that is given ``FACE_TIMEOUT``
    seconds per answer: OpenCascade meshes a large B-spline face at the probe distance, which
    can take hours; such a face's use is recorded as ``{"timeout": true}`` (not captured)."""

    context = multiprocessing.get_context("spawn")
    faces: list[dict] = []
    start = 0
    while True:
        receive, send = context.Pipe(duplex=False)
        child = context.Process(target=_child, args=(str(path), start, send), daemon=True)
        child.start()
        send.close()
        timed_out = False
        try:
            while True:
                if not receive.poll(FACE_TIMEOUT):
                    timed_out = True
                    break
                kind, value = receive.recv()
                if kind == "refused":
                    return {"refused": value}
                if kind == "done":
                    return {"faces": faces}
                if kind == "head":
                    faces.append(value)
                elif kind == "use":
                    faces[-1]["use"] = value
        except EOFError:
            return {"refused": "child process died", "faces": faces}
        finally:
            child.kill()
            child.join()
            receive.close()
        if timed_out:
            if faces and "use" not in faces[-1]:
                faces[-1]["use"] = {"timeout": True}
                start = len(faces)
            else:
                return {"refused": "timed out before a face was answered", "faces": faces}


def _job(job: dict) -> dict:
    path = job.pop("path")
    started = time.monotonic()
    result = {**job, **_faces(path)}
    print(job["file"], len(result.get("faces", [])), f"{time.monotonic() - started:.0f}s",
          file=sys.stderr, flush=True)
    return result


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def _finite(value):
    """JSON without NaN or infinities: a non-finite float as its string."""

    if isinstance(value, float) and not math.isfinite(value):
        return str(value)
    if isinstance(value, dict):
        return {key: _finite(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_finite(item) for item in value]
    return value


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

    parts_dir = OUT / "parts"
    parts_dir.mkdir(exist_ok=True)
    for old in parts_dir.glob("*.step.gz"):
        old.unlink()
    jobs: list[dict] = []
    for sha, (test, text) in sorted(_parts.items()):
        path = parts_dir / f"{sha}.step.gz"
        path.write_bytes(gzip.compress(text.encode(), mtime=0))
        jobs.append({"source": "test", "file": f"parts/{path.name}", "test": test, "path": path})
    for name in GOLDEN:
        jobs.append({"source": "fixture", "file": name, "path": FIXTURES / name})
    corpus = json.loads((FIXTURES / "corpus.json").read_text())["files"]
    for name in sorted(entry["file"] for entry in corpus):
        jobs.append({"source": "corpus", "file": name, "path": CORPUS / name})
    with ThreadPoolExecutor(max_workers=WORKERS) as pool:
        runs = list(pool.map(_job, jobs))
    payload = _finite(
        {"quiddity_revision": revision(QUIDDITY), "skipped": _skipped, "runs": runs}
    )
    text = json.dumps(payload, allow_nan=False, separators=(",", ":"))
    (OUT / "faces.json.gz").write_bytes(gzip.compress((text + "\n").encode(), mtime=0))


if __name__ == "__main__":
    main()
