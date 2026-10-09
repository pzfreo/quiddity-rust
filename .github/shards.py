"""Split the test suite into shards of about equal time, for CI (`.github/workflows/suite.yml`).

    python3 shards.py LIST TIMES PLATFORM K N

LIST is `cargo nextest list --message-format json`. TIMES gives each test's seconds by platform
(`.config/test-times.json`, keyed `<binary-id> <test name>`). Every listed test is dealt, longest
first, to the shard with the least time so far (a test TIMES lacks counts one second), so the N
shards together run each listed test exactly once whatever TIMES says: stale times only make the
shards less even. Prints shard K's (1-based) nextest filterset, and its size on stderr.

    gh run view RUN --log | python3 shards.py times > .config/test-times.json

writes TIMES from a CI run's log (each test's time as its shard ran it, one test at a time).
"""

import json
import re
import sys


def shard(listing, times, platform, k, n):
    with open(listing) as f:
        suites = json.load(f)["rust-suites"]
    with open(times) as f:
        cost = json.load(f)[platform]
    tests = sorted(
        (suite["binary-id"], name) for suite in suites.values() for name in suite["testcases"]
    )
    seconds = {t: cost.get(f"{t[0]} {t[1]}", 1.0) for t in tests}
    load = [0.0] * n
    dealt = {}
    for t in sorted(tests, key=lambda t: (-seconds[t], t)):
        i = min(range(n), key=lambda i: (load[i], i))
        load[i] += seconds[t]
        dealt[t] = i
    mine = [t for t in tests if dealt[t] == k - 1]
    assert mine, f"shard {k} of {n} has no tests"
    print(
        f"shard {k} of {n}: {len(mine)} of {len(tests)} tests, about {load[k - 1]:.0f} s",
        file=sys.stderr,
    )
    print(" | ".join(f"(binary_id(={b}) & test(={name}))" for b, name in mine))


def times(log):
    platforms = {"ubuntu": "Linux", "macos": "macOS"}
    out = {p: {} for p in platforms.values()}
    passed = re.compile(
        r"(?:^|/ )test \((\w+)-latest, \d+\)\t.*\s(?:PASS|FAIL)\s+\[\s*([\d.]+)s\]\s+\(\s*\d+/\d+\)\s+(\S+)\s+(\S+)\s*$"
    )
    for line in log:
        # (A job's log lines start with its name, `linux / test (ubuntu-latest, 3)`.)
        m = passed.search(re.sub(r"(\x1b|\^\[)\[[0-9;]*m", "", line))
        if m:
            out[platforms[m[1]]][f"{m[3]} {m[4]}"] = round(float(m[2]), 1)
    json.dump({p: dict(sorted(t.items())) for p, t in out.items()}, sys.stdout, indent=1)
    print()


if __name__ == "__main__":
    if sys.argv[1:] == ["times"]:
        times(sys.stdin)
    else:
        shard(sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), int(sys.argv[5]))
