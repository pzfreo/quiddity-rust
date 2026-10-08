"""Write AP242 PMI onto corpus parts with specify-core, as reader inputs.

    QUIDDITY_CORPUS=../quiddity/tests/corpus NIST_PMI=<NIST-PMI-STEP-Files> \
    ASSEMBLY=<two-part assembly.step> SPECIFY_CORE=../specify-core \
        $SPECIFY_CORE/.venv/bin/python tools/make_specify_inputs.py

Each case runs specify-core's command line (``analyse``, then ``write --accept-defaults
--intent``) on a part with fixed answers, and writes into ``tests/fixtures/ap242/specify/``:
``<case>.step.gz`` (the file specify-core wrote), ``<case>.intent.json`` (its intent),
``<case>.answers.json`` (the answers given) and ``<case>.meta.json`` (input file and sha256,
specify-core commit, what ``write`` reported).

These files show how specify-core writes PMI through OpenCascade. They are inputs for the
reader, not a reference for correct AP242 (tests/fixtures/ap242/README.md).

Not reproducible byte for byte: the STEP header's ``FILE_NAME`` time stamp, which OpenCascade
sets to the time of writing. ``--check`` compares a fresh run with the committed fixtures with
that one field masked.
"""

from __future__ import annotations

import gzip
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _provenance import sha256  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "ap242" / "specify"
SPECIFY_CORE = Path(os.environ.get("SPECIFY_CORE", ROOT.parent / "specify-core")).resolve()
CORPUS = Path(os.environ.get("QUIDDITY_CORPUS", ROOT.parent / "quiddity" / "tests" / "corpus"))
NIST_PMI = Path(os.environ.get("NIST_PMI", ""))
ASSEMBLY = Path(
    os.environ.get("ASSEMBLY", ROOT / "tests" / "fixtures" / "ap242" / "assembly" / "assembly.step")
)

#: case -> (input, [(part or None, answers)]). Face numbers are specify-core's own (its
#: questions name them); every case covers what the comment says.
CASES = {
    # ISO 286: an H7 bore (datum B), g6 and f7 shaft fits wholly below nominal, an M8 clearance
    # bolt circle with a position, circular runout, flatness of A, perpendicularity of B,
    # material and ISO 2768-m.
    "spool_fits": (
        ("corpus", "cadgenbench/flanged_spool_132.step"),
        [
            (
                None,
                {
                    "part.material": "Aluminium 6082-T6",
                    "part.general_tolerance": "ISO 2768-m",
                    "part.locating": "position tolerances",
                    "hole.function:holes:34.55": "fit:H7",
                    "hole.function:hole_patterns.bolt_circle:0.16.17.18.20.21": "clearance:M8",
                    "hole.position:hole_patterns.bolt_circle:0.16.17.18.20.21": "0.2",
                    "diameter.fit:turned_steps:87": "g6",
                    "diameter.runout:turned_steps:87": "0.02",
                    "diameter.fit:turned_steps:89": "f7",
                    "datum.A.flatness": "0.02",
                    "datum.B.control": "0.05",
                },
            )
        ],
    ),
    # A turned part with an external thread, a diamond knurl, an f7 diameter and a tapped hole.
    "thumbwheel_thread_knurl": (
        ("corpus", "gramel/GRM-03_thumbwheel_drive_screw.step"),
        [
            (
                None,
                {
                    "part.material": "Stainless steel 303",
                    "part.general_tolerance": "ISO 2768-f",
                    "hole.function:holes:12.13": "tapped:M2",
                    "hole.position:holes:12.13": "0.1",
                    "diameter.fit:turned_steps:2": "thread:M5",
                    "diameter.fit:turned_steps:9": "knurl:diamond",
                    "diameter.fit:turned_steps:3": "f7",
                    "diameter.runout:turned_steps:3": "0.02",
                },
            )
        ],
    ),
    # A turned part with an external M10 thread, a straight knurl and an h6 diameter.
    "bolt_thread_knurl": (
        ("corpus", "gramel/GRM-05_depth_lock_bolt.step"),
        [
            (
                None,
                {
                    "part.material": "Steel C45",
                    "part.general_tolerance": "ISO 2768-m",
                    "diameter.fit:turned_steps:0": "thread:M10",
                    "diameter.fit:turned_steps:4": "knurl:straight",
                    "diameter.fit:turned_steps:6": "h6",
                    "diameter.runout:turned_steps:6": "0.05",
                },
            )
        ],
    ),
    # Tapped holes and a clearance hole, each with a position, and flatness of A.
    "string_post_tapped": (
        ("corpus", "gramel/string_post.step"),
        [
            (
                None,
                {
                    "part.material": "Brass CW614N",
                    "part.general_tolerance": "ISO 2768-f",
                    "hole.function:holes:17.19": "tapped:M2",
                    "hole.position:holes:17.19": "0.1",
                    "hole.function:holes:5": "clearance:M1.6",
                    "hole.position:holes:5": "0.1",
                    "datum.A.flatness": "0.01",
                },
            )
        ],
    ),
    # Three planar datums with flatness and perpendicularities, clearance hole sets positioned
    # to A|B|C (Ø, Ⓜ as specify-core writes them).
    "bracket_positions": (
        ("corpus", "cadgenbench_inputs/cgb207.step"),
        [
            (
                None,
                {
                    "part.material": "Aluminium 6061-T6",
                    "part.general_tolerance": "ISO 2768-m",
                    "part.locating": "position tolerances",
                    "hole.function:hole_patterns.set:233.239.240": "clearance:M8",
                    "hole.position:hole_patterns.set:233.239.240": "0.2",
                    "hole.function:hole_patterns.set:234.235.236": "clearance:M8",
                    "hole.position:hole_patterns.set:234.235.236": "0.5",
                    "datum.A.flatness": "0.05",
                    "datum.B.control": "0.05",
                    "datum.C.control": "0.1",
                },
            )
        ],
    ),
    # specify-core adding to a NIST AP242 part that already has semantic PMI (merge.py).
    "nist_ctc_01_merge": (
        ("nist", "nist_ctc_01_asme1_ap242-e1.stp"),
        [
            (
                None,
                {
                    "part.material": "Aluminium 6082-T6",
                    "part.general_tolerance": "ISO 2768-m",
                    "datum.B.control": "0.05",
                },
            )
        ],
    ),
    # A two-part assembly (a plate and a pin placed twice), both parts written at once, each
    # with its own datum A.
    "assembly_plate_pin": (
        ("assembly", "assembly.step"),
        [
            (
                0,
                {
                    "part.material": "Aluminium 6082-T6",
                    "part.general_tolerance": "ISO 2768-m",
                    "hole.function:holes:11": "tapped:M5",
                    "hole.position:holes:11": "0.25",
                    "hole.function:hole_patterns.grid:6.7.8.9.10.12": "clearance:M6",
                    "hole.position:hole_patterns.grid:6.7.8.9.10.12": "0.6",
                    "datum.A.flatness": "0.02",
                },
            ),
            (
                1,
                {
                    "part.material": "Steel C45",
                    "part.general_tolerance": "ISO 2768-f",
                    "diameter.fit:turned_steps:4": "g6",
                    "diameter.runout:turned_steps:4": "0.02",
                    "hole.function:holes:3.5": "tapped:M5",
                },
            ),
        ],
    ),
}

#: OpenCascade's write time in the header: FILE_NAME's second attribute.
_TIME = re.compile(rb"(FILE_NAME\s*\(\s*'(?:[^']|'')*'\s*,\s*)'[^']*'")


def masked(step: bytes) -> bytes:
    return _TIME.sub(rb"\1'<time stamp>'", step, count=1)


def _source(kind: str, name: str) -> Path:
    return {"corpus": CORPUS, "nist": NIST_PMI, "assembly": ASSEMBLY.parent}[kind] / name


def _revision() -> str:
    def git(*args: str) -> str:
        return subprocess.run(
            ["git", "-C", str(SPECIFY_CORE), *args], check=True, capture_output=True, text=True
        ).stdout.strip()

    head = git("rev-parse", "HEAD")
    return head + "-dirty" if git("status", "--porcelain", "--", "src") else head


def run_case(case: str, source: tuple[str, str], parts: list, out: Path) -> None:
    cli = SPECIFY_CORE / ".venv" / "bin" / "specify-core"
    step = _source(*source)
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        local = tmp / step.name
        shutil.copyfile(step, local)
        pairs = []
        for i, (part, answers) in enumerate(parts):
            analysis, answers_file = tmp / f"analysis{i}.json", tmp / f"answers{i}.json"
            extra = ["--part", str(part)] if part is not None else []
            subprocess.run(
                [str(cli), "analyse", str(local), *extra, "-o", str(analysis)],
                check=True,
                capture_output=True,
                text=True,
            )
            answers_file.write_text(json.dumps(answers, indent=1, sort_keys=True) + "\n")
            pairs += [str(analysis), str(answers_file)]
        written, intent = tmp / f"{case}.step", tmp / "intent.json"
        result = subprocess.run(
            [
                str(cli),
                "write",
                str(local),
                *pairs,
                "-o",
                str(written),
                "--accept-defaults",
                "--intent",
                str(intent),
            ],
            check=True,
            capture_output=True,
            text=True,
        )
        (out / f"{case}.step.gz").write_bytes(gzip.compress(written.read_bytes(), mtime=0))
        (out / f"{case}.intent.json").write_text(intent.read_text())
    answers = [{"part": part, "answers": answers} for part, answers in parts]
    (out / f"{case}.answers.json").write_text(json.dumps(answers, indent=1, sort_keys=True) + "\n")
    # OpenCascade prints its transfer statistics to stdout before specify-core's JSON report.
    report = json.loads(result.stdout[result.stdout.index("\n{\n") + 1 :])
    report.pop("output", None)  # the temporary path
    meta = {
        "case": case,
        "input": f"{source[0]}:{source[1]}",
        "input_sha256": sha256(step),
        "specify_core": _revision(),
        "command": "specify-core analyse [--part N]; specify-core write --accept-defaults --intent",
        "write_report": report,
        "not_byte_for_byte": "the STEP header FILE_NAME time stamp",
    }
    (out / f"{case}.meta.json").write_text(
        json.dumps(meta, ensure_ascii=False, indent=1, sort_keys=True) + "\n"
    )


def main() -> None:
    check = "--check" in sys.argv[1:]
    target = Path(tempfile.mkdtemp()) if check else OUT
    target.mkdir(parents=True, exist_ok=True)
    differ = []
    for case, (source, parts) in CASES.items():
        run_case(case, source, parts, target)
        print(case, file=sys.stderr)
        if check:
            for fresh in sorted(target.glob(f"{case}.*")):
                kept = (OUT / fresh.name).read_bytes()
                new = fresh.read_bytes()
                if fresh.name.endswith(".step.gz"):
                    kept, new = masked(gzip.decompress(kept)), masked(gzip.decompress(new))
                if kept != new:
                    differ.append(fresh.name)
    if check:
        print("differ:", differ or "none", file=sys.stderr)
        sys.exit(1 if differ else 0)


if __name__ == "__main__":
    main()
