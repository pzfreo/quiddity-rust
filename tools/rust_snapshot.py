#!/usr/bin/env python3
"""Generate/check Rust-owned regression snapshots, or print their review diff.

QUIDDITY_CORPUS=/path/to/corpus python3 tools/rust_snapshot.py
python3 tools/rust_snapshot.py --check
python3 tools/rust_snapshot.py --diff BASE_REF [--file corpus.json.gz]

Python only orchestrates Cargo and deterministic compression; every expected value is produced
by Rust tests named rust_snapshot. No Python CAD implementation is imported or executed.
"""
from __future__ import annotations
import argparse
import difflib
import gzip
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests/fixtures/rust-snapshot"

def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()

def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def inputs() -> dict[str, str]:
    # All committed fixture INPUTS and independent references, including embedded STEP and
    # legacy fingerprints/notes; expected snapshots themselves are excluded.
    found = {
        str(p.relative_to(ROOT)): digest(p)
        for p in sorted((ROOT / "tests/fixtures").rglob("*"))
        if p.is_file() and OUT not in p.parents
    }
    corpus = Path(os.environ["QUIDDITY_CORPUS"])
    manifest = json.loads((ROOT / "tests/fixtures/corpus.json").read_text())
    for row in manifest["files"]:
        path = corpus / row["file"]
        value = digest(path)
        if value != row["sha256"]:
            raise SystemExit(f"corpus input changed: {row['file']}")
        found[f"corpus/{row['file']}"] = value
    return found

def diff(ref: str, only: str | None) -> None:
    prefix = "tests/fixtures/rust-snapshot/"
    previous = set(git("ls-tree", "-r", "--name-only", ref, "--", prefix).splitlines())
    current = {str(p.relative_to(ROOT)) for p in OUT.rglob("*.json.gz")}
    for name in sorted(previous | current):
        if not name.endswith(".json.gz") or (only and not name.endswith(only)):
            continue
        before = subprocess.run(["git", "show", f"{ref}:{name}"], cwd=ROOT, capture_output=True)
        old = gzip.decompress(before.stdout).decode().splitlines(True) if before.returncode == 0 else []
        path = ROOT / name
        new = gzip.decompress(path.read_bytes()).decode().splitlines(True) if path.exists() else []
        print("".join(difflib.unified_diff(old, new, fromfile=f"{ref}/{name}", tofile=name)), end="")

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--check", action="store_true", help="replay into a temporary directory and require byte-identical snapshots")
    group.add_argument("--diff", metavar="REF", help="print a decompressed unified diff against a Git revision")
    parser.add_argument("--file", help="limit --diff to a snapshot suffix")
    args = parser.parse_args()
    if args.diff:
        diff(args.diff, args.file)
        return
    corpus = os.environ.get("QUIDDITY_CORPUS")
    if not corpus or not Path(corpus).is_dir():
        raise SystemExit("export QUIDDITY_CORPUS to the pinned corpus directory")
    # Commit source changes first: the recorded commit must identify exactly the generator's
    # code. Snapshot files may differ during an intentional regeneration.
    changed = git("status", "--porcelain", "--untracked-files=all").splitlines()
    if any(not line[3:].startswith("tests/fixtures/rust-snapshot/") for line in changed):
        raise SystemExit("commit source/input changes before regenerating the Rust snapshot")
    source = git("rev-parse", "HEAD")
    release = git("describe", "--tags", "--abbrev=0", "--match", "v*")
    tagged_source = git("tag", "--points-at", source).splitlines()
    before_inputs = inputs()
    env = dict(os.environ, CARGO_BUILD_JOBS="6", RUST_TEST_THREADS="6", QUIDDITY_CORPUS_REQUIRED="1")
    with tempfile.TemporaryDirectory(prefix="quiddity-rust-snapshot-") as temporary:
        generated = Path(temporary)
        env["QUIDDITY_SNAPSHOT_DIR"] = temporary
        subprocess.run(["cargo", "test", "--workspace", "--release", "rust_snapshot", "--", "--nocapture"], cwd=ROOT, env=env, check=True)
        if inputs() != before_inputs or git("rev-parse", "HEAD") != source or any(not line[3:].startswith("tests/fixtures/rust-snapshot/") for line in git("status", "--porcelain", "--untracked-files=all").splitlines()):
            raise SystemExit("source/inputs changed during generation")
        files = {str(p.relative_to(generated)): digest(p) for p in sorted(generated.rglob("*.json.gz"))}
        if not files or "corpus.json.gz" not in files or "pmi.json.gz" not in files:
            raise SystemExit("snapshot replay did not cover corpus and PMI")
        if args.check:
            committed = {str(p.relative_to(OUT)): digest(p) for p in sorted(OUT.rglob("*.json.gz"))}
            if files != committed:
                missing = sorted(committed.keys() - files.keys())
                added = sorted(files.keys() - committed.keys())
                changed = sorted(k for k in files.keys() & committed.keys() if files[k] != committed[k])
                raise SystemExit(f"Rust snapshot differs: missing={missing}, added={added}, changed={changed}")
            provenance = json.loads((OUT / "provenance.json").read_text())
            if provenance["inputs"] != before_inputs or provenance["snapshots"] != files:
                raise SystemExit("snapshot provenance/input digests differ")
            print(f"HEAD reproduces all {len(files)} snapshots byte for byte")
            return
        provenance = {"schema": 1, "source_commit": source, "source_tags": tagged_source,
                      "release_anchor": release, "command": "python3 tools/rust_snapshot.py",
                      "inputs": before_inputs, "snapshots": files}
        (generated / "provenance.json").write_text(json.dumps(provenance, indent=2, sort_keys=True) + "\n")
        # Replace only after every Rust replay succeeded. Do not leave stale expected files.
        backup = OUT.with_name(".rust-snapshot-previous")
        if backup.exists():
            raise SystemExit(f"resolve leftover backup {backup} before regenerating")
        staging = OUT.with_name(".rust-snapshot-next")
        shutil.copytree(generated, staging)
        try:
            if OUT.exists():
                OUT.rename(backup)
            staging.rename(OUT)
        except BaseException:
            if backup.exists() and not OUT.exists():
                backup.rename(OUT)
            raise
        finally:
            if staging.exists():
                shutil.rmtree(staging)
        if backup.exists():
            shutil.rmtree(backup)
        size = sum((OUT / name).stat().st_size for name in files)
        print(f"Recorded Rust {source} ({release}): {len(files)} compressed snapshots, {size:,} bytes")

if __name__ == "__main__":
    main()
