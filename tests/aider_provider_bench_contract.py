"""Exercise real Aider replies and the provider report's independent file gates."""
import copy
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
from aider_provider import run_benchmark, render_report, validate_report


class AiderProviderBenchContract(unittest.TestCase):
    def test_actual_coders_outcomes_byte_oracles_and_artifact_gates(self):
        report = run_benchmark(repeats=2, warmups=1, selected=["tiny-config", "aider-create", "aider-noop",
            "aider-reject-missing", "aider-approval-denied", "aider-dry-run", "aider-latin1"])
        self.assertEqual(len(report["results"]), 40)
        self.assertEqual(sum(len(row["samples"]) for row in report["results"]), 80)
        self.assertTrue(all(row["correct_runs"] == 2 for row in report["results"]))
        self.assertTrue(all(value["model_calls"] == 0 for row in report["results"] for value in row["samples"]))
        native = next(row for row in report["results"] if row["provider"] == "xnip-native" and row["scenario"] == "tiny-config")
        self.assertTrue(all(value["bridge"]["actions"] == {"plan": 1, "commit": 1} for value in native["samples"]))
        denied = [row for row in report["results"] if row["provider"] == "xnip-native" and row["scenario"] in ["aider-approval-denied", "aider-dry-run"]]
        self.assertTrue(all(value["bridge"]["total_bytes"] == 0 for row in denied for value in row["samples"]))
        rendered = render_report(report)
        self.assertIn("Zero model completions", rendered)
        self.assertIn(report["provenance"]["binary"]["sha256"], rendered)
        incomplete = copy.deepcopy(report)
        incomplete["results"][0]["samples"][0]["outcomes"] = {}
        with self.assertRaisesRegex(AssertionError, "complete file oracle"):
            validate_report(incomplete)
        altered = copy.deepcopy(report)
        altered["results"][0]["samples"][0]["outcomes"]["src/example.txt"]["expected"]["bytes"] += 1
        with self.assertRaises(AssertionError):
            validate_report(altered)


if __name__ == "__main__":
    unittest.main()
