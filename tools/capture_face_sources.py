"""Record specify-core's part, face and edge numbering of STEP files, with the instance each
face and edge was read from.

    SPECIFY_CORE=../specify-core QUIDDITY=../quiddity HAECCEITY_NIST_PMI=<NIST-PMI-STEP-Files> \
        ../specify-core/.venv/bin/python tools/capture_face_sources.py

For every file of ``tests/fixtures/corpus.json`` (in the shared corpus), every NIST AP242 PMI
test model (``HAECCEITY_NIST_PMI/*.stp``) and ``tests/fixtures/ap242/assembly/assembly.step``,
loads the file as specify-core does (``specify_core.load.load_all(path, gdt=False)``, through
OpenCascade XCAF) and records its parts in specify-core's order: each part's name, its
placements (``load.parts``), and per face index (``TopExp::MapShapes`` over the part's shape,
specify-core's numbering) the ``#N`` of the ``ADVANCED_FACE`` it was read from, and per edge
index likewise the ``#N`` of its ``EDGE_CURVE``; ``null`` where OpenCascade's transfer binds no
entity to the shape (an edge its healing added). A file specify-core cannot load records the
error. Writes ``tests/fixtures/face_sources.json.gz`` with specify-core's revision, the
OpenCascade version and each file's sha256.

``LoadedPart.face_ranks`` gives model *ranks*, not ``#N``: the model's own labels are not
reachable through OCP here (``IdentLabel`` and ``Number`` return 0 for every entity of a loaded
model), so a rank is mapped to the ``#N`` of the rank-th instance of the file's text, and every
mapped instance is checked: its entity type in the text must be the type OpenCascade read, and
for the first and last face and edge of each part ``WorkSession.NumberFromLabel('#N')`` (the
model's label lookup) must give the rank back.
"""

from __future__ import annotations

import gzip
import json
import os
import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SPECIFY_CORE = Path(os.environ.get("SPECIFY_CORE", ROOT.parent / "specify-core")).resolve()
QUIDDITY = Path(os.environ.get("QUIDDITY", ROOT.parent / "quiddity")).resolve()
NIST = os.environ.get("HAECCEITY_NIST_PMI")
sys.path[:0] = [str(SPECIFY_CORE / "src"), str(Path(__file__).resolve().parent)]

import OCP  # noqa: E402
from OCP.TopAbs import TopAbs_EDGE  # noqa: E402
from OCP.TopExp import TopExp  # noqa: E402
from OCP.TopTools import TopTools_IndexedMapOfShape  # noqa: E402
from specify_core import load  # noqa: E402

from _provenance import sha256  # noqa: E402

FIXTURES = ROOT / "tests" / "fixtures"
OUT = FIXTURES / "face_sources.json.gz"
ASSEMBLY = FIXTURES / "ap242" / "assembly" / "assembly.step"

# A string, the start of an instance (`;`, or `DATA;`, then `#N =` and its type, comments
# allowed between the tokens), or a comment.
_GAP = rb"(?:\s|/\*.*?\*/)*"
_TOKEN = re.compile(
    rb"'(?:[^']|'')*'|;" + _GAP + rb"#(\d+)" + _GAP + rb"=" + _GAP + rb"([A-Za-z0-9_]*)|/\*.*?\*/", re.S
)


def _instances(text: bytes) -> list[tuple[int, str]]:
    """Each instance of the file in text order: its id and its type (``""`` if complex)."""

    return [
        (int(m.group(1)), m.group(2).decode().upper())
        for m in _TOKEN.finditer(text)
        if m.group(1) is not None
    ]


def _step_name(occ_type: str) -> str:
    """``StepShape_AdvancedFace`` -> ``ADVANCED_FACE``."""

    camel = occ_type.split("_", 1)[1]
    return re.sub(r"(?<!^)(?=[A-Z])", "_", camel).upper()


def _edges(shape) -> TopTools_IndexedMapOfShape:
    edges = TopTools_IndexedMapOfShape()
    TopExp.MapShapes_s(shape, TopAbs_EDGE, edges)
    return edges


def _capture(path: Path) -> dict:
    loaded = load.load_all(path, gdt=False)
    listed = load.parts(path)
    instances = _instances(path.read_bytes())
    session = loaded[0].reader.Reader().WS()
    model, transfer = session.Model(), session.TransferReader()
    if model.NbEntities() != len(instances):
        raise RuntimeError(f"{model.NbEntities()} model entities, {len(instances)} in the text")
    ranks = {model.Value(k): k for k in range(1, model.NbEntities() + 1)}

    def source(shape) -> int | None:
        entity = transfer.EntityFromShapeResult(shape, 1)
        if entity is None or entity not in ranks:
            return None
        ident, kind = instances[ranks[entity] - 1]
        want = _step_name(entity.DynamicType().Name())
        if kind != want:
            raise RuntimeError(f"rank {ranks[entity]} is #{ident} {kind}, OpenCascade read {want}")
        return ident

    def check_label(ident: int | None) -> None:
        if ident is None:
            return
        rank = session.NumberFromLabel(f"#{ident}")
        if instances[rank - 1][0] != ident:
            raise RuntimeError(f"#{ident} is rank {rank} to the model, #{instances[rank - 1][0]} in the text")

    parts = []
    for part, info in zip(loaded, listed, strict=True):
        edges = _edges(part.shape)
        faces = [source(part.faces.FindKey(i + 1)) for i in range(part.binding.face_count)]
        edge_ids = [source(edges.FindKey(i + 1)) for i in range(edges.Extent())]
        for ids in (faces, edge_ids):
            if ids:
                check_label(ids[0])
                check_label(ids[-1])
        placements = [
            [[part_trsf.Value(r, c) for c in range(1, 5)] for r in range(1, 4)]
            for part_trsf in part.placements
        ]
        # load.parts is the same walk; its matrices are rounded to 1e-9.
        assert info["name"] == part.name and info["faces"] == len(faces), path
        assert all(
            abs(a - b) <= 1e-9
            for p, q in zip(placements, info["placements"], strict=True)
            for row_p, row_q in zip(p, q)
            for a, b in zip(row_p, row_q)
        ), path
        parts.append({"name": part.name, "faces": faces, "edges": edge_ids, "placements": placements})
    return {"parts": parts}


def _files() -> list[tuple[str, Path]]:
    corpus = json.loads((FIXTURES / "corpus.json").read_text())
    files = [(f"corpus/{e['file']}", QUIDDITY / "tests" / "corpus" / e["file"]) for e in corpus["files"]]
    if NIST:
        files += [(f"nist-pmi/{p.name}", p) for p in sorted(Path(NIST).glob("*.stp"))]
    files.append(("fixture/ap242/assembly/assembly.step", ASSEMBLY))
    return files


def main() -> None:
    """With arguments, captures only the files whose names contain one, to standard output."""

    only = sys.argv[1:]
    out = []
    for name, path in _files():
        if only and not any(o in name for o in only):
            continue
        entry = {"file": name, "sha256": sha256(path)}
        try:
            if path.suffix == ".gz":
                with tempfile.TemporaryDirectory() as tmp:
                    plain = Path(tmp) / path.stem
                    plain.write_bytes(gzip.decompress(path.read_bytes()))
                    entry.update(_capture(plain))
            else:
                entry.update(_capture(path))
        except (ValueError, RuntimeError) as error:
            # specify-core names the file by its path; the capture names it by its name here.
            entry["error"] = re.sub(r"\S*" + re.escape(Path(name).name.removesuffix(".gz")), Path(name).name, str(error))
        out.append(entry)
        print(name, len(entry.get("parts", [])), entry.get("error", ""), file=sys.stderr, flush=True)
    text = json.dumps(
        {
            "specify_core": revision_of(SPECIFY_CORE),
            "opencascade": OCP.__version__,
            "files": out,
        },
        allow_nan=False,
    )
    if only:
        print(text)
    else:
        OUT.write_bytes(gzip.compress((text + "\n").encode(), mtime=0))


def revision_of(repo: Path) -> str:
    """specify-core's commit, ``-dirty`` when its sources have uncommitted changes."""

    import subprocess

    def git(*args: str) -> str:
        return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True, text=True).stdout.strip()

    head = git("rev-parse", "HEAD")
    return head + "-dirty" if git("status", "--porcelain", "--", "src") else head


if __name__ == "__main__":
    main()
