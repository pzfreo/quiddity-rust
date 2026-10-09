"""Each test's time in a CI run, from its test jobs' logs (read only: nothing is re-run).

    python3 tools/ci_durations.py RUN > .config/test-times.json
    python3 tools/ci_durations.py RUN --table [N]

The first form writes the shard planner's times (`.github/shards.py`, which deals the tests to
shards by them); `gh run view RUN --log | python3 .github/shards.py times` does the same, but
GitHub refuses a whole run's log once the run has some thirty jobs ("too many API requests"), so
this fetches each test job's log on its own. The second prints the N (default 40) costliest
tests, by Linux time, with both platforms' seconds, then each platform's test count and total.
Needs `gh`, authenticated for the repository.
"""

import importlib.util
import io
import json
import subprocess
import sys
from contextlib import redirect_stdout
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def shards_module():
    spec = importlib.util.spec_from_file_location("shards", ROOT / ".github/shards.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def gh(*args):
    return subprocess.run(["gh", *args], check=True, capture_output=True, text=True).stdout


def main():
    run, rest = sys.argv[1], sys.argv[2:]
    jobs = json.loads(gh("run", "view", run, "--json", "jobs"))["jobs"]
    lines = []
    for job in jobs:
        if "test (" in job["name"]:
            lines.extend(gh("run", "view", "--job", str(job["databaseId"]), "--log").splitlines())
    out = io.StringIO()
    with redirect_stdout(out):
        shards_module().times(lines)
    if not rest:
        sys.stdout.write(out.getvalue())
        return
    times = json.loads(out.getvalue())
    linux, macos = times["Linux"], times["macOS"]
    n = int(rest[1]) if len(rest) > 1 else 40
    print(f"{'Linux s':>8} {'macOS s':>8}  test")
    for name in sorted(linux, key=lambda t: -linux[t])[:n]:
        print(f"{linux[name]:8.1f} {macos.get(name, float('nan')):8.1f}  {name}")
    for platform, t in times.items():
        print(f"{platform}: {len(t)} tests, {sum(t.values()):.0f} s in all")


if __name__ == "__main__":
    main()
