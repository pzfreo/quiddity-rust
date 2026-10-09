"""Record Python's section-recess family entry point, ``build_section_recess_document`` (the
aggregate's section-recess projection: ``quiddity.result._project_result`` after
reconciliation), on the parts its Python tests build, the golden passage fixtures and the
corpus.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_section_recesses.py

Written to ``tests/fixtures/captured/section_recesses/``:

1. The Python tests in ``TESTS`` run in this process with ``build_section_recess_document`` and
   ``recognise_section_recesses`` wrapped: every part handed to one is kept as STEP
   (``parts/<sha>.step.gz``) with the tests that built it. Of the parts one test case (a
   pytest node, each parametrised case its own) builds, the first ``PER_TEST`` are kept (the
   rest are counted in ``unselected``); a part STEP export refuses is counted in ``skipped``.
2. Each kept part, the golden passage fixtures and every corpus part, each re-imported from
   STEP as the port reads it, is asked again in a child process given ``PART_TIMEOUT`` seconds
   (``documents.json.gz``): ``document``, the document's ``to_dict()`` or the exception
   Python raises (``{"refused": "<Type>: <message>"}``) or ``{"timeout": seconds}``, and
   ``projected``, every record handed to ``_unique_section_recesses`` in order (native records,
   then prismatic pockets, the converging families and corner pockets), so a difference can be
   placed before or after the de-duplication.
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

import quiddity.result as result  # noqa: E402
from _provenance import revision  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "captured" / "section_recesses"
FIXTURES = ROOT / "tests" / "fixtures"
CORPUS = QUIDDITY / "tests" / "corpus"
TESTS = (
    "tests/test_section_recesses.py",
    "tests/test_section_recess_cutover.py",
    "tests/test_section_recess_geometry_golden.py",
    "tests/test_section_pattern_projection.py",
    "tests/test_grid_lattice_convention.py",
    "tests/test_corner_notch_guards.py",
    "tests/test_open_channel_section.py",
    "tests/test_support_apertures.py",
    "tests/test_boolean_support_compat.py",
    "tests/test_cylindrical_seats.py",
    "tests/test_cylindrical_channel_public.py",
    "tests/test_cylindrical_passage_public.py",
    "tests/test_cylindrical_pocket_proofs.py",
    "tests/test_mixed_section_pockets.py",
    "tests/test_passage_entry_treatments.py",
    "tests/test_plane_envelope_passages.py",
    "tests/test_passage_publication_ties.py",
    "tests/test_polygonal_split_mouths.py",
    "tests/test_section_passages.py",
)
GOLDEN = ("golden_hexagonal_passage.step", "golden_interrupted_and_cross_bores.step")
PART_TIMEOUT = 600
WORKERS = 4
PER_TEST = 3

# sha -> (STEP text, test nodes)
_parts: dict[str, tuple[str, set[str]]] = {}
# test node -> shas kept for it, in the order it built them
_per_test: dict[str, list[str]] = {}
_counts = {"skipped": 0, "unselected": 0}


def _step(shape) -> str:
    """The shape as STEP text, its export timestamp fixed so one part has one file name."""

    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        export_step(shape, tmp.name)
        text = Path(tmp.name).read_text()
    return re.sub(r"'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d'", "'2000-01-01T00:00:00'", text, count=1)


def _keep(part) -> None:
    node = os.environ.get("PYTEST_CURRENT_TEST", "").rsplit(" ", 1)[0]
    try:
        text = _step(part)
    except Exception:  # noqa: BLE001
        _counts["skipped"] += 1
        return
    sha = hashlib.sha256(text.encode()).hexdigest()[:16]
    kept = _per_test.setdefault(node, [])
    if sha in kept:
        return
    if sha not in _parts and len(kept) >= PER_TEST:
        _counts["unselected"] += 1
        return
    kept.append(sha)
    _parts.setdefault(sha, (text, set()))[1].add(node.split("::")[0])


_real_document = result.build_section_recess_document
_real_recognise = result.recognise_section_recesses


def _document(part):
    _keep(part)
    return _real_document(part)


def _recognise(part):
    _keep(part)
    return _real_recognise(part)


class _Recorder:
    """Installs the wrappers for the test run and removes them after it."""

    def pytest_sessionstart(self, session) -> None:  # noqa: ARG002
        wrappers = {id(_real_document): _document, id(_real_recognise): _recognise}
        self._modules = []
        for module in list(sys.modules.values()):
            if not getattr(module, "__name__", "").startswith(("quiddity", "test")):
                continue
            for name, value in list(vars(module).items()):
                if id(value) in wrappers:
                    self._modules.append((module, name, value))
                    setattr(module, name, wrappers[id(value)])

    def pytest_collection_finish(self, session) -> None:  # noqa: ARG002
        # Test modules import the entry points at collection; wrap their names too.
        self.pytest_sessionstart(session)

    def pytest_sessionfinish(self, session, exitstatus) -> None:  # noqa: ARG002
        for owner, name, value in self._modules:
            setattr(owner, name, value)


# --- phase 2: the document on re-imported parts ----------------------------------------------


def _raised(error: BaseException) -> str:
    return f"{type(error).__name__}: {error}"


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def _child(path: str, conn) -> None:
    try:
        part = _load(Path(path))
        projected: list[dict] = []
        real_unique = result._unique_section_recesses

        def unique(records):
            records = tuple(records)
            projected.extend(record.to_dict() for record in records)
            return real_unique(records)

        result._unique_section_recesses = unique
        try:
            document = _real_document(part).to_dict()
        except Exception as error:  # noqa: BLE001
            document = {"refused": _raised(error)}
        conn.send({"document": document, "projected": projected})
    except Exception as error:  # noqa: BLE001
        conn.send({"document": {"refused": _raised(error)}, "projected": []})


def _job(job: dict) -> dict:
    started = time.monotonic()
    context = multiprocessing.get_context("spawn")
    receive, send = context.Pipe(duplex=False)
    child = context.Process(target=_child, args=(str(job["path"]), send), daemon=True)
    child.start()
    send.close()
    run = {key: job[key] for key in ("source", "file")}
    if "tests" in job:
        run["tests"] = job["tests"]
    try:
        if receive.poll(PART_TIMEOUT):
            run.update(receive.recv())
        else:
            run["document"] = {"timeout": PART_TIMEOUT}
            run["projected"] = []
    except EOFError:
        run["document"] = {"refused": "child process died"}
        run["projected"] = []
    finally:
        child.kill()
        child.join()
        receive.close()
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


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    only = os.environ.get("CAPTURE_ONLY")  # a comma-separated list of corpus files, for checks
    cwd = os.getcwd()
    if not only:
        os.chdir(QUIDDITY)
        try:
            status = pytest.main(
                ["-q", "-p", "no:cacheprovider", "-p", "no:randomly", *TESTS],
                plugins=[_Recorder()],
            )
        finally:
            os.chdir(cwd)
        print("pytest exit", status, file=sys.stderr)

    jobs: list[dict] = []
    if not only:
        parts_dir = OUT / "parts"
        parts_dir.mkdir(exist_ok=True)
        for old in parts_dir.glob("*.step.gz"):
            old.unlink()
        for sha, (text, tests) in sorted(_parts.items()):
            path = parts_dir / f"{sha}.step.gz"
            path.write_bytes(gzip.compress(text.encode(), mtime=0))
            jobs.append(
                {"source": "test", "file": f"parts/{path.name}", "path": path, "tests": sorted(tests)}
            )
        for name in GOLDEN:
            jobs.append({"source": "fixture", "file": name, "path": FIXTURES / name})
    corpus = json.loads((FIXTURES / "corpus.json").read_text())["files"]
    names = sorted(entry["file"] for entry in corpus)
    if only:
        names = [name for name in names if name in only.split(",")]
    for name in names:
        jobs.append({"source": "corpus", "file": name, "path": CORPUS / name})
    with ThreadPoolExecutor(max_workers=WORKERS) as pool:
        runs = list(pool.map(_job, jobs))
    runs.sort(key=lambda run: (run["source"] != "test", run["source"], run["file"]))
    text = json.dumps(
        _finite(
            {
                "quiddity_revision": revision(QUIDDITY),
                "part_timeout": PART_TIMEOUT,
                "per_test": PER_TEST,
                "skipped": _counts["skipped"],
                "unselected": _counts["unselected"],
                "runs": runs,
            }
        ),
        allow_nan=False,
        separators=(",", ":"),
    )
    name = "documents.json.gz" if not only else "documents-only.json.gz"
    (OUT / name).write_bytes(gzip.compress((text + "\n").encode(), mtime=0))


if __name__ == "__main__":
    main()
