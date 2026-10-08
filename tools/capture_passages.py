"""Record the passage helpers' answers: ``quiddity._rings``, the legacy roster, and the
``_passage_compat`` / ``_same_legacy_passage_geometry`` calls ``recognise_section_passages`` makes.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_passages.py

The entry points' own calls in the Python tests are recorded by ``capture_plugin.py`` into
``tests/fixtures/captured/passages/calls.json`` (with their parts as gzipped STEP beside it).
This runs, on each of those parts, the section helpers' test parts
(``captured/sections/parts``), every golden fixture and every corpus part, each re-imported from
STEP as the port reads it:

- ``rings``: every ring with its walls, canonical section, axis, span and caps;
- the legacy roster's walls, per legacy passage in order;
- ``recognise_section_passages``: its records, or its refusal, and the defining walls of each;
- every ``principal_projection`` and ``_same_legacy_passage_geometry`` call made on the way,
  with its answer;

written to ``tests/fixtures/captured/passages/helpers.json.gz``. Faces are indices into
``part.faces()``.
"""

from __future__ import annotations

import dataclasses
import gzip
import json
import math
import os
import sys
import tempfile
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src"), str(QUIDDITY)]
sys.path.insert(0, str(Path(__file__).resolve().parent))

import quiddity.passages as passages  # noqa: E402
from _provenance import revision  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402
from quiddity._adjacency import FaceGraph  # noqa: E402
from quiddity._candidates import FamilyId  # noqa: E402
from quiddity._claims import ClaimLedger  # noqa: E402
from quiddity._rings import rings  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "tests" / "fixtures"
OUT = FIXTURES / "captured" / "passages"
CORPUS = QUIDDITY / "tests" / "corpus"


def _plain(value):
    if dataclasses.is_dataclass(value) and not isinstance(value, type):
        return {f.name: _plain(getattr(value, f.name)) for f in dataclasses.fields(value)}
    if isinstance(value, (list, tuple)):
        return [_plain(v) for v in value]
    if isinstance(value, float) and not math.isfinite(value):
        return str(value)
    return value


_calls: list[dict] = []
_real_projection = passages.principal_projection
_real_same = passages._same_legacy_passage_geometry


def _projection(origin, run, u, v, interval, boundary):
    answer = _real_projection(origin, run, u, v, interval, boundary)
    _calls.append(
        {
            "kind": "principal_projection",
            "given": _plain([origin, run, u, v, interval, boundary]),
            "answer": _plain(answer),
        }
    )
    return answer


def _same(left, right, *, exact_at=None):
    answer = _real_same(left, right, exact_at=exact_at)
    _calls.append(
        {
            "kind": "same_legacy_passage_geometry",
            "given": _plain([left, right, exact_at]),
            "answer": answer,
        }
    )
    return answer


def _run(part) -> dict:
    graph = FaceGraph(part)
    found = [
        {
            "nodes": sorted(node.index for node in ring.nodes),
            "section": _plain(ring.section),
            "axis": ring.axis,
            "low": ring.low,
            "high": ring.high,
            "caps": [sorted(node.index for node in end) for end in ring.cap_nodes],
        }
        for ring in rings(part, graph)
    ]
    roster = [sorted(node.index for node in nodes) for _, nodes in passages._legacy_roster(part, FaceGraph(part))]
    del _calls[:]
    passages.principal_projection = _projection
    passages._same_legacy_passage_geometry = _same
    try:
        result = {"result": _plain(passages.recognise_section_passages(part))}
        ledger = ClaimLedger(FaceGraph(part))
        passages._discover_section_passages(part, ledger.graph, ledger.writer.sink)
        result["defining"] = [
            sorted(node.index for node in ledger.defining_of(c))
            for c in ledger.candidate_set(FamilyId.PASSAGES).candidates
        ]
    except ValueError as error:
        result = {"refused": str(error)}
    finally:
        passages.principal_projection = _real_projection
        passages._same_legacy_passage_geometry = _real_same
    # Each call is made twice (records, then evidence): keep distinct ones.
    calls = []
    for call in _calls:
        if call not in calls:
            calls.append(call)
    return {"rings": found, "roster": roster, **result, "calls": calls}


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def main() -> None:
    sources = [("test", p, f"passages/{p.name}") for p in sorted(OUT.glob("*.step.gz"))]
    sections = FIXTURES / "captured" / "sections" / "parts"
    sources += [("test", p, f"sections/parts/{p.name}") for p in sorted(sections.glob("*.step.gz"))]
    sources += [("fixture", p, p.name) for p in sorted(FIXTURES.glob("golden_*.step"))]
    corpus = json.loads((FIXTURES / "corpus.json").read_text())["files"]
    sources += [("corpus", CORPUS / name, name) for name in sorted(e["file"] for e in corpus)]
    runs = []
    for source, path, name in sources:
        runs.append({"source": source, "file": name, **_run(_load(path))})
        print(name, len(runs[-1]["rings"]), len(runs[-1].get("result", [])), file=sys.stderr)
    payload = {"quiddity_revision": revision(QUIDDITY), "runs": runs}
    text = json.dumps(payload, allow_nan=False)
    (OUT / "helpers.json.gz").write_bytes(gzip.compress((text + "\n").encode(), mtime=0))


if __name__ == "__main__":
    main()
