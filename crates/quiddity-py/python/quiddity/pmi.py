"""Haecceity PMI JSON interchange on STEP paths, with verified writes."""
import os
import tempfile
from ._native import pmi_json


def read(step: str, part: int | None = None) -> str:
    """Return haecceity.pmi/1 JSON. Multi-part files require a part index."""
    args = ["export", os.fspath(step)]
    if part is not None:
        args += ["--part", str(part)]
    return pmi_json(args)


def write(step: str, document: str, output: str, part: int | None = None) -> str:
    """Import PMI JSON, verify it by reading back, then atomically write output."""
    with tempfile.TemporaryDirectory(prefix="quiddity-pmi-") as directory:
        path = os.path.join(directory, "pmi.json")
        with open(path, "w", encoding="utf-8") as stream:
            stream.write(document)
        args = ["import", os.fspath(step), path, "-o", os.fspath(output)]
        if part is not None:
            args += ["--part", str(part)]
        return pmi_json(args)
