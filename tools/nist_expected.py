"""Write NIST's expected PMI for each NIST fixture, from the STEP File Analyzer's own data.

    python3 -I tools/nist_expected.py SFA-5.51.zip NIST_DIR

The STEP File Analyzer and Viewer (usnistgov/SFA) checks the semantic PMI of the NIST test
models against NIST's expected PMI. That data ships inside ``STEP-File-Analyzer.exe`` of the
release zip as ``SFA-NIST-files.zip`` (a freewrap archive member, stored as one zlib stream):
per model a spreadsheet ``SFA-PMI-<model>.xlsx`` whose rows are (entity type, PMI as SFA prints
it), and ``SFA-PMI-NIST-coverage.csv``, the expected count of each PMI element per model.
``sfa-nist.tcl`` (``nistReadExpectedPMI``) reads exactly these: column A and B of the first sheet.

For every ``tests/fixtures/ap242/nist/<model>_*.stp.gz`` this writes
``tests/fixtures/ap242/nist/<model>.expected.json``: every spreadsheet row verbatim (all its
columns), the model's coverage column verbatim, the drawing's unit note (``NIST_DIR/PDF``, by
``pdftotext``; STC models have no drawing), the length units the STEP file's unit contexts and
measures name, and the provenance (release zip, executable, embedded zip and spreadsheet
sha256). Nothing is interpreted: SFA's strings are recorded as SFA has them. Inputs are
untrusted downloads: run with ``python3 -I``; only the standard library is used.
"""

from __future__ import annotations

import csv
import gzip
import hashlib
import io
import json
import re
import subprocess
import sys
import xml.etree.ElementTree as ET
import zipfile
import zlib
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NIST = ROOT / "tests" / "fixtures" / "ap242" / "nist"
EXE = "STEP-File-Analyzer.exe"
MAIN = "{http://schemas.openxmlformats.org/spreadsheetml/2006/main}"


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def embedded_zip(exe: bytes) -> tuple[int, bytes]:
    """The offset and bytes of the zlib stream in ``exe`` that inflates to SFA-NIST-files.zip."""
    for m in re.finditer(rb"\x78[\x01\x5e\x9c\xda]", exe):
        try:
            head = zlib.decompressobj().decompress(exe[m.start() : m.start() + 4096], 64)
        except zlib.error:
            continue
        if head.startswith(b"PK\x03\x04") and b"SFA-PMI-NIST-coverage.csv" in head:
            inflater = zlib.decompressobj()
            data = inflater.decompress(exe[m.start() :])
            if not inflater.eof:
                raise ValueError("embedded zip stream is truncated")
            return m.start(), data
    raise ValueError(f"no embedded SFA-NIST-files.zip in {EXE}")


def sheet_rows(xlsx: bytes) -> list[dict]:
    z = zipfile.ZipFile(io.BytesIO(xlsx))
    strings = []
    if "xl/sharedStrings.xml" in z.namelist():
        for si in ET.fromstring(z.read("xl/sharedStrings.xml")).findall(f"{MAIN}si"):
            strings.append("".join(t.text or "" for t in si.iter(f"{MAIN}t")))
    sheet = ET.fromstring(z.read("xl/worksheets/sheet1.xml"))
    rows = []
    for row in sheet.iter(f"{MAIN}row"):
        cells = {}
        for c in row.findall(f"{MAIN}c"):
            column = re.match(r"[A-Z]+", c.get("r")).group(0)
            v, kind = c.find(f"{MAIN}v"), c.get("t")
            if kind == "inlineStr":
                cells[column] = "".join(t.text or "" for t in c.iter(f"{MAIN}t"))
            elif v is None:
                continue
            elif kind == "s":
                cells[column] = strings[int(v.text)]
            else:
                cells[column] = v.text
        if cells:
            rows.append({"row": int(row.get("r")), "cells": dict(sorted(cells.items()))})
    return rows


def coverage(text: str, model: str) -> list[list[str]]:
    """The model's column of the coverage table: (PMI element, expected count) where stated."""
    table = list(csv.reader(io.StringIO(text)))
    header = table[0]
    column = header.index(model.removeprefix("nist_"))
    return [[r[0], r[column]] for r in table[1:] if r and r[0] and len(r) > column and r[column]]


def _entities(text: str) -> dict[int, str]:
    flat = re.sub(r"\s+", " ", text)
    return {
        int(i): body
        for i, body in re.findall(r"#(\d+)\s*=\s*(.*?);(?=\s*#\d+\s*=|\s*ENDSEC)", flat)
    }


def _unit_name(body: str) -> str | None:
    if "LENGTH_UNIT" not in body:
        return None
    si = re.search(r"SI_UNIT\(\s*([^,]*?)\s*,\s*\.METRE\.\s*\)", body)
    if si:
        prefix = si.group(1).strip(".").lower()
        return ("" if prefix in ("$", "") else prefix) + "metre"
    conversion = re.search(r"CONVERSION_BASED_UNIT\(\s*'([^']*)'", body)
    return f"conversion_based_unit '{conversion.group(1)}'" if conversion else "other"


#: ``MEASURE_WITH_UNIT(value, #unit)``, typed (``LENGTH_MEASURE(1.)``) or not, simple or a leaf.
_MEASURE = re.compile(
    r"(?<![A-Z_])MEASURE_WITH_UNIT\s*\(\s*(?:[A-Z_]+\s*\([^()]*\)|[^,()]+)\s*,\s*#(\d+)\s*\)"
)


def step_units(text: str) -> dict:
    entities = _entities(text)
    contexts: Counter = Counter()
    for body in entities.values():
        m = re.search(r"GLOBAL_UNIT_ASSIGNED_CONTEXT\s*\(\s*\(([^)]*)\)", body)
        if m:
            for ref in re.findall(r"#(\d+)", m.group(1)):
                name = _unit_name(entities.get(int(ref), ""))
                if name:
                    contexts[name] += 1
    measures: Counter = Counter()
    for body in entities.values():
        for ref in _MEASURE.findall(body):
            name = _unit_name(entities.get(int(ref), ""))
            if name:
                measures[name] += 1
    return {
        "unit_context_length_units": dict(sorted(contexts.items())),
        "length_measure_units": dict(sorted(measures.items())),
    }


def drawing_units(pdf: Path) -> str | None:
    text = subprocess.run(
        ["pdftotext", "-layout", str(pdf), "-"], capture_output=True, text=True, check=True
    ).stdout
    m = re.search(r"UNITS:\s*([A-Z]+)", text)
    return m.group(0) if m else None


def main() -> None:
    release, nist_dir = Path(sys.argv[1]), Path(sys.argv[2])
    release_bytes = release.read_bytes()
    exe = zipfile.ZipFile(io.BytesIO(release_bytes)).read(EXE)
    offset, data = embedded_zip(exe)
    archive = zipfile.ZipFile(io.BytesIO(data))
    coverage_text = archive.read("SFA-PMI-NIST-coverage.csv").decode("utf-8-sig")
    pdfs = sorted((nist_dir / "PDF").glob("*.pdf"))
    for fixture in sorted(NIST.glob("nist_*_ap242-*.stp.gz")):
        model = re.match(r"(nist_[a-z]+_\d+)", fixture.name).group(1)
        member = f"SFA-PMI-{model}.xlsx"
        xlsx = archive.read(member)
        drawing = next((p for p in pdfs if p.name.startswith(model + "_")), None)
        record = {
            "model": model,
            "step_file": fixture.name.removesuffix(".gz"),
            "step_sha256": _sha(gzip.decompress(fixture.read_bytes())),
            "source": {
                "release": release.name,
                "release_sha256": _sha(release_bytes),
                "executable": EXE,
                "executable_sha256": _sha(exe),
                "embedded": "SFA-NIST-files.zip",
                "embedded_offset": offset,
                "embedded_sha256": _sha(data),
                "spreadsheet": member,
                "spreadsheet_sha256": _sha(xlsx),
                "reader": "sfa-nist.tcl nistReadExpectedPMI: columns A (type) and B (PMI) of sheet 1",
            },
            "units": {
                "drawing": drawing.name if drawing else None,
                "drawing_note": drawing_units(drawing) if drawing else None,
                **step_units(gzip.decompress(fixture.read_bytes()).decode("latin-1")),
            },
            "coverage": coverage(coverage_text, model),
            "rows": sheet_rows(xlsx),
        }
        out = NIST / f"{model}.expected.json"
        out.write_text(json.dumps(record, ensure_ascii=False, indent=1, sort_keys=True) + "\n")
        print(out.name, len(record["rows"]), "rows", file=sys.stderr)


if __name__ == "__main__":
    main()
