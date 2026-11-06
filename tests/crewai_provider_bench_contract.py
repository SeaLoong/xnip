"""Actual CrewAI benchmark calls and rejection evidence."""
import copy
import os
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
from crewai_provider import run, render, validate, validate_call


class CrewAIBenchmarkContract(unittest.TestCase):
    def test_actual_sdk_calls_complete_file_oracles_and_typed_rejections(self):
        report = run(Path(os.environ.get("XNIP_TEST_BINARY", ROOT / "target/debug/xnip")), repeats=2, warmups=1,
                     names=["tiny-config", "crlf", "reject-stale", "reject-ambiguous", "write-create", "delete", "reject-late-batch"])
        self.assertEqual(len(report["results"]), 14)
        self.assertEqual(sum(len(row["samples"]) for row in report["results"]), 28)
        self.assertIn("Zero model calls", render(report))
        broken = copy.deepcopy(report)
        broken["results"][0]["samples"][0]["outcomes"] = {}
        with self.assertRaises(AssertionError):
            validate(broken)
        broken = copy.deepcopy(report)
        row = next(row for row in broken["results"] if row["provider"] == "crewai" and row["scenario"] == "reject-stale")
        row["samples"][0]["calls"][-1]["rpc"] = []
        with self.assertRaises(AssertionError):
            validate(broken)
        infrastructure = {"name": "xnip_edit", "arguments": {}, "projection": None,
            "exception": {"type": "RuntimeError", "message": "cannot spawn Permission denied"}, "rpc": []}
        with self.assertRaises(AssertionError):
            validate_call(infrastructure, {"code": -32600, "contains": "old text was not found"})


if __name__ == "__main__":
    unittest.main(verbosity=2)
