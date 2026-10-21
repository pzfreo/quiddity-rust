"""Where a fixture's Python answers came from: the quiddity revision, and each corpus file's
sha256 (checked by ``tests/corpus_files.rs``)."""

from __future__ import annotations

import hashlib
import subprocess
from pathlib import Path


def revision(quiddity: Path) -> str:
    """The quiddity checkout's commit, with ``-dirty`` when its sources or corpus have
    uncommitted changes (the answers then belong to no commit)."""

    def git(*args: str) -> str:
        return subprocess.run(
            ["git", "-C", str(quiddity), *args], check=True, capture_output=True, text=True
        ).stdout.strip()

    head = git("rev-parse", "HEAD")
    return head + "-dirty" if git("status", "--porcelain", "--", "src", "tests/corpus") else head


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()
