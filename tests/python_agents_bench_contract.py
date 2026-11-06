"""Actual SDK measurement runner and independent artifact validation gates."""
import asyncio
import copy
import os
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
from python_agents_provider import run_benchmark, render_report, validate_report, digest, encoded


class PythonAgentBenchmarkContract(unittest.TestCase):
    def test_real_sdk_dispatch_and_complete_file_oracle(self):
        executable = Path(os.environ.get("XNIP_TEST_BINARY", ROOT / "target/debug/xnip"))
        report = asyncio.run(run_benchmark(executable, repeats=2, warmups=1,
            names=["tiny-config", "crlf", "global-replace", "reject-missing", "reject-ambiguous", "write-create", "noop",
                   "bom-crlf", "mixed-eol", "write-overwrite", "large-65536"],
            build_profile="debug"))
        self.assertEqual(len(report["results"]), 44)
        self.assertTrue(all(item["correct_runs"] == 2 for item in report["results"]))
        self.assertTrue(all(sample["model_calls"] == 0 for item in report["results"] for sample in item["samples"]))
        self.assertIn("Zero model calls", render_report(report))
        broken = copy.deepcopy(report)
        broken["results"][0]["samples"][0]["outcomes"] = {}
        with self.assertRaisesRegex(AssertionError, "complete expected file oracle"):
            validate_report(broken)
        for incorrect_message in ["permission denied", "File not found"]:
            wrong_error = copy.deepcopy(report)
            row = next(row for row in wrong_error["results"] if row["framework"] == "langgraph" and row["scenario"] == "reject-missing")
            sample = row["samples"][0]
            call = next(call for call in sample["calls"] if call["phase"] == "mutation")
            call["result"]["content"] = incorrect_message
            call["result_sha256"] = digest(encoded(call["result"]["content"]))
            call["result_bytes"] = len(encoded(call["result"]["content"]))
            sample["result_bytes"] = sum(item["result_bytes"] for item in sample["calls"])
            sample["tool_bytes"] = sample["argument_bytes"] + sample["result_bytes"] + sample["exception_bytes"]
            with self.assertRaisesRegex(AssertionError, "expected SDK receipt/error oracle"):
                validate_report(wrong_error)


if __name__ == "__main__":
    unittest.main()
