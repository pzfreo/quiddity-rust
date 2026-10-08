"""Record ``quiddity._section_recess_geometry``: the section-recess candidates and their geometry
(``_candidates``, the three floor readers, the seat, cylindrical-pocket, cylindrical-passage,
plane-envelope and cylindrical-channel projections, and ``has_physical_planar_floor``), which
have no ``recognise_*`` entry point of their own.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_section_recess_geometry.py

Written to ``tests/fixtures/captured/section_recess_geometry/``:

1. The Python tests in ``TESTS`` run in this process with ``_candidates`` and
   ``has_physical_planar_floor`` wrapped: every part handed to one is kept as STEP
   (``parts/<sha>.step.gz``), and every ``has_physical_planar_floor`` question (walls,
   constituent faces, axis, open side, published floor) is kept with its part.
2. Once the tests are done, each kept part, the golden passage fixtures and every corpus part,
   each re-imported from STEP as the port reads it, is asked again (``calls.json.gz``), each in
   a child process given ``PART_TIMEOUT`` seconds (a part that takes longer is recorded as timed
   out, not dropped):

   - ``candidates``: ``_candidates(graph, surfaces)``, every candidate with its faces, mouth,
     body, shape, kind and geometry (``to_dict()``);
   - ``floors``: ``_one_obround_candidate``, ``_one_polygonal_candidate`` and
     ``_one_mixed_candidate`` asked of every planar face on its own (not only where the one
     before refused), with ``planar`` the number of faces asked; only the answers that are not
     ``None`` are listed, so every other planar face's three answers are ``None``;
   - ``seats``, ``pockets``, ``passages``, ``envelopes``: each proof Python's provers give, with
     what ``_seat_geometry``, ``_cylindrical_candidate``, ``_cylindrical_geometry`` (as
     ``_candidates`` calls it for a passage) and ``_plane_envelope_geometry`` make of it, or the
     exception they raise (which ``_candidates`` swallows);
   - ``floor_questions``: every ``has_physical_planar_floor`` question with its answer; on a test
     part those its tests asked, on a fixture or corpus part those Python's own section-recess
     projection asks (``build_section_recess_document`` with the function wrapped).

``channels.json.gz`` holds ``cylindrical_channel_geometry`` (kernel-free) of every channel proof
``tools/capture_section_geometry.py`` recorded (``section_geometry/proofs.json.gz``), with the
proof, keyed by that fixture's file and defining faces.

A test part is kept when Python finds something on it (a candidate, a floor answer, a proof) or
it was asked a floor question; of the rest, ``NOTHING_PER_TEST`` per test are kept (the port must
find nothing there too) and the others deleted again and counted in ``unselected``. A part that
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
from types import SimpleNamespace

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src"), str(QUIDDITY), str(Path(__file__).resolve().parent)]

import pytest  # noqa: E402
from build123d import export_step  # noqa: E402

import quiddity._section_recess_geometry as geometry  # noqa: E402
from _provenance import revision  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402
from quiddity._adjacency import FaceGraph  # noqa: E402
from quiddity._cylindrical_passages import cylindrical_passage_proofs  # noqa: E402
from quiddity._cylindrical_pockets import cylindrical_pocket_proofs  # noqa: E402
from quiddity._cylindrical_seats import cylindrical_seat_proofs  # noqa: E402
from quiddity._effective_surfaces import EffectiveSurfaceIndex  # noqa: E402
from quiddity._plane_envelope_passages import plane_envelope_passage_proofs  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "captured" / "section_recess_geometry"
FIXTURES = ROOT / "tests" / "fixtures"
CORPUS = QUIDDITY / "tests" / "corpus"
TESTS = (
    "tests/test_section_recesses.py",
    "tests/test_section_recess_invariants.py",
    "tests/test_section_recess_cutover.py",
    "tests/test_section_recess_geometry_golden.py",
    "tests/test_section_adapter_rounding.py",
    "tests/test_section_projection_refusals.py",
    "tests/test_corner_section.py",
    "tests/test_open_channel_section.py",
    "tests/test_cylindrical_channel_public.py",
    "tests/test_cylindrical_passage_public.py",
    "tests/test_cylindrical_pocket_proofs.py",
    "tests/test_plane_envelope_passages.py",
    "tests/test_mixed_section_pockets.py",
    "tests/test_polygonal_split_mouths.py",
    "tests/test_support_patches.py",
    "tests/test_volume_probe_scope.py",
    "tests/test_pocket_physical_caps.py",
)
GOLDEN = ("golden_hexagonal_passage.step", "golden_interrupted_and_cross_bores.step")
PART_TIMEOUT = 600
WORKERS = 4
NOTHING_PER_TEST = 2

# sha -> (STEP text, tests, floor questions)
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


_real_candidates = geometry._candidates
_real_floor = geometry.has_physical_planar_floor


def _floor_question(walls, constituent, axis, open_sign, published_floor) -> dict:
    return {
        "walls": sorted(n.index for n in walls),
        "constituent": sorted(n.index for n in constituent),
        "axis": axis,
        "open_sign": open_sign,
        "published_floor": published_floor,
    }


def _candidates(graph, surfaces):
    _keep(graph)
    return _real_candidates(graph, surfaces)


def _floor(graph, walls, constituent, *, axis, open_sign, published_floor):
    _keep(graph, _floor_question(walls, constituent, axis, open_sign, published_floor))
    return _real_floor(
        graph, walls, constituent, axis=axis, open_sign=open_sign, published_floor=published_floor
    )


class _Recorder:
    """Installs the wrappers for the test run and removes them after it."""

    def pytest_sessionstart(self, session) -> None:  # noqa: ARG002
        import quiddity._section_recess_discovery  # noqa: F401
        import quiddity.result  # noqa: F401

        wrappers = {id(_real_candidates): _candidates, id(_real_floor): _floor}
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


# --- phase 2: calls on re-imported parts -----------------------------------------------------


def _frame(frame) -> dict:
    return {key: list(getattr(frame, key)) for key in ("origin", "run", "u", "v")}


def _vertices(boundary) -> list[list[float]]:
    return [[float(v.point[0]), float(v.point[1]), float(v.bulge)] for v in boundary]


def _candidate(candidate) -> dict:
    return {
        "defining": list(candidate.defining_faces),
        "constituent": list(candidate.constituent_faces),
        "mouth": candidate.mouth,
        "body": candidate.body,
        "section_shape": candidate.section_shape,
        "feature_kind": candidate.feature_kind,
        "geometry": candidate.geometry.to_dict(),
    }


def _seat(proof) -> dict:
    return {
        "walls": [n.index for n in proof.walls],
        "context": [n.index for n in proof.context],
        "owner": proof.owner.ordinal,
        "frame": _frame(proof.frame),
        "boundary": _vertices(proof.boundary),
        "run_interval": list(proof.run_interval),
    }


def _pocket(proof) -> dict:
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


def _passage(proof) -> dict:
    return {
        "walls": [n.index for n in proof.walls],
        "cylinder": proof.cylinder.index,
        "planar_context": proof.planar_context.index,
        "owner": proof.owner.ordinal,
        "frame": _frame(proof.frame),
        "section": _vertices(proof.section.boundary),
        "run_interval": list(proof.run_interval),
        "cylindrical_end": proof.cylindrical_end,
        "axis_point": list(proof.axis_point),
        "axis_direction": list(proof.axis_direction),
        "radius": proof.radius,
        "volume": proof.volume,
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


def _raised(error: Exception) -> str:
    return f"{type(error).__name__}: {error}"


def _answer(call: dict, key: str, make) -> dict:
    """*call* with *key* set to what *make* returns, or ``raised`` set to its exception."""

    try:
        call[key] = make()
    except Exception as error:  # noqa: BLE001
        call["raised"] = _raised(error)
    return call


def _passage_geometry(proof):
    return geometry._cylindrical_geometry(
        proof,
        proof.frame,
        proof.section,
        proof.run_interval[1 - proof.cylindrical_end],
        -1 if proof.cylindrical_end == 1 else 1,
        proof.cylindrical_end,
        planar_open=True,
    )


def _calls(part) -> dict:
    """Every call on *part* but the floor questions (see the module docstring)."""

    graph = FaceGraph(part)
    surfaces = EffectiveSurfaceIndex(graph)
    out: dict = {}
    try:
        out["candidates"] = [_candidate(c) for c in _real_candidates(graph, surfaces)]
    except Exception as error:  # noqa: BLE001
        out["candidates"] = {"refused": _raised(error)}
    floors = []
    planar = 0
    readers = (
        ("obround", geometry._one_obround_candidate),
        ("polygonal", geometry._one_polygonal_candidate),
        ("mixed", geometry._one_mixed_candidate),
    )
    for node in graph.nodes:
        if not graph.is_planar(node):
            continue
        planar += 1
        for kind, reader in readers:
            call = {"floor": node.index, "kind": kind}
            try:
                found = reader(graph, node)
            except Exception as error:  # noqa: BLE001
                floors.append({**call, "raised": _raised(error)})
                continue
            if found is not None:
                floors.append({**call, "candidate": _candidate(found)})
    out["planar"] = planar
    out["floors"] = floors
    for name, prover, shape, project in (
        ("seats", lambda: cylindrical_seat_proofs(graph), _seat, geometry._seat_geometry),
        (
            "pockets",
            lambda: cylindrical_pocket_proofs(graph, surfaces),
            _pocket,
            lambda p: _candidate(geometry._cylindrical_candidate(graph, p)),
        ),
        (
            "passages",
            lambda: cylindrical_passage_proofs(graph, surfaces),
            _passage,
            _passage_geometry,
        ),
        (
            "envelopes",
            lambda: plane_envelope_passage_proofs(graph),
            _envelope,
            geometry._plane_envelope_geometry,
        ),
    ):
        try:
            proofs = prover()
        except Exception as error:  # noqa: BLE001
            out[name] = {"refused": _raised(error)}
            continue
        calls = []
        for proof in proofs:
            key = "candidate" if name == "pockets" else "geometry"
            value = (lambda p=proof: project(p)) if name == "pockets" else (
                lambda p=proof: project(p).to_dict()
            )
            calls.append(_answer({"proof": shape(proof)}, key, value))
        out[name] = calls
    return out


def _ask_floor(graph, question: dict) -> dict:
    nodes = graph.nodes
    call = dict(question)
    if any(i >= len(nodes) for i in question["walls"] + question["constituent"]):
        call["refused"] = "face index out of range on the re-imported part"
        return call
    return _answer(
        call,
        "answer",
        lambda: _real_floor(
            graph,
            frozenset(nodes[i] for i in question["walls"]),
            frozenset(nodes[i] for i in question["constituent"]),
            axis=question["axis"],
            open_sign=question["open_sign"],
            published_floor=question["published_floor"],
        ),
    )


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def _child(path: str, questions: list[dict] | None, conn) -> None:
    """Everything one part answers, sent piece by piece so a slow projection loses only the
    floor questions."""

    try:
        part = _load(Path(path))
        conn.send(("calls", _calls(part)))
        graph = FaceGraph(part)
        if questions is None:
            # Python's own section-recess projection asks the floor questions here.
            import quiddity.result as result

            asked: list[dict] = []

            def floor(g, walls, constituent, *, axis, open_sign, published_floor):
                question = _floor_question(walls, constituent, axis, open_sign, published_floor)
                if question not in asked:
                    asked.append(question)
                return _real_floor(
                    g, walls, constituent,
                    axis=axis, open_sign=open_sign, published_floor=published_floor,
                )

            result.has_physical_planar_floor = floor
            try:
                result.build_section_recess_document(part)
            except Exception:  # noqa: BLE001
                pass  # the questions asked before a refusal are still Python's questions
            questions = asked
        conn.send(("floor_questions", [_ask_floor(graph, q) for q in questions]))
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
        while "floor_questions" not in run:
            if not receive.poll(PART_TIMEOUT):
                run["floor_questions"] = {"timeout": PART_TIMEOUT}
                break
            kind, value = receive.recv()
            if kind == "calls":
                run.update(value)
            elif kind == "floor_questions":
                run["floor_questions"] = value
            else:
                run.setdefault("candidates", {"refused": value})
                run["floor_questions"] = {"refused": value}
    except EOFError:
        run["floor_questions"] = {"refused": "child process died"}
    finally:
        child.kill()
        child.join()
        receive.close()
    for key in ("candidates", "seats", "pockets", "passages", "envelopes"):
        run.setdefault(key, {"refused": "not answered"})
    run.setdefault("floors", [])
    run.setdefault("planar", 0)
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


def _found(run: dict) -> bool:
    lists = ("candidates", "seats", "pockets", "passages", "envelopes", "floors")
    questions = run.get("floor_questions")
    return any(isinstance(run[k], list) and run[k] for k in lists) or (
        isinstance(questions, list) and bool(questions)
    )


def _channels() -> dict:
    """``cylindrical_channel_geometry`` of every channel proof the section-geometry capture
    recorded."""

    source = FIXTURES / "captured" / "section_geometry" / "proofs.json.gz"
    captured = json.loads(gzip.decompress(source.read_bytes()))
    calls = []
    for run in captured["runs"]:
        if not isinstance(run.get("channel_calls"), list):
            continue
        for call in run["channel_calls"]:
            proof = call.get("proof")
            if not proof:
                continue
            value = SimpleNamespace(
                **{
                    **proof,
                    "bounds": tuple(tuple(b) for b in proof["bounds"]),
                    "run_interval": tuple(proof["run_interval"]),
                    "axis_point": tuple(proof["axis_point"]),
                    "axis_direction": tuple(proof["axis_direction"]),
                }
            )
            entry = {"file": run["file"], "defining": call["defining"], "proof": proof}
            calls.append(
                _answer(
                    entry, "geometry", lambda v=value: geometry.cylindrical_channel_geometry(v).to_dict()
                )
            )
    return {"calls": calls}


def _write(name: str, payload: dict) -> None:
    text = json.dumps(_finite(payload), allow_nan=False, separators=(",", ":"))
    (OUT / name).write_bytes(gzip.compress((text + "\n").encode(), mtime=0))


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    _write("channels.json.gz", {"quiddity_revision": revision(QUIDDITY), **_channels()})
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
    nothing: dict[str, list[dict]] = {}
    for run in answered:
        if run["source"] == "test" and not _found(run):
            tests = tests_of[run["file"]]
            first = min(tests, key=lambda t: (TESTS.index(t) if t in TESTS else len(TESTS), t))
            nothing.setdefault(first, []).append(run)
            continue
        runs.append(run)
    unselected = 0
    for test in sorted(nothing):
        for i, run in enumerate(sorted(nothing[test], key=lambda r: r["file"])):
            if i < NOTHING_PER_TEST:
                runs.append(run)
            else:
                (OUT / run["file"]).unlink()
                unselected += 1
    runs.sort(key=lambda run: (run["source"] != "test", run["source"], run["file"]))
    _write(
        "calls.json.gz",
        {
            "quiddity_revision": revision(QUIDDITY),
            "skipped": _skipped["parts"],
            "unselected": unselected,
            "runs": runs,
        },
    )


if __name__ == "__main__":
    main()
