"""Record every construction of a ``quiddity._section_recess`` record (ADR 0019's section-recess
geometry, profiles, ends, classification, evidence, patterns and document) with its outcome.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/capture_section_recess.py

The records have no ``recognise_*`` entry point of their own: their validation runs in their
``__post_init__``. Every class's ``__post_init__`` is wrapped, and each distinct construction
is recorded with its inputs and either its ``to_dict()`` or its refusal (the ``ValueError``
message). Two sources, written to ``tests/fixtures/captured/section_recess/values.json.gz``:

1. the Python tests that build these records directly or through recognition (``TESTS``), run
   in this process;
2. ``build_section_recess_document`` on every corpus part, which builds every record the public
   section-recess projection publishes, the document included.

Inputs are recorded as the port's types carry them: a nested record as its class name and its
fields, a tuple as a list, a non-finite float as its string. A construction whose inputs the
port's types cannot carry (a boolean, a list or a wrongly sized tuple where a tuple belongs, a
record of a class the field does not admit, a negative or non-integer index), which Python
refuses by type or shape, is counted in ``skipped`` per class, not silently dropped.
Constructions are deduplicated by class and inputs and written sorted, so the file does not
depend on the order a run builds them in.
"""

from __future__ import annotations

import gzip
import json
import math
import multiprocessing
import os
import sys
import tempfile
from collections import Counter
from pathlib import Path

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src"), str(QUIDDITY)]

import pytest  # noqa: E402

import quiddity._section_recess as section_recess  # noqa: E402
from _provenance import revision  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402
from quiddity.result import build_section_recess_document  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "captured" / "section_recess"
FIXTURES = ROOT / "tests" / "fixtures"
CORPUS = QUIDDITY / "tests" / "corpus"
# Corpus parts are recognised in this many worker processes, alongside the tests.
WORKERS = 4
TESTS = (
    "tests/test_section_recesses.py",
    "tests/test_section_recess_invariants.py",
    "tests/test_section_recess_cutover.py",
    "tests/test_section_recess_geometry_golden.py",
    "tests/test_section_recess_migration.py",
    "tests/test_section_adapter_rounding.py",
    "tests/test_section_projection_refusals.py",
    "tests/test_section_pattern_projection.py",
    "tests/test_corner_section.py",
    "tests/test_open_channel_section.py",
    "tests/test_cylindrical_channel_contract.py",
    "tests/test_cylindrical_channel_public.py",
    "tests/test_cylindrical_passage_contract.py",
    "tests/test_cylindrical_passage_public.py",
    "tests/test_cylindrical_pocket_proofs.py",
    "tests/test_plane_envelope_contract.py",
    "tests/test_mixed_section_pockets.py",
    "tests/test_polygonal_split_mouths.py",
    "tests/test_corner_notch_guards.py",
    "tests/test_support_apertures.py",
    "tests/test_passage_publication_ties.py",
)

# Each wrapped class's fields as the port's types carry them: "f" a number, "fN" a tuple of N
# numbers, "s" a string, "s?" a string or None, "i" a non-negative int, "int" any int, a class
# name (or a tuple of names) a nested record of one of those classes, and ("tuple", kind) a
# tuple of any length (("pair", kind) of exactly two) of that kind.
VERTEX = "PassageSectionVertex"
SURFACES = ("PlanarEndSurface", "CylindricalEndSurface", "PlanarEnvelopeEndSurface")
FIELDS: dict[str, list[tuple[str, object]]] = {
    "PassageFrame": [("origin", "f3"), ("run", "f3"), ("u", "f3"), ("v", "f3")],
    VERTEX: [("point", "f2"), ("bulge", "f")],
    "CylindricalEndSurface": [
        ("type", "s"),
        ("axis_point", "f3"),
        ("axis_direction", "f2"),
        ("radius", "f"),
        ("branch", "s"),
    ],
    "ClosedSectionProfile": [("closure", "s"), ("boundary", ("tuple", VERTEX))],
    "OpenSectionProfile": [
        ("closure", "s"),
        ("boundary", ("tuple", VERTEX)),
        ("opening", ("pair", "f2")),
        ("material_side", "s?"),
    ],
    "PlanarEndSurface": [("type", "s"), ("gradient", "f2")],
    "PlanarEndTerm": [("height", "f"), ("gradient", "f2")],
    "PlanarEnvelopeEndSurface": [
        ("type", "s"),
        ("operator", "s"),
        ("terms", ("pair", "PlanarEndTerm")),
    ],
    "SectionEnd": [("condition", "s"), ("surface", SURFACES)],
    "SectionRecessEnds": [("low", "SectionEnd"), ("high", "SectionEnd")],
    "SectionRecessGeometry": [
        ("type", "s"),
        ("frame", "PassageFrame"),
        ("run_interval", "f2"),
        ("profile", ("ClosedSectionProfile", "OpenSectionProfile")),
        ("ends", "SectionRecessEnds"),
    ],
    "SectionRecessClassification": [("feature_kind", "s"), ("section_shape", "s")],
    "SectionRecessEvidence": [
        ("defining_faces", ("tuple", "i")),
        ("constituent_faces", ("tuple", "i")),
    ],
    "SectionRecessBodyRef": [("index", "i")],
    "SectionRecessFaceRef": [("index", "i")],
    "SectionRecess": [
        ("index", "i"),
        ("body", "i"),
        ("geometry", "SectionRecessGeometry"),
        ("classification", "SectionRecessClassification"),
        ("evidence", "SectionRecessEvidence"),
    ],
    "SectionRecessRefusal": [("body", "i"), ("reason", "s"), ("evidence", "SectionRecessEvidence")],
    "SectionRecessArray": [("members", ("tuple", "i")), ("pitch", "f"), ("direction", "f3")],
    "SectionRecessGrid": [
        ("members", ("tuple", "i")),
        ("rows", "i"),
        ("cols", "i"),
        ("row_pitch", "f"),
        ("col_pitch", "f"),
        ("row_direction", "f3"),
        ("col_direction", "f3"),
        ("center", "f3"),
    ],
    "SectionRecessDocument": [
        ("schema_version", "int"),
        ("reference_scope", "s"),
        ("bodies", ("tuple", "SectionRecessBodyRef")),
        ("faces", ("tuple", "SectionRecessFaceRef")),
        ("occurrences", ("tuple", "SectionRecess")),
        ("refusals", ("tuple", "SectionRecessRefusal")),
        ("patterns", ("tuple", ("SectionRecessArray", "SectionRecessGrid"))),
    ],
}
# The classes whose constructions are recorded: every record `_section_recess` defines (the
# nested passage and end-surface values are carried, but recorded by their own captures).
RECORDED = [name for name in FIELDS if name not in ("PassageFrame", VERTEX, "CylindricalEndSurface")]


class Uncarriable(Exception):
    pass


def _number(value) -> float:
    if isinstance(value, bool) or not isinstance(value, int | float):
        raise Uncarriable
    value = float(value)
    return value if math.isfinite(value) else str(value)


def _carry(kind, value):
    """*value* as the port's type for *kind* carries it, or Uncarriable."""

    if kind == "f":
        return _number(value)
    if kind in ("f2", "f3"):
        if not isinstance(value, tuple) or len(value) != int(kind[1]):
            raise Uncarriable
        return [_number(v) for v in value]
    if kind == "s":
        if not isinstance(value, str):
            raise Uncarriable
        return value
    if kind == "s?":
        if value is not None and not isinstance(value, str):
            raise Uncarriable
        return value
    if kind in ("i", "int"):
        if type(value) is not int or (kind == "i" and value < 0):
            raise Uncarriable
        return value
    if isinstance(kind, tuple) and kind[0] in ("tuple", "pair"):
        if not isinstance(value, tuple) or (kind[0] == "pair" and len(value) != 2):
            raise Uncarriable
        return [_carry(kind[1], v) for v in value]
    names = (kind,) if isinstance(kind, str) else kind
    name = type(value).__name__
    if name not in names or name not in FIELDS:
        raise Uncarriable
    return _record(value)


def _record(value) -> dict:
    name = type(value).__name__
    return {"record": name, **{f: _carry(k, getattr(value, f)) for f, k in FIELDS[name]}}


def _finite(value):
    """JSON without NaN or infinities: a non-finite float as its string."""

    if isinstance(value, float) and not math.isfinite(value):
        return str(value)
    if isinstance(value, dict):
        return {key: _finite(item) for key, item in value.items()}
    if isinstance(value, list | tuple):
        return [_finite(item) for item in value]
    return value


_values: dict[str, dict] = {}
_skipped: Counter[str] = Counter()
_sources: Counter[str] = Counter()


def _note(key: str, given: dict, outcome: dict, name: str) -> None:
    if key not in _values:
        _values[key] = {"given": given, **outcome}
        _sources[f"test {name}"] += 1


def _wrap(cls):
    real = cls.__post_init__

    def post_init(self) -> None:
        try:
            given = _record(self)
        except Uncarriable:
            _skipped[cls.__name__] += 1
            return real(self)
        key = json.dumps(given, sort_keys=True)
        try:
            real(self)
        except Exception as error:
            # Anything but a ValueError is recorded with its type, so the replay sees it.
            message = str(error) if type(error) is ValueError else f"{type(error).__name__}: {error}"
            _note(key, given, {"refused": message}, cls.__name__)
            raise
        _note(key, given, {"dict": _finite(self.to_dict())}, cls.__name__)

    cls.__post_init__ = post_init
    return real


class _Recorder:
    def pytest_sessionstart(self, session) -> None:  # noqa: ARG002
        self._real = {name: _wrap(getattr(section_recess, name)) for name in RECORDED}

    def pytest_sessionfinish(self, session, exitstatus) -> None:  # noqa: ARG002
        for name, real in self._real.items():
            getattr(section_recess, name).__post_init__ = real


def _load(path: Path):
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def _wrap_all() -> None:
    for name in RECORDED:
        _wrap(getattr(section_recess, name))


def _corpus_part(name: str) -> tuple[str, dict, Counter, str | None]:
    """One corpus part's constructions, in a worker process with the classes wrapped."""

    _values.clear()
    _skipped.clear()
    try:
        document = build_section_recess_document(_load(CORPUS / name))
    except Exception as error:  # noqa: BLE001
        refusal = f"{type(error).__name__}: {error}"
        print(name, "refused", refusal, file=sys.stderr)
        return name, dict(_values), Counter(_skipped), refusal
    print(name, len(document.occurrences), file=sys.stderr)
    return name, dict(_values), Counter(_skipped), None


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    corpus = json.loads((FIXTURES / "corpus.json").read_text())["files"]
    names = sorted(entry["file"] for entry in corpus)
    # The corpus runs in worker processes while the tests run here; the results are merged in
    # file order after the tests' own, so the output does not depend on which finishes first.
    with multiprocessing.get_context("spawn").Pool(WORKERS, initializer=_wrap_all) as pool:
        pending = pool.map_async(_corpus_part, names, chunksize=1)
        cwd = os.getcwd()
        os.chdir(QUIDDITY)
        try:
            status = pytest.main(
                ["-q", "-p", "no:cacheprovider", "-p", "no:randomly", *TESTS],
                plugins=[_Recorder()],
            )
        finally:
            os.chdir(cwd)
        print("pytest exit", status, file=sys.stderr)
        results = pending.get()

    refused: dict[str, str] = {}
    for name, part_values, skipped, refusal in results:
        if refusal is not None:
            refused[name] = refusal
        _skipped.update(skipped)
        for key, value in part_values.items():
            if key not in _values:
                _values[key] = value
                _sources[f"corpus {value['given']['record']}"] += 1

    values = [_values[key] for key in sorted(_values)]
    payload = {
        "quiddity_revision": revision(QUIDDITY),
        "skipped": dict(sorted(_skipped.items())),
        "sources": dict(sorted(_sources.items())),
        "corpus_refused": refused,
        "values": values,
    }
    text = json.dumps(payload, allow_nan=False)
    (OUT / "values.json.gz").write_bytes(gzip.compress((text + "\n").encode(), mtime=0))
    print(len(values), "values", dict(_skipped), file=sys.stderr)


if __name__ == "__main__":
    main()
