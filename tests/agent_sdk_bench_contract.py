"""Real SDK MCP bridge measurement and complete artifact oracles."""
import asyncio
import copy
import os
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
from agent_sdk_bridge import encoded, digest, run, render, stats, validate, validate_call


class BridgeBenchmarkContract(unittest.TestCase):
    def test_real_sdk_tools_samples_and_independent_files(self):
        host = os.environ.get("XNIP_AGENT_SDK_HOST", "agents-sdk")
        binary = os.environ.get("XNIP_TEST_BINARY", str(ROOT / "target/debug/xnip"))
        report = asyncio.run(run(host, binary, repeats=2, warmups=1,
            names=["tiny-config", "crlf", "write-create", "reject-missing", "reject-ambiguous",
                   "reject-stale-write", "reject-late-batch", "preview", "create-rename-delete"]))
        self.assertEqual(len(report["results"]), 18)
        self.assertTrue(all(r["correct_runs"] == 2 for r in report["results"]))
        self.assertIn("Zero model calls", render(report))
        broken = copy.deepcopy(report)
        broken["results"][0]["samples"][0]["outcomes"] = {}
        self.assertRaises(AssertionError, validate, broken)
        broken = copy.deepcopy(report)
        broken["results"][0]["samples"][0]["calls"][0]["argument_sha256"] = "wrong"
        self.assertRaises(AssertionError, validate, broken)
        broken = copy.deepcopy(report)
        broken["results"][0]["samples"][0]["calls"][0]["expected_error"] = True
        self.assertRaises(AssertionError, validate, broken)
        for provider in ("direct-mcp", host):
            broken = copy.deepcopy(report)
            row = next(r for r in broken["results"] if r["scenario"] == "reject-missing" and r["provider"] == provider)
            sample = row["samples"][0]
            call = sample["calls"][-1]
            message = "cannot spawn configured executable: Permission denied"
            if provider == "direct-mcp":
                call["projection"] = {"jsonrpc_error": {"code": -32600, "message": message}}
            elif host == "autogen":
                call["projection"]["result"][0]["content"] = message
            else:
                call["exception"]["message"] = message
            call["projection_bytes"] = 0 if call["projection"] is None else len(encoded(call["projection"]))
            call["projection_sha256"] = None if call["projection"] is None else digest(encoded(call["projection"]))
            call["exception_bytes"] = len((call["exception"] or {}).get("message", "").encode())
            for field in ("projection_bytes", "exception_bytes"):
                sample[field] = sum(c[field] for c in sample["calls"])
                row["summary"][field] = stats([s[field] for s in row["samples"]])
            self.assertRaises(AssertionError, validate, broken)
            self.assertRaises(AssertionError, validate_call, call, provider, "old text was not found")


if __name__ == "__main__":
    unittest.main()
