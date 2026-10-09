"""Record ``evidence.planar_outer_profile``'s answers (``quiddity._outer_profile_geometry``) for
every face of the parts quiddity's outer-profile tests build, the golden fixtures draftwright's
profile angles read and the corpus: the refusal reason, or the ordered supports (lines and arcs
with their geometry), the normal and origin, the inner-loop count, the schema version and the
faces of the body the profile belongs to.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_outer_profiles.py

Two phases, written to ``tests/fixtures/captured/outer_profiles/``:

1. ``tests/test_planar_outer_profiles.py`` runs in this process with
   ``_OuterProfileSource.__init__`` wrapped: every part a profile source is built over is kept
   as STEP (``parts/<sha>.step.gz``), at most ``MAX_PER_TEST`` distinct parts per test function
   (counted). A part STEP export refuses is counted in ``skipped``, not dropped silently. Faces
   handed to ``_read_profile`` on their own (a reversed copy of a face) are not parts and are not
   kept.
2. Once the tests are done (so no monkeypatch is live), each kept part, the golden fixtures and
   every corpus part, re-imported from STEP as the port reads it, is asked face by face through
   one strict ``FaceGraph`` and ``_OuterProfileSource`` (``faces.json.gz``), the source
   ``RecognitionEvidence.planar_outer_profile`` reads. Each run lists its bodies' face rosters
   once (``bodies``); a profile names its body by position in that list.
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
import time
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src"), str(QUIDDITY)]

import pytest  # noqa: E402
from build123d import export_step  # noqa: E402

import quiddity._outer_profile_geometry as geometry  # noqa: E402
from _provenance import revision  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402
from quiddity._adjacency import FaceGraph  # noqa: E402
from quiddity._outer_profile import ProfileArc, RefusedPlanarOuterProfile  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "captured" / "outer_profiles"
FIXTURES = ROOT / "tests" / "fixtures"
CORPUS = QUIDDITY / "tests" / "corpus"
TESTS = ("tests/test_planar_outer_profiles.py",)
# The golden parts whose chamfers and regular polygons define draftwright's profile angles.
GOLDEN = (
    "golden_oriented_chamfer.step",
    "golden_polygonal_boss.step",
    "golden_polygonal_stock.step",
    "golden_chamfers_fillets_and_flats.step",
)
MAX_PER_TEST = 4
WORKERS = max(1, (os.cpu_count() or 2) // 4)

_parts: dict[str, tuple[str, str]] = {}
_per_test: dict[str, int] = {}
_skipped = {"export": 0, "per_test": 0}


def _test() -> str:
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


_real_init = geometry._OuterProfileSource.__init__


def _init(self, graph) -> None:
    _keep(graph._part)
    _real_init(self, graph)


class _Recorder:
    def pytest_sessionstart(self, session) -> None:  # noqa: ARG002
        geometry._OuterProfileSource.__init__ = _init

    def pytest_sessionfinish(self, session, exitstatus) -> None:  # noqa: ARG002
        geometry._OuterProfileSource.__init__ = _real_init


# --- phase 2: every face of each re-imported part ---------------------------------------------


def _support(support) -> dict:
    out = {"kind": support.kind, "start": list(support.start), "end": list(support.end)}
    if isinstance(support, ProfileArc):
        out.update(center=list(support.center), radius=support.radius, sweep=support.sweep)
    return out


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def _faces(path: Path) -> dict:
    try:
        graph = FaceGraph(_load(path))
    except Exception as error:  # noqa: BLE001
        return {"refused": f"{type(error).__name__}: {error}"}
    source = geometry._OuterProfileSource(graph)
    bodies: list[list[int]] = []
    body_at: dict[frozenset, int] = {}
    faces = []
    for node in graph.nodes:
        value = source.read(node)
        if isinstance(value, RefusedPlanarOuterProfile):
            faces.append({"face": node.index, "refused": value.reason.value})
            continue
        profile, edges, body = value
        if body not in body_at:
            body_at[body] = len(bodies)
            bodies.append(sorted(n.index for n in body))
        assert len(edges) == len(profile.supports)
        faces.append(
            {
                "face": node.index,
                "origin": list(profile.origin),
                "normal": list(profile.normal),
                "inner_loop_count": profile.inner_loop_count,
                "schema_version": profile.schema_version,
                "body": body_at[body],
                "supports": [_support(s) for s in profile.supports],
            }
        )
    return {"faces": faces, "bodies": bodies}


def _job(job: dict) -> dict:
    path = job.pop("path")
    started = time.monotonic()
    result = {**job, **_faces(path)}
    print(job["file"], len(result.get("faces", [])), f"{time.monotonic() - started:.0f}s",
          file=sys.stderr, flush=True)
    return result


def _finite(value):
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
    with ProcessPoolExecutor(max_workers=WORKERS) as pool:
        runs = list(pool.map(_job, jobs))
    payload = _finite({"quiddity_revision": revision(QUIDDITY), "skipped": _skipped, "runs": runs})
    text = json.dumps(payload, allow_nan=False, separators=(",", ":"))
    (OUT / "faces.json.gz").write_bytes(gzip.compress((text + "\n").encode(), mtime=0))


if __name__ == "__main__":
    main()
