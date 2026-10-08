"""Record what draftwright reads from the semantic PMI of the AP242 fixtures.

    DRAFTWRIGHT=<draftwright checkout> <its venv>/bin/python tools/capture_pmi_draftwright.py

Run in draftwright's own environment (``uv sync`` in a checkout of its ``origin/main``). For
every ``tests/fixtures/ap242/nist/*.stp.gz`` and ``tests/fixtures/ap242/specify/*.step.gz``,
``tests/fixtures/ap242/draftwright/<file stem>.json.gz`` gets:

- ``report``: ``draftwright.pmi.extract_pmi_report`` (the ``pmi='annotate'`` path) on the file
  in its own STEP space (no part frame): every source in its census (category, OCCT type code,
  outcome, reason) and every record it projects, with every field except those holding
  coordinates (``ref_pts``, ``ref_bbox``, ``reference_bboxes``, ``cylindrical_refs``,
  ``circular_refs``, ``angular_reference``, ``angular_references``, ``reference_normal``),
  and its material facts;
- ``part21``: each reader of ``draftwright._pmi_part21`` on the file, its facts as returned:
  geometric tolerances, datum occurrences and definitions, dimension associations, display
  facts (decimal places, unit, basic), the length factor, material, manufacturing
  requirements (prose and structured), surface labels and common labels. A reader that raises
  is recorded with its exception, not skipped.

Values are draftwright's, recorded as returned; nothing is interpreted. Provenance: the
draftwright commit and each input's sha256.
"""

from __future__ import annotations

import dataclasses
import enum
import gzip
import json
import math
import os
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _provenance import sha256  # noqa: E402

import draftwright  # noqa: E402
from draftwright import _pmi_part21 as p21  # noqa: E402
from draftwright.pmi import extract_pmi_report  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "tests" / "fixtures" / "ap242"
OUT = FIXTURES / "draftwright"
DRAFTWRIGHT = Path(os.environ.get("DRAFTWRIGHT", Path(draftwright.__file__).parents[2])).resolve()

#: Record fields holding coordinates in the extraction space, left out (see the docstring).
GEOMETRY = {
    "ref_pts",
    "ref_bbox",
    "reference_bboxes",
    "cylindrical_refs",
    "circular_refs",
    "angular_reference",
    "angular_references",
    "reference_normal",
}

READERS = [
    "read_geometric_tolerances",
    "read_datum_occurrences",
    "read_datum_definitions",
    "read_dimension_associations",
    "read_dimension_display_facts",
    "read_dimension_length_factor",
    "read_material_properties",
    "read_manufacturing_requirements",
    "read_structured_manufacturing_requirements",
    "read_surface_labels",
    "read_common_labels",
]


def plain(value):
    """JSON form of draftwright's values: dataclasses as objects, tuples as lists."""
    if dataclasses.is_dataclass(value) and not isinstance(value, type):
        return {f.name: plain(getattr(value, f.name)) for f in dataclasses.fields(value)}
    if isinstance(value, enum.Enum):
        return value.name
    if isinstance(value, (list, tuple)):
        return [plain(v) for v in value]
    if isinstance(value, dict):
        return {str(k): plain(v) for k, v in value.items()}
    if isinstance(value, float) and not math.isfinite(value):
        return repr(value)
    return value


def record(rec) -> dict:
    out = plain(rec)
    for key in GEOMETRY:
        out.pop(key, None)
    return out


def capture(step: Path, name: str) -> dict:
    report = extract_pmi_report(step)
    part21 = {}
    for reader in READERS:
        try:
            part21[reader] = {"facts": plain(getattr(p21, reader)(step))}
        except Exception as exc:  # recorded, not skipped: the oracle says what draftwright does
            part21[reader] = {"error": f"{type(exc).__name__}: {exc}"}
    return {
        "file": name,
        "report": {
            "error": report.error,
            "source_name": report.source_name,
            "sources": plain(report.sources),
            "records": [record(r) for r in report.records],
            "material_facts": plain(report.material_facts),
            "material_error": report.material_error,
        },
        "part21": part21,
    }


def git_revision(path: Path) -> str:
    import subprocess

    def git(*args: str) -> str:
        return subprocess.run(
            ["git", "-C", str(path), *args], check=True, capture_output=True, text=True
        ).stdout.strip()

    head = git("rev-parse", "HEAD")
    return head + "-dirty" if git("status", "--porcelain", "--", "src") else head


def inputs() -> list[Path]:
    return sorted((FIXTURES / "nist").glob("*.stp.gz")) + sorted(
        (FIXTURES / "specify").glob("*.step.gz")
    )


def main() -> None:
    revision = git_revision(DRAFTWRIGHT)
    OUT.mkdir(parents=True, exist_ok=True)
    for path in inputs():
        name = path.name.removesuffix(".gz")
        with tempfile.TemporaryDirectory() as tmp:
            step = Path(tmp) / name
            step.write_bytes(gzip.decompress(path.read_bytes()))
            body = capture(step, name)
        body["sha256"] = sha256(path)
        body["draftwright"] = revision
        stem = name.rsplit(".", 1)[0]
        text = json.dumps(body, allow_nan=False, sort_keys=True, separators=(",", ":")) + "\n"
        (OUT / f"{stem}.json.gz").write_bytes(gzip.compress(text.encode(), mtime=0))
        print(
            name,
            len(body["report"]["sources"]),
            "sources",
            len(body["report"]["records"]),
            "records",
            file=sys.stderr,
        )


if __name__ == "__main__":
    main()
