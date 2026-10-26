"""Shard selection must honor the macOS PR profile's default filter."""
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest

import shards


class ShardsTest(unittest.TestCase):
    def test_deals_every_matching_test_once_and_never_a_filtered_test(self):
        with tempfile.TemporaryDirectory() as tmp:
            listing = Path(tmp) / "tests.json"
            times = Path(tmp) / "times.json"
            listing.write_text(json.dumps({"rust-suites": {"suite": {
                "binary-id": "bin",
                "testcases": {
                    "slow": {"filter-match": {"status": "matches"}},
                    "fast": {"filter-match": {"status": "matches"}},
                    "excluded": {"filter-match": {"status": "mismatch"}},
                },
            }}}))
            times.write_text(json.dumps({"macOS": {"bin slow": 20, "bin fast": 1}}))
            outputs = []
            for k in [1, 2]:
                stdout = io.StringIO()
                with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(io.StringIO()):
                    shards.shard(listing, times, "macOS", k, 2)
                outputs.append(stdout.getvalue())
            self.assertIn("test(=slow)", outputs[0])
            self.assertIn("test(=fast)", outputs[1])
            self.assertNotIn("excluded", "".join(outputs))
            self.assertEqual("".join(outputs).count("test(=slow)"), 1)
            self.assertEqual("".join(outputs).count("test(=fast)"), 1)


if __name__ == "__main__":
    unittest.main()
