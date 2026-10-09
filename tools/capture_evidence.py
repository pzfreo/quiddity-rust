"""Record Python's recognition evidence view over the shared STEP corpus.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_evidence.py \
        [--out PATH] [file ...]

Runs Python's ``quiddity.evidence.build_recognition_evidence`` (its default inventory,
``quiddity.result._take_inventory`` with the local-degradation retry, projected by
``_project_recognition_evidence``) on every corpus part and writes
``tests/fixtures/captured/recognition_evidence/capture.json.gz``: the quiddity revision and, per
part, its sha256 and either the inventory's refusal or

- ``features``: the view's features in order (accepted physical occurrences family by family in
  ``PHYSICAL_DEFINITIONS`` order, the section recesses and their refusals, then the hole
  patterns), each as ``family``, ``index`` (its position among that family's features here),
  ``defining``, ``constituent`` and ``hosts`` (indices into ``part.faces()``, sorted),
  ``groups`` (each instance group's faces, sorted, in order) and ``members`` (positions in
  ``features``);
- ``candidates``: the view's candidate projection (every rejected candidate and, transitively,
  the candidates they are related to) in the inventory's canonical order, each as ``family``,
  ``index`` (its position among that family's physical candidates), ``outcome``, ``reason``,
  ``defining``, ``constituent`` (sorted) and ``related`` (positions in ``candidates``);
- ``rejected``: the positions in ``candidates`` of ``rejected_candidates``, in order.

A face node's index is its position in ``part.faces()`` (checked), the numbering every other
capture uses. ``tests/recognition_evidence.rs`` compares the port's view with these. Nothing in
the Python repo is edited. Naming files captures only those, and ``--out`` writes elsewhere (to
run shards in parallel and join their ``parts``, in corpus order).
"""

from __future__ import annotations

import gzip
import json
import os
import sys
import tempfile
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src")]
sys.path.insert(0, str(Path(__file__).resolve().parent))

from _provenance import revision, sha256  # noqa: E402

import quiddity.evidence as evidence_module  # noqa: E402
import quiddity.result as result  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402

CORPUS = QUIDDITY / "tests" / "corpus"
OUT = (
    Path(__file__).resolve().parent.parent
    / "tests" / "fixtures" / "captured" / "recognition_evidence" / "capture.json.gz"
)


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


def _capture(part) -> dict:
    product = result._take_inventory(part)
    view = evidence_module._project_recognition_evidence(product)
    faces = list(part.faces())
    graph = product.context.graph
    for node in graph.nodes:
        # The port numbers faces in OpenCascade's order; a node's index is that position.
        if not faces[node.index].wrapped.IsSame(graph.face(node).wrapped):
            raise AssertionError("face node index is not its position in part.faces()")
    index_of = {}
    for node in graph.nodes:
        index_of[id(view._RecognitionEvidence__node_refs[node])] = node.index

    def indices(refs) -> list[int]:
        return sorted(index_of[id(ref)] for ref in refs)

    features = view.features
    position = {id(ref): at for at, ref in enumerate(features)}
    counts: dict[str, int] = {}
    out_features = []
    for ref in features:
        family = view.family(ref)
        at = counts.get(family, 0)
        counts[family] = at + 1
        out_features.append(
            {
                "family": family,
                "index": at,
                "defining": indices(view.defining_faces(ref)),
                "constituent": indices(view.constituent_faces(ref)),
                "hosts": indices(view.host_faces(ref)),
                "groups": [indices(group) for group in view.instance_faces(ref)],
                "members": [position[id(member)] for member in view.members(ref)],
            }
        )

    projections = view._RecognitionEvidence__candidate_projections
    candidate_position = {id(item.reference): at for at, item in enumerate(projections)}
    dispositions = product.reconciliation.dispositions
    family_position = {}
    for family in result.PHYSICAL_FAMILIES:
        for at, candidate in enumerate(product.physical.candidate_set(family).candidates):
            family_position[id(candidate)] = at
    # The dispositions the view projects, selected as `_project_recognition_evidence` selects
    # them (every rejected one and, transitively, those they are related to), in their order.
    by_id = {id(item.candidate): item for item in dispositions}
    chosen = {id(item.candidate) for item in dispositions if item.outcome.value == "rejected"}
    pending = list(chosen)
    while pending:
        for related in by_id[pending.pop()].related:
            if id(related) not in chosen:
                chosen.add(id(related))
                pending.append(id(related))
    projected = [item for item in dispositions if id(item.candidate) in chosen]
    if len(projected) != len(projections) or any(
        item.family != disposition.candidate.family.value
        for item, disposition in zip(projections, projected, strict=True)
    ):
        raise AssertionError("candidate projections do not follow the dispositions' order")
    by_reference = {
        id(item.reference): disposition
        for item, disposition in zip(projections, projected, strict=True)
    }
    candidates = []
    for item in projections:
        disposition = by_reference[id(item.reference)]
        candidates.append(
            {
                "family": item.family,
                "index": family_position[id(disposition.candidate)],
                "outcome": item.outcome.value,
                "reason": item.reason.value,
                "defining": indices(view.candidate_defining_faces(item.reference)),
                "constituent": indices(view.candidate_constituent_faces(item.reference)),
                "related": [
                    candidate_position[id(other)]
                    for other in view.related_candidates(item.reference)
                ],
            }
        )
    rejected = [candidate_position[id(ref)] for ref in view.rejected_candidates]
    return {"features": out_features, "candidates": candidates, "rejected": rejected}


def main() -> None:
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
        try:
            entry.update(_capture(part))
        except Exception as error:  # noqa: BLE001 -- the refusal is the record
            entry["error"] = f"{type(error).__name__}: {error}"
        parts.append(entry)
        print(name, len(entry.get("features", ())), len(entry.get("rejected", ())),
              entry.get("error", ""), file=sys.stderr, flush=True)
    out.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(
        {"quiddity_revision": revision(QUIDDITY), "parts": parts}, separators=(",", ":")
    )
    # A fixed mtime keeps the gzip bytes a function of the content.
    out.write_bytes(gzip.compress(text.encode(), mtime=0))


if __name__ == "__main__":
    main()
