"""Actual configured transport and Goose-client benchmark contract."""
import copy
import json
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import Mock
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
from host_profiles import run_benchmark, render_report, validate_report, sample, fixtures


class HostProfilesBenchmarkContract(unittest.TestCase):
    def test_real_profiles_actual_goose_and_complete_file_oracles(self):
        report = run_benchmark(Path(os.environ.get("XNIP_TEST_BINARY", ROOT / "target/debug/xnip")),
                               repeats=2, warmups=1,
                               names=["tiny-config", "crlf", "reject-stale", "write-create", "delete", "rename", "reject-late-batch"])
        self.assertEqual(len(report["profiles"]), 14)
        self.assertEqual(len(report["results"]), 14)
        self.assertEqual(sum(len(row["samples"]) for row in report["results"]), 28)
        self.assertTrue(all(item["configured_call_passed"] for item in report["profiles"]))
        self.assertIn("shared transport", render_report(report))
        broken = copy.deepcopy(report)
        broken["results"][0]["samples"][0]["outcomes"] = {}
        with self.assertRaises(AssertionError):
            validate_report(broken)
        rejected = copy.deepcopy(report)
        row = next(row for row in rejected["results"] if row["scenario"] == "reject-stale" and row["lane"] == "goose-official-client")
        row["samples"][0]["calls"] = []
        with self.assertRaises(AssertionError):
            validate_report(rejected)
        client = Mock()
        client.bridge.return_value = {"ok": False, "adapter_error": "cannot spawn configured executable: Permission denied"}
        case = next(case for case in fixtures() if case["name"] == "reject-stale")
        with tempfile.TemporaryDirectory(dir=ROOT / "target") as temporary:
            project = Path(temporary)
            with self.assertRaisesRegex(RuntimeError, "connection/transport failed"):
                sample("goose-official-client", {}, project, client, project, case, "reject-stale")
        evidence = ROOT / "target/host-profiles/preflight.json"
        evidence.parent.mkdir(parents=True, exist_ok=True)
        evidence.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    unittest.main(verbosity=2)
