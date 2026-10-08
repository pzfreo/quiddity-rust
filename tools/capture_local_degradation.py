"""Record which corpus parts take Python's local-degradation retry, and what it skips there.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_local_degradation.py

Python's default inventory (``quiddity.result._take_inventory``) runs strictly first; if that
raises ``Hole cylindrical evidence does not prove one valid solid`` and the part's solids are
not all valid under ``BRepCheck_Analyzer``, it runs again with ``local_degradation=True``, where
a family skips a record that does not prove one valid solid instead of refusing it. This runs
the default inventory over every corpus part with ``_take_inventory_once``,
``FaceGraph.common_valid_solid`` and reads of ``FaceGraph.local_degradation`` wrapped (nothing
in the Python repo is edited), and writes ``tests/fixtures/captured/local_degradation/
capture.json``: the quiddity revision and, per part, its sha256, how the strict pass ended,
whether the retry ran and how it ended, and on a retried part:

- ``solids``: per solid, its ``BRepCheck`` validity, whether the degraded run admitted it as a
  closed owner (``_build_solid_ownership``), whether as a locally degraded one, and its
  bad-face region (``bad_faces``, and ``unsafe_faces``: those with their edge neighbours);
- ``reads``: every family site that read ``local_degradation`` as set, with how often;
- ``skips``: every proof that answered no solid where the family skips the record only because
  ``local_degradation`` is set (strict mode refuses there), with the family module, function and
  line, the faces the proof was asked about (indices into ``part.faces()``, each with its
  surface type and box, so a reader with another face order can find it) and why it failed;
- ``other_unproved``: every other proof that answered no solid in the degraded run (the family
  decides those the same way in strict mode), so nothing the run decided goes unrecorded.

A proof belongs to a skip when the read that decides it is next to it in the same frame: the
read just before a filter or conjunction of proofs (``if graph.local_degradation: found = [...
if graph.common_valid_solid(...) is not None]``), or just after one proof (``if
graph.common_valid_solid(nodes) is None: if graph.local_degradation: continue``).
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

import quiddity.result as result  # noqa: E402
from OCP.BRepCheck import BRepCheck_Analyzer  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402
from quiddity._adjacency import FaceGraph  # noqa: E402

CORPUS = QUIDDITY / "tests" / "corpus"
OUT = (
    Path(__file__).resolve().parent.parent
    / "tests" / "fixtures" / "captured" / "local_degradation" / "capture.json"
)
RETRY_MESSAGE = "Hole cylindrical evidence does not prove one valid solid"

#: Modules a proof or read passes through on its way from the family that asked it.
PLUMBING = {"quiddity._adjacency", "quiddity._geometry_evidence", "quiddity._claims"}

#: How many source lines a deciding read may sit before the proofs it gates, or after the one
#: proof whose failure it turns into a skip.
NEAR_BEFORE = 10
NEAR_AFTER = 3

_current: dict = {}


def _box(face) -> dict:
    bb = face.bounding_box()
    return {
        "type": face.geom_type.name,
        "min": [round(bb.min.X, 3), round(bb.min.Y, 3), round(bb.min.Z, 3)],
        "max": [round(bb.max.X, 3), round(bb.max.Y, 3), round(bb.max.Z, 3)],
    }


def _family_frame(frame):
    """The first frame, from *frame* outwards, in a family module rather than plumbing; a
    generator expression stands for the function consuming it (``tuple(... for ...)``)."""

    while frame is not None:
        module = frame.f_globals.get("__name__", "")
        if (
            module.startswith("quiddity.")
            and module not in PLUMBING
            and frame.f_code.co_name != "<genexpr>"
        ):
            return frame
        frame = frame.f_back
    return None


def _state(frame) -> dict:
    """Per family frame: the line of its last read, and its failed proofs awaiting one. The
    frame is held so its id is not reused within the part."""

    return _current["frames"].setdefault(id(frame), {"frame": frame, "read": None, "pending": []})


def _site(frame) -> dict:
    return {
        "module": frame.f_globals["__name__"].removeprefix("quiddity."),
        "function": frame.f_code.co_name,
        "line": frame.f_lineno,
    }


def _reason(graph, nodes) -> str:
    """Why ``common_valid_solid`` answered no solid, in its own order of tests."""

    memberships = [graph._face_solids[node.index] for node in nodes]
    if not memberships:
        return "no faces"
    if (
        memberships[0]
        and memberships[0][0] in graph._degraded_solids
        and any(node.index in graph._unsafe_faces for node in nodes)
    ):
        return "touches the degraded solid's bad-face region"
    if any(len(m) != 1 for m in memberships):
        return "a face is not owned by exactly one solid"
    if any(m != memberships[0] for m in memberships[1:]):
        return "faces of different solids"
    return "solid neither valid nor admitted as locally degraded"


def _read(self):
    value = self.__dict__["_local_degradation"]
    reader = sys._getframe(1)
    # The graph's own read (``_build_solid_ownership``) decides admission, not a record.
    if (
        value
        and _current.get("running_degraded")
        and reader.f_globals.get("__name__") != "quiddity._adjacency"
    ):
        frame = _family_frame(reader)
        if frame is not None:
            site = _site(frame)
            key = f"{site['module']}.{site['function']}:{site['line']}"
            _current["reads"][key] = _current["reads"].get(key, 0) + 1
            state = _state(frame)
            for entry in state["pending"]:
                gap = site["line"] - entry["line"]
                bucket = "skips" if 0 <= gap <= NEAR_AFTER else "other_unproved"
                _current[bucket].append(entry)
            state["pending"] = []
            state["read"] = site["line"]
    return value


def _write(self, value):
    self.__dict__["_local_degradation"] = value


_real_proof = FaceGraph.common_valid_solid


def _proof(self, nodes):
    nodes = tuple(nodes)
    answer = _real_proof(self, nodes)
    if answer is None and self.__dict__["_local_degradation"] and _current.get("running_degraded"):
        frame = _family_frame(sys._getframe(1))
        faces = sorted({node.index for node in nodes})
        entry = {
            **_site(frame),
            "faces": faces,
            "boxes": [_box(self.face(self.nodes[i])) for i in faces],
            "reason": _reason(self, nodes),
        }
        state = _state(frame)
        if state["read"] is not None and 0 <= entry["line"] - state["read"] <= NEAR_BEFORE:
            _current["skips"].append(entry)
        else:
            state["pending"].append(entry)
    return answer


_real_once = result._take_inventory_once


def _once(part, **kwargs):
    degraded = kwargs["local_degradation"]
    _current["running_degraded"] = degraded
    key = "degraded" if degraded else "strict"
    try:
        product = _real_once(part, **kwargs)
    except Exception as error:  # noqa: BLE001 -- the outcome is the record
        _current[key] = {"error": f"{type(error).__name__}: {error}"}
        raise
    _current[key] = "ok"
    if degraded:
        _current["graph"] = product.context.graph
    return product


def _solids(part, graph) -> list[dict]:
    graph._build_solid_ownership()
    out = []
    for at, solid in enumerate(part.solids()):
        analyzer = BRepCheck_Analyzer(solid.wrapped)
        by_index = {graph._index[face]: face for face in solid.faces() if face in graph._index}
        bad = sorted(i for i in graph._invalid_faces if i in by_index)
        out.append(
            {
                "valid": bool(analyzer.IsValid()),
                "admitted": at in graph._closed_solids,
                "degraded": at in graph._degraded_solids,
                "bad_faces": bad,
                # What BRepCheck reports on each bad face: the evidence a verdict reads.
                "bad_face_status": {
                    str(i): [s.name for s in analyzer.Result(by_index[i].wrapped).Status()]
                    for i in bad
                },
                "unsafe_faces": sorted(i for i in graph._unsafe_faces if i in by_index),
            }
        )
    return out


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


def _key(entry: dict) -> tuple:
    return (entry["module"], entry["function"], entry["line"], entry["faces"])


def main() -> None:
    FaceGraph.local_degradation = property(_read, _write)
    FaceGraph.common_valid_solid = _proof
    result._take_inventory_once = _once
    only = set(sys.argv[1:])
    parts = []
    for path in _files():
        name = str(path.relative_to(CORPUS))
        if only and name not in only:
            continue
        part = _load(path)
        _current.clear()
        _current.update(skips=[], other_unproved=[], reads={}, frames={})
        try:
            result._take_inventory(part)
            final = "ok"
        except Exception as error:  # noqa: BLE001 -- the outcome is the record
            final = {"error": f"{type(error).__name__}: {error}"}
        for state in _current["frames"].values():
            _current["other_unproved"].extend(state["pending"])
        entry = {"file": name, "sha256": sha256(path), "strict": _current["strict"]}
        if "degraded" in _current:
            entry["degraded"] = _current["degraded"]
            if "graph" in _current:
                entry["solids"] = _solids(part, _current["graph"])
            entry["reads"] = _current["reads"]
            entry["skips"] = sorted(_current["skips"], key=_key)
            entry["other_unproved"] = sorted(_current["other_unproved"], key=_key)
        entry["result"] = final
        parts.append(entry)
        print(name, entry["strict"], entry.get("degraded", "-"), len(entry.get("skips", ())),
              file=sys.stderr, flush=True)
    text = json.dumps(
        {
            "quiddity_revision": revision(QUIDDITY),
            "retry_message": RETRY_MESSAGE,
            "retried": [p["file"] for p in parts if "degraded" in p],
            "parts": parts,
        },
        indent=1,
        sort_keys=True,
        allow_nan=False,
    )
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(text + "\n")


if __name__ == "__main__":
    main()
