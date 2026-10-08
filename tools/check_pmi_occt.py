"""Record what OpenCascade's XCAF reader makes of the files haecceity's PMI writer wrote.

    cargo test --release -p haecceity --test pmi_write export_written_files -- --ignored
    /Users/paul/repos/specify-core/.venv/bin/python tools/check_pmi_occt.py

Every ``tests/fixtures/ap242/write/<case>.step.gz`` (written by the Rust test
``export_written_files``: every kind of item the writer makes on a specify-core part, and
specify-core's intents written onto their original corpus files) is read exactly as
``tools/capture_pmi_occt.py`` reads the NIST and specify-core files (``STEPCAFControl_Reader``,
GD&T and names on, specify-core's ``load.py`` numbering), and the capture is written beside it
as ``<case>.occt.json.gz``. ``crates/haecceity/tests/pmi_write.rs``
(``opencascade_reads_the_written_files``) compares each capture with haecceity's reading of the
same file, which equals what was written, and requires a verdict for every difference in
``tests/fixtures/known_pmi_write.json`` ("occt"). OpenCascade is a cross-check, not an
authority: where it misreads (its ``known_misreads``), haecceity is rust-correct with the
clause and the file text.

Run with the specify-core venv (OCP 7.9); nothing in specify-core is changed.
"""

from __future__ import annotations

import gzip
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from capture_pmi_occt import capture_file  # noqa: E402

WRITE = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "ap242" / "write"


def main() -> None:
    files = sorted(WRITE.glob("*.step.gz"))
    if not files:
        sys.exit(f"no written files in {WRITE}: run export_written_files first")
    for path in files:
        record = capture_file(path)
        stem = path.name.removesuffix(".step.gz")
        text = json.dumps(record, allow_nan=False, sort_keys=True, separators=(",", ":")) + "\n"
        (WRITE / f"{stem}.occt.json.gz").write_bytes(gzip.compress(text.encode(), mtime=0))
        counts = [
            sum(len(p[k]) for p in record["parts"])
            for k in ("dimensions", "geometric_tolerances", "datums")
        ]
        print(path.name, "dims/tols/datums", *counts, file=sys.stderr)


if __name__ == "__main__":
    main()
