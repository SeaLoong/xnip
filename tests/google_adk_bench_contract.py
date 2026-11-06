"""Real Google ADK MCP benchmark and rejection-evidence contracts."""
import asyncio
import copy
import os
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
from google_adk_provider import run, render, validate, validate_call


class GoogleAdkBenchmarkContract(unittest.IsolatedAsyncioTestCase):
    async def test_actual_toolset_raw_rpc_and_complete_outcome_oracles(self):
        report = await run(Path(os.environ.get("XNIP_TEST_BINARY", ROOT / "target/debug/xnip")), repeats=2, warmups=1,
                           names=["tiny-config", "crlf", "reject-missing", "reject-ambiguous", "reject-stale-write", "write-create", "reject-late-batch"])
        self.assertEqual(len(report["results"]), 14)
        self.assertEqual(sum(len(row["samples"]) for row in report["results"]), 28)
        self.assertIn("Zero model calls", render(report))
        broken = copy.deepcopy(report)
        broken["results"][0]["samples"][0]["outcomes"] = {}
        with self.assertRaises(AssertionError):
            validate(broken)
        rejected = copy.deepcopy(report)
        row = next(row for row in rejected["results"] if row["scenario"] == "reject-missing" and row["provider"] == "google-adk")
        row["samples"][0]["calls"][-1]["rpc"] = []
        with self.assertRaises(AssertionError):
            validate(rejected)
        infrastructure = {"name": "xnip_edit", "arguments": {}, "projection": {"error": "transport failed"},
                          "exception": None, "rpc": [{"name": "xnip_edit", "arguments": {},
                           "infrastructure_error": {"type": "ConnectionError", "message": "spawn failed"}}]}
        with self.assertRaises(AssertionError):
            validate_call(infrastructure, {"code": -32600, "contains": "old text was not found"})


if __name__ == "__main__":
    unittest.main(verbosity=2)
