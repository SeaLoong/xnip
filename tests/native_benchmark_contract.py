"""Contracts for benchmark plumbing; these fake responses never enter benchmark results."""
import hashlib
import json
import math
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
import native_tools as bench
import native_report as report


class Reply:
    def request(self, _):
        return {"ok": False, "output": "", "error": "native edit rejected its target", "elapsed_ms": 1}


class BenchmarkContract(unittest.TestCase):
    def test_native_error_is_not_erased_by_an_empty_success_output_field(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            backend = bench.Backend("test-result-plumbing", "test", "opencode", {}, Reply())
            calls = backend.execute(root, {"a": "old"}, {"a": "new"},
                [{"file": "a", "old": "old", "text": "new"}], "fixture")
            self.assertEqual(calls[0]["returned_text"], "native edit rejected its target")
            self.assertFalse(calls[0]["ok"])

    def test_whole_file_text_uses_lf_in_the_message_and_leaves_newlines_to_native_io(self):
        payload = bench.aider_payload({"a": "old\r\n"}, {"a": "new\r\n"}, "whole")
        self.assertEqual(payload, "a\n" + bench.FENCE + "\nnew\n" + bench.FENCE + "\n")
        self.assertNotIn("\r", payload)

    def test_oracle_detects_additional_files_and_keeps_eof_distinct(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bench.reset_files(root, {"a": "new"})
            (root / "unexpected").write_bytes(b"side effect")
            result = bench.observe(bench.actual_files(root, {"a": "new"}), {"a": "new"})
            self.assertFalse(result["byte_exact"])
            self.assertFalse(result["lf_normalized_equal"])
        self.assertFalse(bench.observe({"a": b"new\n"}, {"a": "new"})["lf_normalized_equal"])
        self.assertTrue(bench.observe({"a": b"new\r\n"}, {"a": "new\n"})["lf_normalized_equal"])

    def test_context_profile_uses_real_surrounding_lines_and_preserves_crlf_bytes(self):
        before = "head\r\nold\r\ntail\r\n"
        after = "head\r\nnew\r\ntail\r\n"
        edits = bench.contextual_edits({"a": before}, {"a": after})
        self.assertEqual(edits, [{"file": "a", "old": before, "text": after}])

def fixture_report(unit="tokens"):
    def row(name, family, cost, exact, repetitions):
        return {"backend": name, "family": family, "input_cost": {"tokens": cost, "utf8_bytes": cost * 4},
            "returned_text_cost": {"tokens": 0, "utf8_bytes": 0},
            "outcomes": [{"byte_exact": exact, "lf_normalized_equal": True}] * repetitions,
            "stable_outcome": True}
    return {"counter": {"unit": unit}, "repeats": 3, "platform": "fixture",
        "xnip_binary_sha256": "fixture-not-an-execution",
        "backends": [{"name": "xnip-edit", "family": "xnip", "version": "fixture"},
                     {"name": "native-test", "family": "aider", "version": "fixture"}],
        "cases": [{"name": "small", "results": [row("xnip-edit", "xnip", 10, True, 1),
                                                  row("native-test", "aider", 20, True, 3)]},
                  {"name": "large", "results": [row("xnip-edit", "xnip", 90, True, 3),
                                                  row("native-test", "aider", 360, False, 1)]}],
        "probes": [], "unavailable": []}


class RelativeReportContract(unittest.TestCase):
    def test_ratios_are_paired_per_task_and_not_weighted_by_tokens_or_repetitions(self):
        stats = report.paired_statistics(fixture_report())
        native = stats["backends"][1]
        self.assertAlmostEqual(native["input_geomean_all"], math.sqrt(2 * 4))
        self.assertEqual(native["pairs"], 2)
        self.assertEqual(stats["pairs"][1]["input_ratio"], 2)
        self.assertEqual(stats["pairs"][3]["input_ratio"], 4)
        self.assertEqual(stats["backends"][0]["input_geomean_all"], 1)

    def test_equal_outcome_subset_excludes_nonexact_native_output(self):
        stats = report.paired_statistics(fixture_report())
        native = stats["backends"][1]
        self.assertEqual(native["exact_pairs"], 1)
        self.assertEqual(native["exact_scenarios"], ["small"])
        self.assertEqual(native["input_geomean_exact_pairs"], 2)
        self.assertEqual(stats["common_exact_scenarios"], ["small"])

    def test_empty_exact_subset_is_unknown_and_byte_counter_keeps_its_unit(self):
        fixture = fixture_report("utf8_bytes")
        for case in fixture["cases"]:
            case["results"][1]["outcomes"][0]["byte_exact"] = False
        stats = report.paired_statistics(fixture)
        self.assertEqual(stats["unit"], "utf8_bytes")
        self.assertEqual(stats["backends"][1]["exact_pairs"], 0)
        self.assertIsNone(stats["backends"][1]["input_geomean_exact_pairs"])
        self.assertEqual(stats["common_exact_scenarios"], [])

    def test_returned_and_combined_ratios_do_not_treat_aider_unknown_as_zero(self):
        fixture = fixture_report()
        unknown = report.paired_statistics(fixture)
        self.assertIsNone(unknown["pairs"][1]["native_captured_returned_text"])
        self.assertIsNone(unknown["backends"][1]["captured_returned_text_geomean_all"])
        self.assertIsNone(unknown["backends"][1]["input_plus_captured_returned_text_geomean_all"])
        fixture["backends"][1]["family"] = "opencode"
        for case, baseline_output, native_output in zip(fixture["cases"], (5, 15), (2, 3)):
            case["results"][0]["returned_text_cost"]["tokens"] = baseline_output
            case["results"][1]["returned_text_cost"]["tokens"] = native_output
            case["results"][1]["family"] = "opencode"
        stats = report.paired_statistics(fixture)
        self.assertAlmostEqual(stats["backends"][1]["captured_returned_text_geomean_all"], math.sqrt(0.4 * 0.2))
        self.assertAlmostEqual(stats["backends"][1]["input_plus_captured_returned_text_geomean_all"], math.sqrt((22 / 15) * (363 / 105)))
        self.assertAlmostEqual(stats["backends"][1]["input_plus_captured_returned_text_geomean_exact_pairs"], 22 / 15)

    def test_captured_empty_return_is_zero_but_zero_denominator_is_undefined(self):
        fixture = fixture_report()
        fixture["backends"][1]["family"] = "codex"
        for case in fixture["cases"]:
            case["results"][0]["returned_text_cost"]["tokens"] = 5
            case["results"][1]["family"] = "codex"
        stats = report.paired_statistics(fixture)
        self.assertEqual(stats["backends"][1]["captured_returned_text_geomean_all"], 0)
        self.assertEqual(stats["pairs"][1]["captured_returned_text_ratio"], 0)
        fixture["cases"][0]["results"][0]["returned_text_cost"]["tokens"] = 0
        stats = report.paired_statistics(fixture)
        self.assertIsNone(stats["pairs"][1]["captured_returned_text_ratio"])
        self.assertIsNotNone(stats["pairs"][1]["input_plus_captured_returned_text_ratio"])

    def test_conclusion_changes_with_measured_receipts_instead_of_reusing_old_claims(self):
        fixture = fixture_report()
        fixture["cases"][0]["name"] = "tiny-config"
        fixture["backends"][1].update(name="opencode-edit", family="opencode")
        for case in fixture["cases"]:
            case["results"][1].update(backend="opencode-edit", family="opencode")
            case["results"][0]["returned_text_cost"]["tokens"] = 50
            case["results"][1]["returned_text_cost"]["tokens"] = 2
        oversized = report.render(fixture, report.paired_statistics(fixture))
        self.assertIn("longer captured return reverses that advantage", oversized)
        for case in fixture["cases"]:
            case["results"][0]["returned_text_cost"]["tokens"] = 1
        compact = report.render(fixture, report.paired_statistics(fixture))
        self.assertIn("xnip has the smaller combined captured exchange in this task.", compact)
        self.assertNotIn("longer captured return reverses that advantage", compact)
        self.assertNotIn("0.83x", compact)

    def test_report_only_needs_no_binary_and_preserves_raw_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, markdown = root / "results.json", root / "results.md"
            source.write_text(json.dumps(fixture_report()) + "\n", encoding="utf8")
            original = source.read_bytes()
            process = subprocess.run([sys.executable, str(ROOT / "benches/native_tools.py"),
                "--report-only", str(source), "--markdown", str(markdown),
                "--xnip", str(root / "does-not-exist")], capture_output=True, text=True)
            self.assertEqual(process.returncode, 0, process.stderr)
            self.assertEqual(source.read_bytes(), original)
            relative = json.loads((root / "relative-results.json").read_text(encoding="utf8"))
            self.assertEqual(relative["source_json_sha256"], hashlib.sha256(original).hexdigest())
            self.assertIn("not captured", markdown.read_text(encoding="utf8"))
            for name in ("overview", "input-ratios", "scenario-ratios", "byte-exact"):
                image = ET.parse(root / "charts" / (name + ".svg"))
                self.assertTrue(image.getroot().tag.endswith("svg"))
            # Regenerating must produce the same derived artifacts, without a timestamp.
            before = {path.relative_to(root): path.read_bytes() for path in root.rglob("*") if path.is_file()}
            report.write_report(fixture_report(), markdown, source)
            self.assertEqual(before, {path.relative_to(root): path.read_bytes() for path in root.rglob("*") if path.is_file()})

    def test_overview_uses_measured_ratios_and_labels_different_exact_cohorts(self):
        fixture = fixture_report()
        svg = report.overview_chart(report.paired_statistics(fixture), fixture)
        ET.fromstring(svg)
        self.assertIn("2.83x", svg)
        self.assertIn("2.00x (1/2)", svg)
        self.assertIn("Exact subsets differ", svg)
        self.assertIn("aider returned text is unknown", svg)
        self.assertIn("not full agent sessions", svg)
        fixture["cases"][0]["results"][1]["input_cost"]["tokens"] = 80
        updated = report.overview_chart(report.paired_statistics(fixture), fixture)
        self.assertIn("5.66x", updated)
        self.assertNotEqual(svg, updated)

    def test_overview_keeps_an_empty_exact_cohort_unknown(self):
        fixture = fixture_report()
        for case in fixture["cases"]:
            case["results"][1]["outcomes"][0]["byte_exact"] = False
        svg = report.overview_chart(report.paired_statistics(fixture), fixture)
        self.assertIn("n/a (0/2)", svg)
        self.assertNotIn("0.00x (0/2)", svg)

    def test_zero_or_absent_baseline_cannot_produce_plausible_ratios(self):
        fixture = fixture_report()
        fixture["cases"][0]["results"][0]["input_cost"]["tokens"] = 0
        with self.assertRaisesRegex(ValueError, "positive baseline"):
            report.paired_statistics(fixture)
        fixture = fixture_report()
        fixture["cases"][0]["results"] = fixture["cases"][0]["results"][1:]
        with self.assertRaisesRegex(ValueError, "missing xnip"):
            report.paired_statistics(fixture)


if __name__ == "__main__":
    unittest.main()
