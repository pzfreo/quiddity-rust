"""Record Python's cross-family reconciliation decisions over the shared STEP corpus.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_reconcile.py \
        [--out PATH] [file ...]
    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_reconcile.py --scenarios

Runs Python's default inventory (``quiddity.result._take_inventory``, the path
``build_recognition_result`` takes, local-degradation retry included) on every corpus part and
writes ``tests/fixtures/captured/reconcile/capture.json.gz``: the quiddity revision and, per
part, its sha256, whether the inventory took the retry, and either the inventory's refusal or

- ``candidates``: per family a rule reads, every physical candidate's defining faces (indices
  into ``part.faces()``, sorted), in candidate order;
- ``decisions``: every disposition ``_reconcile_existing`` made (the default acceptances
  ``ReconciliationResult.complete`` fills in for the other candidates are left out), as its
  candidate (family, index in that family's candidates, defining faces), outcome, reason code
  and related candidates (winners, or peers for a compatible pair), in Python's canonical order.

``tests/reconcile.rs`` compares the port's decisions with these, keyed by family and defining
faces. Nothing in the Python repo is edited. Naming files captures only those, and ``--out``
writes elsewhere (to run shards in parallel and join their ``parts``, in corpus order).

``--scenarios`` writes ``scenarios.json`` there instead: the branches the corpus never reaches
(a pocket inside an edge-open circular pocket, a blind slot's constituent test, a
non-rectangular ring, pooled passage groupings, Double-D bores, thin-wall precedence after a
turned step, a fillet a circular step has taken, a candidate two rules decide), each as small
synthetic inputs (defining faces per family, pocket constituents, prismatic side counts,
passage groupings) and the decisions Python's own ``_reconcile_existing`` and
``ReconciliationResult.complete`` make on them, or the refusal. The inputs reach the rules
through stand-ins for the candidate sets and the evidence index that answer only what the rules
ask; the rules and the completion checks are Python's.
"""

from __future__ import annotations

import gzip
import json
import os
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src")]
sys.path.insert(0, str(Path(__file__).resolve().parent))

from _provenance import revision, sha256  # noqa: E402

import quiddity.result as result  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402
from quiddity._candidates import FamilyId  # noqa: E402
from quiddity._dispositions import ReasonCode  # noqa: E402
from quiddity._passage_compat import PassageCompatibilityView  # noqa: E402
from quiddity.prismatic_pockets import PrismaticPocket  # noqa: E402

CORPUS = QUIDDITY / "tests" / "corpus"
OUT = (
    Path(__file__).resolve().parent.parent
    / "tests" / "fixtures" / "captured" / "reconcile" / "capture.json.gz"
)

#: Every family a rule in ``_reconcile_existing`` reads, as subject or winner.
FAMILIES = (
    FamilyId.SLOTS,
    FamilyId.POCKETS,
    FamilyId.PRISMATIC_POCKETS,
    FamilyId.PASSAGES,
    FamilyId.RECTANGULAR_BLIND_SLOTS,
    FamilyId.EDGE_OPEN_CIRCULAR_POCKETS,
    FamilyId.CHAMFERS,
    FamilyId.ANGLED_STEPS,
    FamilyId.FILLETS,
    FamilyId.CIRCULAR_BLIND_STEPS,
    FamilyId.BLENDS,
    FamilyId.HOLES,
    FamilyId.DOUBLE_D_BORES,
    FamilyId.BOSSES,
    FamilyId.TURNED_STEPS,
    FamilyId.GROOVES,
    FamilyId.ORIENTED_SLOTS,
    FamilyId.THIN_WALL_BODIES,
    FamilyId.PLATES,
    FamilyId.RISERS,
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


def _capture(part) -> dict:
    product = result._take_inventory(part)
    evidence = product.evidence
    faces = list(part.faces())

    def defining(candidate) -> list[int]:
        out = []
        for node in evidence.defining_of(candidate):
            # The port numbers faces in OpenCascade's order; a node's index is that position.
            if not faces[node.index].wrapped.IsSame(product.context.graph.face(node).wrapped):
                raise AssertionError("face node index is not its position in part.faces()")
            out.append(node.index)
        return sorted(out)

    position = {}
    candidates = {}
    for family in FAMILIES:
        found = product.physical.candidate_set(family).candidates
        candidates[family.value] = [defining(c) for c in found]
        for at, candidate in enumerate(found):
            position[id(candidate)] = at

    def named(candidate) -> list:
        return [candidate.family.value, position[id(candidate)], defining(candidate)]

    decisions = [
        {
            "candidate": named(item.candidate),
            "outcome": item.outcome.value,
            "reason": item.reason.value,
            "related": [named(other) for other in item.related],
        }
        for item in product.reconciliation.dispositions
        if item.reason is not ReasonCode.DEFAULT_ACCEPTED
    ]
    return {"candidates": candidates, "decisions": decisions}


SQUARE = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
TRIANGLE = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]

#: Synthetic inputs for the branches the corpus does not reach: each family's defining faces
#: (face numbers are arbitrary), a pocket's constituent faces, a prismatic pocket's side count,
#: and a passage's slot grouping ``[axis, section, sides]`` or ``None``.
SCENARIOS = [
    {
        "name": "a pocket inside an edge-open circular pocket and a passage",
        "families": {"pockets": [[1, 2]], "edge_open_circular_pockets": [[1, 2, 5]],
                     "passages": [[1, 2, 6, 7]]},
        "pocket_constituent": [[1, 2, 3]],
        "passage_groupings": [None],
    },
    {
        "name": "a blind slot wins a pocket only by the pocket's constituent faces",
        "families": {"pockets": [[1, 2], [10, 11]],
                     "rectangular_blind_slots": [[1, 2, 3], [10, 11]]},
        "pocket_constituent": [[1, 2, 3], [10, 11, 12]],
    },
    {
        "name": "a non-rectangular ring wins a pocket; a four-sided ring yields to one",
        "families": {"pockets": [[1, 2], [20, 21]],
                     "prismatic_pockets": [[1, 2, 3, 4, 5, 6], [20, 21, 22, 23]]},
        "pocket_constituent": [[1, 2], [20, 21, 30]],
        "prismatic_sides": [6, 4],
    },
    {
        "name": "a five-sided ring with no rectangular reading wins the pocket inside it",
        "families": {"pockets": [[1, 2]], "prismatic_pockets": [[1, 2, 3, 4, 5]]},
        "pocket_constituent": [[1, 2]],
        "prismatic_sides": [5],
    },
    {
        "name": "slots yield to accepted pockets, rings and pooled passages, not rejected ones",
        "families": {
            "slots": [[1, 2], [30, 31], [40, 45], [50, 51], [60, 61], [70, 71]],
            "pockets": [[1, 2, 3, 4], [60, 61]],
            "edge_open_circular_pockets": [[60, 61, 62]],
            "prismatic_pockets": [[30, 31, 32, 33, 34, 35], [70, 71, 72, 73]],
            "passages": [[40, 41, 42], [43, 44, 45], [50, 51, 52, 53], [70, 71, 72, 73, 74]],
        },
        "pocket_constituent": [[1, 2, 3, 4], [60, 61]],
        "prismatic_sides": [6, 4],
        "passage_groupings": [["z", TRIANGLE, 3], ["z", TRIANGLE, 3], ["x", SQUARE, 4], None],
    },
    {
        "name": "passages pool only within one exact axis and section",
        "families": {"slots": [[40, 45]], "passages": [[40, 41, 42], [43, 44, 45]]},
        "passage_groupings": [["z", TRIANGLE, 3], ["y", TRIANGLE, 3]],
    },
    {
        "name": "a passage without a side count has no grouping and pools with nothing",
        "families": {"slots": [[1, 4]], "passages": [[1, 2], [3, 4]]},
        "passage_groupings": [["z", TRIANGLE, None], ["z", TRIANGLE, None]],
    },
    {
        "name": "a hole yields only to a Double-D bore strictly containing it",
        "families": {"holes": [[1], [5, 6], []], "double_d_bores": [[1, 2, 3], [5, 6]]},
    },
    {
        "name": "thin walls take plates, bosses and risers no earlier rule decided",
        "families": {"thin_wall_bodies": [[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], [3, 4]],
                     "plates": [[1, 2], [11]], "bosses": [[3], [4]], "turned_steps": [[3]],
                     "risers": [[5], [12], []]},
    },
    {
        "name": "a fillet a circular blind step has taken covers no blend",
        "families": {"fillets": [[1], [2], [3]], "circular_blind_steps": [[1, 9]],
                     "blends": [[1, 2], [2, 3], [4]]},
    },
    {
        "name": "chamfers on a slant; steps and grooves related both ways",
        "families": {"chamfers": [[1], [2], [1, 7]], "angled_steps": [[1], [7]],
                     "turned_steps": [[10, 11], [12], [11, 14]], "grooves": [[11], [13], []]},
    },
    {
        "name": "a passage both a slot and an oriented slot decide refuses",
        "families": {"passages": [[1, 2, 3, 4]], "slots": [[1, 2]],
                     "oriented_slots": [[1, 2, 3, 4]]},
        "passage_groupings": [["z", SQUARE, 4]],
    },
    {
        "name": "empty evidence proves no containment",
        "families": {"pockets": [[]], "slots": [[]], "passages": [[1]],
                     "prismatic_pockets": [[1, 2, 3, 4]], "fillets": [[]],
                     "circular_blind_steps": [[1]], "blends": [[]], "bosses": [[]],
                     "turned_steps": [[]], "oriented_slots": [[]], "chamfers": [[]],
                     "angled_steps": [[]]},
        "pocket_constituent": [[]],
        "prismatic_sides": [4],
        "passage_groupings": [["z", SQUARE, 4]],
    },
]


class _Candidate:
    """A candidate stand-in: family, record and issuer, hashed by identity as `Candidate` is."""

    def __init__(self, family, record, issuer):
        self.family, self.record, self._issuer = family, record, issuer


def _scenario(spec: dict) -> dict:
    """Python's decisions on one synthetic scenario, through stand-ins for what the rules read."""

    issuer = object()
    families = spec["families"]
    defining, constituent, compatibility, sets = {}, {}, {}, {}
    for family in result.PHYSICAL_FAMILIES:
        found = []
        for at, faces in enumerate(families.get(family.value, [])):
            record = object()
            if family is FamilyId.PRISMATIC_POCKETS:
                record = object.__new__(PrismaticPocket)
                object.__setattr__(record, "sides", spec["prismatic_sides"][at])
            candidate = _Candidate(family, record, issuer)
            defining[id(candidate)] = frozenset(faces)
            constituent[id(candidate)] = frozenset(
                spec["pocket_constituent"][at] if family is FamilyId.POCKETS else faces
            )
            if family is FamilyId.PASSAGES:
                grouping = spec["passage_groupings"][at]
                compatibility[id(candidate)] = (
                    PassageCompatibilityView(None, None, None, None, None, None, False)
                    if grouping is None
                    else PassageCompatibilityView(
                        grouping[0], tuple(map(tuple, grouping[1])), grouping[2],
                        None, None, None, False,
                    )
                )
            found.append(candidate)
        sets[family] = SimpleNamespace(family=family, candidates=tuple(found), _issuer=issuer)
    evidence = SimpleNamespace(
        defining_of=lambda c: defining[id(c)],
        constituent_of=lambda c: constituent[id(c)],
        passage_compatibility=lambda c: compatibility[id(c)],
        validate_candidate_set=lambda group: None,
    )
    physical = SimpleNamespace(candidate_set=lambda family: sets[family])
    position = {id(c): at for group in sets.values() for at, c in enumerate(group.candidates)}

    def named(candidate) -> list:
        return [candidate.family.value, position[id(candidate)], sorted(defining[id(candidate)])]

    out = dict(spec)
    try:
        reconciliation = result._reconcile_existing(physical, evidence)
    except ValueError as error:
        out["error"] = str(error)
        return out
    out["decisions"] = [
        {
            "candidate": named(item.candidate),
            "outcome": item.outcome.value,
            "reason": item.reason.value,
            "related": [named(other) for other in item.related],
        }
        for item in reconciliation.dispositions
        if item.reason is not ReasonCode.DEFAULT_ACCEPTED
    ]
    return out


def main() -> None:
    if sys.argv[1:] == ["--scenarios"]:
        out = OUT.parent / "scenarios.json"
        out.parent.mkdir(parents=True, exist_ok=True)
        scenarios = [_scenario(spec) for spec in SCENARIOS]
        text = json.dumps({"quiddity_revision": revision(QUIDDITY), "scenarios": scenarios})
        out.write_text(text + "\n")
        return
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
        _retried.clear()
        entry = {"file": name, "sha256": sha256(path)}
        try:
            entry.update(_capture(part))
        except Exception as error:  # noqa: BLE001 -- the refusal is the record
            entry["error"] = f"{type(error).__name__}: {error}"
        entry["retried"] = bool(_retried)
        parts.append(entry)
        print(name, entry["retried"], len(entry.get("decisions", ())), entry.get("error", ""),
              file=sys.stderr, flush=True)
    out.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(
        {"quiddity_revision": revision(QUIDDITY), "parts": parts}, separators=(",", ":")
    )
    # A fixed mtime keeps the gzip bytes a function of the content.
    out.write_bytes(gzip.compress(text.encode(), mtime=0))


if __name__ == "__main__":
    main()
