"""Record the cylindrical proofs section-recess geometry composes: ``quiddity._cylindrical_channels``
(``prove_cylindrical_channel``), ``_cylindrical_pockets`` (``cylindrical_pocket_proofs``) and
``_cylindrical_passages`` (``cylindrical_passage_proofs``), which have no public ``recognise_*``
entry point of their own.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_section_geometry.py

Two phases, written to ``tests/fixtures/captured/section_geometry/``:

1. The Python tests in ``TESTS`` run in this process with the three public functions wrapped:
   every part handed to one is kept as STEP (``parts/<sha>.step.gz``), and every channel
   question (its defining and constituent faces, run and width axes and open side) is kept with
   its part.
2. Once the tests are done (so no test's monkeypatch is live), each kept part, the golden passage
   fixtures and every corpus part, each re-imported from STEP as the port reads it, is asked
   again (``proofs.json.gz``), with the private questions wrapped: per part the pocket proofs and
   every ``_proofs(floor)`` question, the passage proofs, the native bores and every
   ``_cell_proof`` question, and every channel question with its answer (a proof, ``null``, or
   the exception ``prove_cylindrical_channel`` swallows). The channel questions on a test part
   are the ones its tests asked; on a fixture or corpus part they are the ones Python's own
   recognition asks (``_take_inventory`` with ``prove_cylindrical_channel`` wrapped), each part
   in a child process given ``PART_TIMEOUT`` seconds (a part that takes longer is recorded as
   timed out, not dropped).

A test part is kept when it came from the proofs' own tests (``OWN_TESTS``) or Python proves
something on it; the rest are deleted again and counted in ``unselected`` (their questions
repeat refusals the kept parts and the corpus already ask, at several MB of STEP). A part that
cannot be exported is counted in ``skipped``.
"""

from __future__ import annotations

import gzip
import hashlib
import json
import math
import multiprocessing
import os
import re
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src"), str(QUIDDITY), str(Path(__file__).resolve().parent)]

import pytest  # noqa: E402
from build123d import export_step  # noqa: E402

import quiddity._cylindrical_channels as channels  # noqa: E402
import quiddity._cylindrical_passages as passages  # noqa: E402
import quiddity._cylindrical_pockets as pockets  # noqa: E402
from _provenance import revision  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402
from quiddity._adjacency import FaceGraph  # noqa: E402
from quiddity._effective_surfaces import EffectiveSurfaceIndex  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "captured" / "section_geometry"
FIXTURES = ROOT / "tests" / "fixtures"
CORPUS = QUIDDITY / "tests" / "corpus"
TESTS = (
    "tests/test_cylindrical_channel_proof.py",
    "tests/test_cylindrical_channel_public.py",
    "tests/test_cylindrical_pocket_proofs.py",
    "tests/test_cylindrical_passage_proof.py",
    "tests/test_cylindrical_passage_public.py",
    "tests/test_mixed_section_pockets.py",
    "tests/test_section_recesses.py",
    "tests/test_section_recess_cutover.py",
    "tests/test_section_recess_geometry_golden.py",
    "tests/test_section_recess_invariants.py",
    "tests/test_open_channel_section.py",
    "tests/test_polygonal_split_mouths.py",
)
OWN_TESTS = (
    "tests/test_cylindrical_channel_proof.py",
    "tests/test_cylindrical_pocket_proofs.py",
    "tests/test_cylindrical_passage_proof.py",
)
GOLDEN = ("golden_hexagonal_passage.step", "golden_interrupted_and_cross_bores.step")
PART_TIMEOUT = 600
WORKERS = 4

# sha -> (STEP text, tests, channel questions)
_parts: dict[str, tuple[str, set[str], list[dict]]] = {}
_skipped = {"parts": 0}


def _step(shape) -> str:
    """The shape as STEP text, its export timestamp fixed so one part has one file name."""

    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        export_step(shape, tmp.name)
        text = Path(tmp.name).read_text()
    return re.sub(r"'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d'", "'2000-01-01T00:00:00'", text, count=1)


def _keep(graph, question: dict | None = None) -> None:
    try:
        text = _step(graph._part)
    except Exception:  # noqa: BLE001
        _skipped["parts"] += 1
        return
    test = os.environ.get("PYTEST_CURRENT_TEST", "").split("::")[0]
    entry = _parts.setdefault(hashlib.sha256(text.encode()).hexdigest()[:16], (text, set(), []))
    entry[1].add(test)
    if question is not None and question not in entry[2]:
        entry[2].append(question)


_real_channel = channels.prove_cylindrical_channel
_real_pockets = pockets.cylindrical_pocket_proofs
_real_passages = passages.cylindrical_passage_proofs


def _channel_question(defining, constituent, run_axis, width_axis, open_sign) -> dict:
    return {
        "defining": sorted(n.index for n in defining),
        "constituent": sorted(n.index for n in constituent),
        "run_axis": run_axis,
        "width_axis": width_axis,
        "open_sign": open_sign,
    }


def _channel(graph, surfaces, defining, constituent, *, run_axis, width_axis, open_sign):
    _keep(graph, _channel_question(defining, constituent, run_axis, width_axis, open_sign))
    return _real_channel(
        graph,
        surfaces,
        defining,
        constituent,
        run_axis=run_axis,
        width_axis=width_axis,
        open_sign=open_sign,
    )


def _pockets(graph, surfaces):
    _keep(graph)
    return _real_pockets(graph, surfaces)


def _passages(graph, surfaces):
    _keep(graph)
    return _real_passages(graph, surfaces)


class _Recorder:
    """Installs the wrappers for the test run and removes them after it."""

    def pytest_sessionstart(self, session) -> None:  # noqa: ARG002
        import quiddity.result  # noqa: F401
        import quiddity._section_recess_geometry  # noqa: F401

        wrappers = {
            id(_real_channel): _channel,
            id(_real_pockets): _pockets,
            id(_real_passages): _passages,
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
        for owner, name, value in self._modules:
            setattr(owner, name, value)


# --- phase 2: proofs on re-imported parts ----------------------------------------------------


def _frame(frame) -> dict:
    return {key: list(getattr(frame, key)) for key in ("origin", "run", "u", "v")}


def _channel_proof(proof) -> dict:
    return {
        "supports": [n.index for n in proof.supports],
        "cylinder": proof.cylinder.index,
        "planar_context": proof.planar_context.index,
        "owner": proof.owner.ordinal,
        "run_axis": proof.run_axis,
        "width_axis": proof.width_axis,
        "open_sign": proof.open_sign,
        "bounds": [list(b) for b in proof.bounds],
        "run_interval": list(proof.run_interval),
        "cylindrical_end": proof.cylindrical_end,
        "axis_point": list(proof.axis_point),
        "axis_direction": list(proof.axis_direction),
        "radius": proof.radius,
        "volume": proof.volume,
    }


def _pocket_proof(proof) -> dict:
    return {
        "floor": proof.floor.index,
        "walls": [n.index for n in proof.walls],
        "stock": proof.stock.index,
        "owner": proof.owner.ordinal,
        "axis_point": list(proof.axis_point),
        "axis_direction": list(proof.axis_direction),
        "run": list(proof.run),
        "radius": proof.radius,
        "volume": proof.volume,
    }


def _passage_proof(proof) -> dict:
    return {
        "walls": [n.index for n in proof.walls],
        "cylinder": proof.cylinder.index,
        "planar_context": proof.planar_context.index,
        "owner": proof.owner.ordinal,
        "frame": _frame(proof.frame),
        "section": [[float(v.point[0]), float(v.point[1]), float(v.bulge)]
                    for v in proof.section.boundary],
        "run_interval": list(proof.run_interval),
        "cylindrical_end": proof.cylindrical_end,
        "axis_point": list(proof.axis_point),
        "axis_direction": list(proof.axis_direction),
        "radius": proof.radius,
        "volume": proof.volume,
    }


def _raised(error: Exception) -> str:
    return f"{type(error).__name__}: {error}"


def _ask_channel(graph, surfaces, question: dict) -> dict:
    """One channel question with its answer: the proof, ``null``, or what ``_prove`` raised."""

    call = dict(question)
    real = channels._prove

    def prove(*args):
        try:
            proof = real(*args)
        except Exception as error:
            call.update(proof=None, raised=_raised(error))
            raise
        call["proof"] = None if proof is None else _channel_proof(proof)
        return proof

    channels._prove = prove
    try:
        nodes = graph.nodes
        if any(i >= len(nodes) for i in question["defining"] + question["constituent"]):
            call.update(proof=None, refused="face index out of range on the re-imported part")
            return call
        _real_channel(
            graph,
            surfaces,
            frozenset(nodes[i] for i in question["defining"]),
            frozenset(nodes[i] for i in question["constituent"]),
            run_axis=question["run_axis"],
            width_axis=question["width_axis"],
            open_sign=question["open_sign"],
        )
    finally:
        channels._prove = real
    return call


def _proofs(part) -> dict:
    """The pocket and passage proofs on *part*, with every private question asked on the way."""

    pocket_calls: list[dict] = []
    passage_calls: list[dict] = []
    real_floor, real_cell = pockets._proofs, passages._cell_proof

    def floor(graph, surfaces, node):
        call = {"floor": node.index}
        pocket_calls.append(call)
        try:
            found = real_floor(graph, surfaces, node)
        except Exception as error:
            call.update(proofs=[], raised=_raised(error))
            raise
        call["proofs"] = [_pocket_proof(p) for p in found]
        return found

    def cell(graph, planar, cylinder, fact, walls, base):
        call = {
            "planar": planar.index,
            "cylinder": cylinder.index,
            "walls": [n.index for n in walls],
            "base": _frame(base),
        }
        passage_calls.append(call)
        try:
            proof = real_cell(graph, planar, cylinder, fact, walls, base)
        except Exception as error:
            call.update(proof=None, raised=_raised(error))
            raise
        call["proof"] = None if proof is None else _passage_proof(proof)
        return proof

    pockets._proofs, passages._cell_proof = floor, cell
    out: dict = {}
    try:
        graph = FaceGraph(part)
        surfaces = EffectiveSurfaceIndex(graph)
        for name, run, shape in (
            ("pockets", _real_pockets, _pocket_proof),
            ("passages", _real_passages, _passage_proof),
        ):
            try:
                out[name] = [shape(p) for p in run(graph, surfaces)]
            except Exception as error:  # noqa: BLE001
                out[name] = {"refused": _raised(error)}
        try:
            out["bores"] = [node.index for node, _ in passages._native_bores(graph, surfaces)]
        except Exception as error:  # noqa: BLE001
            out["bores"] = {"refused": _raised(error)}
    finally:
        pockets._proofs, passages._cell_proof = real_floor, real_cell
    return {**out, "pocket_calls": pocket_calls, "passage_calls": passage_calls}


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def _child(path: str, questions: list[dict] | None, conn) -> None:
    """Everything one part answers, sent piece by piece so a slow recognition loses only the
    channel questions."""

    try:
        part = _load(Path(path))
        conn.send(("proofs", _proofs(part)))
        graph = FaceGraph(part)
        surfaces = EffectiveSurfaceIndex(graph)
        if questions is None:
            # Python's own recognition asks the channel questions on a fixture or corpus part.
            import quiddity.result as result
            from quiddity.result import _take_inventory

            asked: list[dict] = []

            def channel(g, s, defining, constituent, *, run_axis, width_axis, open_sign):
                question = _channel_question(defining, constituent, run_axis, width_axis, open_sign)
                if question not in asked:
                    asked.append(question)
                return _real_channel(
                    g, s, defining, constituent,
                    run_axis=run_axis, width_axis=width_axis, open_sign=open_sign,
                )

            result.prove_cylindrical_channel = channel
            _take_inventory(part)
            questions = asked
        conn.send(("channels", [_ask_channel(graph, surfaces, q) for q in questions]))
    except Exception as error:  # noqa: BLE001
        conn.send(("refused", _raised(error)))


def _job(job: dict) -> dict:
    started = time.monotonic()
    context = multiprocessing.get_context("spawn")
    receive, send = context.Pipe(duplex=False)
    child = context.Process(
        target=_child, args=(str(job["path"]), job.get("questions"), send), daemon=True
    )
    child.start()
    send.close()
    run = {key: job[key] for key in ("source", "file")}
    try:
        while "channel_calls" not in run:
            if not receive.poll(PART_TIMEOUT):
                run["channel_calls"] = {"timeout": PART_TIMEOUT}
                break
            kind, value = receive.recv()
            if kind == "proofs":
                run.update(value)
            elif kind == "channels":
                run["channel_calls"] = value
            else:
                run.setdefault("pockets", {"refused": value})
                run["channel_calls"] = {"refused": value}
    except EOFError:
        run["channel_calls"] = {"refused": "child process died"}
    finally:
        child.kill()
        child.join()
        receive.close()
    for key in ("pockets", "passages", "bores"):
        run.setdefault(key, {"refused": "not answered"})
    run.setdefault("pocket_calls", [])
    run.setdefault("passage_calls", [])
    print(job["file"], f"{time.monotonic() - started:.0f}s", file=sys.stderr, flush=True)
    return run


def _finite(value):
    """JSON without NaN or infinities: a non-finite float as its string."""

    if isinstance(value, float) and not math.isfinite(value):
        return str(value)
    if isinstance(value, dict):
        return {key: _finite(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_finite(item) for item in value]
    return value


def _proved(run: dict) -> bool:
    calls = run.get("channel_calls")
    return any(isinstance(run[k], list) and run[k] for k in ("pockets", "passages")) or (
        isinstance(calls, list) and any(c.get("proof") for c in calls)
    )


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
    tests_of: dict[str, set[str]] = {}
    for sha, (text, tests, questions) in sorted(_parts.items()):
        path = parts_dir / f"{sha}.step.gz"
        path.write_bytes(gzip.compress(text.encode(), mtime=0))
        name = f"parts/{path.name}"
        tests_of[name] = tests
        jobs.append({"source": "test", "file": name, "path": path, "questions": questions})
    for name in GOLDEN:
        jobs.append({"source": "fixture", "file": name, "path": FIXTURES / name})
    corpus = json.loads((FIXTURES / "corpus.json").read_text())["files"]
    for name in sorted(entry["file"] for entry in corpus):
        jobs.append({"source": "corpus", "file": name, "path": CORPUS / name})
    with ThreadPoolExecutor(max_workers=WORKERS) as pool:
        answered = list(pool.map(_job, jobs))
    runs: list[dict] = []
    unselected = 0
    for run in answered:
        if run["source"] == "test" and not (_proved(run) or tests_of[run["file"]] & set(OWN_TESTS)):
            (OUT / run["file"]).unlink()
            unselected += 1
            continue
        runs.append(run)
    payload = _finite(
        {
            "quiddity_revision": revision(QUIDDITY),
            "skipped": _skipped["parts"],
            "unselected": unselected,
            "runs": runs,
        }
    )
    text = json.dumps(payload, allow_nan=False, separators=(",", ":"))
    (OUT / "proofs.json.gz").write_bytes(gzip.compress((text + "\n").encode(), mtime=0))


if __name__ == "__main__":
    main()
