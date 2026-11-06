"""Exercise actual SDK provider measurements and their file/result oracle."""
import copy
import os
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
from openhands_provider import render_report, run_benchmark, validate_report


class OpenHandsProviderBenchContract(unittest.TestCase):
    def test_actual_sdk_tool_pipeline_and_complete_file_oracle(self):
        report = run_benchmark(Path(os.environ.get("XNIP_TEST_BINARY", ROOT / "target/debug/xnip")), repeats=2, warmups=1,
                               names=["tiny-config", "crlf", "openhands-create", "openhands-undo",
                                      "openhands-missing", "openhands-ambiguous"], build_profile="debug")
        self.assertEqual(len(report["results"]), 12)
        self.assertEqual(sum(len(row["samples"]) for row in report["results"]), 24)
        native = [row for row in report["results"] if row["provider"] == "xnip-native"]
        self.assertTrue(all(sample["bridge"]["total_bytes"] > 0 for row in native for sample in row["samples"]))
        self.assertTrue(all(sample["model_calls"] == 0 for row in report["results"] for sample in row["samples"]))
        rendered = render_report(report)
        self.assertIn("Conversation.execute_tool", rendered)
        broken = copy.deepcopy(report)
        broken["results"][0]["samples"][0]["outcomes"] = {}
        with self.assertRaises(AssertionError):
            validate_report(broken)
        broken = copy.deepcopy(report)
        negative = next(row for row in broken["results"] if row["scenario"] == "openhands-missing")
        negative["samples"][0]["calls"][-1]["error"] = "permission denied"
        with self.assertRaises(AssertionError):
            validate_report(broken)
        broken = copy.deepcopy(report)
        broken["results"][0]["samples"][0]["calls"][-1]["is_error"] = True
        with self.assertRaises(AssertionError):
            validate_report(broken)


if __name__ == "__main__":
    unittest.main()
