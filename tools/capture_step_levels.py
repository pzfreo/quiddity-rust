"""Record Python's step levels and risers over the shared STEP corpus.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_step_levels.py \
        [--out PATH] [file ...]

Writes ``tests/fixtures/captured/step_levels/capture.json.gz``: the quiddity revision and, per
corpus part, its sha256 and

- ``records``: ``levels.step_level_records(part)``, the public step levels at the default end
  margin (``bounded_end_margin`` of each solid's height);
- ``proposals``: the step levels ``levels._discover_step_levels`` proposes before it proves
  them, in its order, each with its defining faces (indices into ``part.faces()``, sorted) and
  whether ``FaceGraph.common_valid_solid`` proves them strictly (``strict``) and under local
  degradation (``degraded``); and the same for the aggregate's riser proposals
  (``riser_proposals``: ``_riser_proposals_one`` with the aggregate's options and no body
  levels, the body key set as ``_discover_risers`` sets it);
- ``aggregate``: Python's default inventory (``quiddity.result._take_inventory``, the path
  ``build_recognition_result`` takes, local-degradation retry included): whether it took the
  retry, its physical ``step_levels`` and ``risers`` candidates, each record with its defining
  faces, the indices of the risers reconciliation accepts (``accepted_risers``), and the
  published ``RecognitionResult.step_levels`` and ``risers``; or the inventory's refusal.

``tests/step_levels.rs`` compares the port's step levels and risers with these. Nothing in the
Python repo is edited. Naming files captures only those, and ``--out`` writes elsewhere (to run
shards in parallel and join their ``parts``, in corpus order).
"""

from __future__ import annotations

import gzip
import json
import os
import sys
import tempfile
from dataclasses import replace
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src")]
sys.path.insert(0, str(Path(__file__).resolve().parent))

from _provenance import revision, sha256  # noqa: E402
from capture_plugin import _plain  # noqa: E402

import quiddity.levels as levels  # noqa: E402
import quiddity.result as result  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402
from quiddity._adjacency import FaceGraph  # noqa: E402
from quiddity._body_identity import unambiguous_body_keys  # noqa: E402
from quiddity._candidates import FamilyId  # noqa: E402
from quiddity._solid_properties import solid_properties  # noqa: E402

CORPUS = QUIDDITY / "tests" / "corpus"
OUT = (
    Path(__file__).resolve().parent.parent
    / "tests" / "fixtures" / "captured" / "step_levels" / "capture.json.gz"
)

_retried: list[bool] = []
_original_once = result._take_inventory_once


def _once(part, **kwargs):
    if kwargs.get("local_degradation"):
        _retried.append(True)
    return _original_once(part, **kwargs)


def _files():
    for path in sorted(CORPUS.rglob("*")):
        if path.name.lower().endswith((".step", ".stp", ".step.gz", ".stp.gz")):
            yield path


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def _proposals(part) -> tuple[list[dict], list[dict]]:
    """The aggregate's step-level and riser proposals, as ``_discover_step_levels`` and
    ``_discover_risers`` form them, each with both proofs."""

    strict, degraded = FaceGraph(part), FaceGraph(part, local_degradation=True)
    faces = list(part.faces())

    def entry(record, found) -> dict:
        nodes = tuple(strict.require_node(face) for face in found)
        for node in nodes:
            if not faces[node.index].wrapped.IsSame(strict.face(node).wrapped):
                raise AssertionError("face node index is not its position in part.faces()")
        degraded_nodes = tuple(degraded.require_node(face) for face in found)
        return {
            "record": _plain(record),
            "defining": sorted(node.index for node in nodes),
            "strict": strict.common_valid_solid(nodes) is not None,
            "degraded": degraded.common_valid_solid(degraded_nodes) is not None,
        }

    scopes = list(part.solids()) or [part]
    properties = solid_properties(None)
    body_keys = unambiguous_body_keys(scopes, require_valid_solid=True, properties=properties)
    found_levels, found_risers = [], []
    for scope, body_key in zip(scopes, body_keys, strict=True):
        bb = properties.bounding_box(scope)
        margin = levels.bounded_end_margin(bb.max.Z - bb.min.Z)
        found_levels.extend(
            (replace(p.record, body_key=body_key), p.faces)
            for p in levels._face_level_proposals_one(
                scope, tol=levels._TOL, min_area_frac=levels._STEP_MIN_AREA_FRAC,
                properties=properties,
            )
            if bb.min.Z + margin < p.record.z < bb.max.Z - margin
        )
        found_risers.extend(
            (replace(p.record, body_key=body_key), p.faces)
            for p in levels._riser_proposals_one(
                scope, min_area_frac=0.15, tol=levels._TOL, body_levels=(),
                properties=properties,
            )
        )
    found_levels.sort(key=lambda item: item[0])
    return (
        [entry(record, found) for record, found in found_levels],
        [entry(record, found) for record, found in found_risers],
    )


def _aggregate(part) -> dict:
    product = result._take_inventory(part)
    evidence = product.evidence

    def candidates(family) -> list:
        return product.physical.candidate_set(family).candidates

    def entry(candidate) -> dict:
        return {
            "record": _plain(candidate.record),
            "defining": sorted(node.index for node in evidence.defining_of(candidate)),
        }

    risers = candidates(FamilyId.RISERS)
    accepted = {
        id(c)
        for c in product.reconciliation.accepted_set(
            product.physical.candidate_set(FamilyId.RISERS)
        ).candidates
    }
    return {
        "step_levels": [entry(c) for c in candidates(FamilyId.STEP_LEVELS)],
        "risers": [entry(c) for c in risers],
        "accepted_risers": [at for at, c in enumerate(risers) if id(c) in accepted],
        "result_step_levels": _plain(list(product.result.step_levels)),
        "result_risers": _plain(list(product.result.risers)),
    }


def main() -> None:
    result._take_inventory_once = _once
    args = sys.argv[1:]
    out = OUT
    if args[:1] == ["--out"]:
        out, args = Path(args[1]), args[2:]
    only = set(args)
    parts = []
    for path in _files():
        name = str(path.relative_to(CORPUS))
        if only and name not in only:
            continue
        part = _load(path)
        entry = {"file": name, "sha256": sha256(path)}
        entry["records"] = _plain(levels.step_level_records(part))
        entry["proposals"], entry["riser_proposals"] = _proposals(part)
        _retried.clear()
        try:
            entry["aggregate"] = _aggregate(part)
        except Exception as error:  # noqa: BLE001 -- the refusal is the record
            entry["aggregate"] = {"error": f"{type(error).__name__}: {error}"}
        entry["aggregate"]["retried"] = bool(_retried)
        parts.append(entry)
        print(name, len(entry["records"]), len(entry["proposals"]),
              len(entry["aggregate"].get("risers", ())), entry["aggregate"].get("error", ""),
              file=sys.stderr, flush=True)
    out.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(
        {"quiddity_revision": revision(QUIDDITY), "parts": parts}, separators=(",", ":")
    )
    # A fixed mtime keeps the gzip bytes a function of the content.
    out.write_bytes(gzip.compress(text.encode(), mtime=0))


if __name__ == "__main__":
    main()
