"""Contracts for benchmark plumbing; these fake responses never enter benchmark results."""
import hashlib
import json
import math
import os
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
import native_tools as bench
import native_report as report


class CounterContract(unittest.TestCase):
    def test_repeated_text_is_cached_without_aliasing_returned_counts(self):
        class CountedText(str):
            encodes = 0
            def encode(self, *args, **kwargs):
                self.encodes += 1
                return super().encode(*args, **kwargs)
        text = CountedText("你好\r\n")
        counter = bench.Counter(None)
        first = counter.measure(text)
        first["utf8_bytes"] = -1
        self.assertEqual(counter.measure(text), {"utf8_bytes": 8, "characters": 4, "tokens": None})
        self.assertEqual(text.encodes, 1)

    def test_cache_can_be_disabled_and_eviction_never_changes_totals(self):
        cached = bench.Counter(None, cache_entries=1, cache_text_bytes=128)
        uncached = bench.Counter(None, cache_entries=0)
        texts = ["é", "e\u0301", "é", "x" * 200, "x" * 200, ""]
        self.assertEqual([cached.measure(text) for text in texts], [uncached.measure(text) for text in texts])
        self.assertEqual(cached.total(iter(texts)), {"utf8_bytes": 407, "characters": 404, "tokens": None})
        self.assertEqual(cached.total([]), {"utf8_bytes": 0, "characters": 0, "tokens": None})


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
            fixture = fixture_report()
            fixture["adapter_fingerprints"] = {"benches/native_report.py": "sampled-renderer-before-regeneration"}
            source.write_text(json.dumps(fixture) + "\n", encoding="utf8")
            original = source.read_bytes()
            process = subprocess.run([sys.executable, str(ROOT / "benches/native_tools.py"),
                "--report-only", str(source), "--markdown", str(markdown),
                "--xnip", str(root / "does-not-exist")], capture_output=True, text=True)
            self.assertEqual(process.returncode, 0, process.stderr)
            self.assertEqual(source.read_bytes(), original)
            relative = json.loads((root / "relative-results.json").read_text(encoding="utf8"))
            self.assertEqual(relative["source_json_sha256"], hashlib.sha256(original).hexdigest())
            self.assertEqual(relative["report_generator_sha256"], hashlib.sha256(Path(report.__file__).read_bytes()).hexdigest())
            self.assertEqual(relative["recorded_generator_sha256"], "sampled-renderer-before-regeneration")
            self.assertIn("not captured", markdown.read_text(encoding="utf8"))
            for name in ("overview", "input-ratios", "scenario-ratios", "byte-exact", "frontier", "readme-frontier"):
                image = ET.parse(root / "charts" / (name + ".svg"))
                self.assertTrue(image.getroot().tag.endswith("svg"))
            # Regenerating must produce the same derived artifacts, without a timestamp.
            before = {path.relative_to(root): path.read_bytes() for path in root.rglob("*") if path.is_file()}
            report.write_report(fixture, markdown, source)
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


class StrategyContract(unittest.TestCase):
    def test_guard_probes_cannot_become_snapshot_writes_or_regenerated_guards(self):
        files = {"a.txt": "version=2\n"}
        edits = [{"file": "a.txt", "old": "version=1\n", "text": "version=3\n"}]
        plans = bench.mcp_plans(files, files, edits, allow_snapshot=True, probe=True)
        self.assertEqual(len(plans), 1)
        self.assertEqual(plans[0]["calls"], [("xnip_edit", edits[0])])
        for format in ("native", "compact"):
            plans = bench.cli_plans(files, files, edits, True, True, format)
            self.assertTrue(all(plan["contract"] == "guarded" for plan in plans))
            self.assertIn("version=1", plans[0]["calls"][0][1])
            self.assertNotIn("version=2", plans[0]["calls"][0][1])
            self.assertFalse(any(program.startswith("write ") for plan in plans for _, program in plan["calls"]))

    def test_grouping_preserves_file_order_and_keeps_global_replacement_explicit(self):
        edits = [{"file": "a", "old": "one", "text": "two"},
                 {"file": "b", "old": "x", "text": "y"},
                 {"file": "a", "old": "two", "text": "three"}]
        self.assertEqual(bench.grouped_pairs(edits), {"a": [["one", "two"], ["two", "three"]],
                                                     "b": [["x", "y"]]})
        self.assertIsNone(bench.grouped_pairs([{**edits[0], "all": True}]))
        plans = bench.mcp_plans({"a": "one one"}, {"a": "two two"},
                               [{**edits[0], "all": True}])
        self.assertTrue(plans[0]["calls"][0][1]["all"])
        self.assertNotIn("file-pairs", [plan["name"] for plan in plans])

    def test_mcp_native_programs_keep_guard_policy_and_count_the_complete_manifest(self):
        files, expected = {"a": "old old\n"}, {"a": "new new\n"}
        edits = [{"file": "a", "old": "old", "text": "new", "all": True}]
        plans = bench.mcp_plans(files, expected, edits)
        native = [plan for plan in plans if plan["name"].startswith("native-program")]
        self.assertTrue(native)
        self.assertIn({"manifest_text": "edit a old new all"},
                      [args for plan in native for _, args in plan["calls"]])
        self.assertTrue(all(plan["contract"] == "guarded" for plan in native))
        counter = bench.Counter(None)
        selected, evidence = bench.choose_plan(plans, counter)
        self.assertFalse(evidence["uses_observed_output_to_select"])
        self.assertEqual(bench.argument_cost(counter, selected["calls"])["utf8_bytes"],
                         min(row["input_cost"]["utf8_bytes"] for row in evidence["candidates"]))
        with mock.patch.object(bench, "cli_plans", side_effect=AssertionError("probe converted")):
            probe = bench.mcp_plans(files, expected, edits, allow_snapshot=True, probe=True)
        self.assertEqual(probe[0]["calls"], [("xnip_edit", edits[0])])

    def test_snapshot_known_file_trims_unique_old_against_each_virtual_revision(self):
        files = {"a": "port=8080\nvalue=1\n"}
        edits = [{"file": "a", "old": "port=8080", "text": "port=3000"},
                 {"file": "a", "old": "port=3000", "text": "port=7000"}]
        trimmed = bench.trim_snapshot_edits(files, {"a": "port=7000\nvalue=1\n"}, edits)
        self.assertEqual([(edit["old"], edit["text"]) for edit in trimmed],
                         [("808", "300"), ("3", "7")])
        self.assertEqual(edits[0]["old"], "port=8080")
        self.assertIsNone(bench.trim_snapshot_edits({}, {"a": "new"}, edits))
        self.assertIsNone(bench.trim_snapshot_edits(files, {"a": "wrong"}, edits))

    def test_snapshot_trim_preserves_nonempty_overlap_uniqueness_and_explicit_all(self):
        files = {"a": "ababa"}
        edits = [{"file": "a", "old": "ababa", "text": "abXba"}]
        trimmed = bench.trim_snapshot_edits(files, {"a": "abXba"}, edits)
        old = trimmed[0]["old"]
        self.assertTrue(old)
        self.assertNotEqual(old, "aba")  # Two overlapping matches, one nonoverlapping match.
        self.assertEqual(sum(files["a"].startswith(old, i) for i in range(len(files["a"]))), 1)
        global_edits = [{"file": "a", "old": "one", "text": "only", "all": True}]
        self.assertEqual(bench.trim_snapshot_edits({"a": "one one"}, {"a": "only only"}, global_edits),
                         global_edits)

    def test_only_snapshot_positive_plans_may_consult_the_complete_snapshot(self):
        files, expected = {"a": "port=8080"}, {"a": "port=3000"}
        edits = [{"file": "a", "old": "port=8080", "text": "port=3000"}]
        with mock.patch.object(bench, "trim_snapshot_edits", side_effect=AssertionError("snapshot consulted")):
            guarded = bench.mcp_plans(files, expected, edits)
            probe = bench.mcp_plans(files, expected, edits, allow_snapshot=True, probe=True)
        self.assertEqual(probe[0]["calls"], [("xnip_edit", edits[0])])
        self.assertFalse(any(plan["name"].startswith("snapshot-") for plan in guarded))
        with mock.patch.object(Path, "read_bytes", side_effect=AssertionError("extra read")), \
             mock.patch.object(Path, "read_text", side_effect=AssertionError("extra read")):
            positive = bench.mcp_plans(files, expected, edits, allow_snapshot=True)
        self.assertIn({"files": {"a": [["808", "300"]]}},
                      [args for plan in positive for _, args in plan["calls"]])

    def test_snapshot_line_candidates_preserve_lf_crlf_and_missing_eof(self):
        def programs(before, after):
            edits = [{"file": "arbitrary.dat", "old": before, "text": after}]
            return [args["manifest_text"] for plan in bench.mcp_plans(
                {"arbitrary.dat": before}, {"arbitrary.dat": after}, edits, allow_snapshot=True)
                if plan["name"].startswith("snapshot-lines") for _, args in plan["calls"]]
        self.assertIn('replace arbitrary.dat 2 "c"', programs("a\nb\n", "a\nc\n"))
        self.assertIn('replace arbitrary.dat 2 "c\r"', programs("a\r\nb\r\n", "a\r\nc\r\n"))
        self.assertIn('replace arbitrary.dat 2 "c"', programs("a\nb", "a\nc"))
        self.assertEqual(programs("a\n", "b"), [])
        self.assertIn('replace arbitrary.dat 1 "\n"', programs("a\n", "\n"))
        self.assertNotIn('replace arbitrary.dat 1 ""', programs("a\n", "\n"))
        self.assertIn('replace arbitrary.dat 1 "x\n\n"', programs("a\n", "x\n\n"))
        self.assertIn('replace arbitrary.dat 2 "y"',
                      programs("a\rb\u2028c\nx\n", "a\rb\u2028c\ny\n"))
        self.assertIn('insert arbitrary.dat 1 before "H"',
                      programs("a\n", "H\na\n"))

    def test_snapshot_line_payloads_are_literals_and_inserted_blank_lines_survive(self):
        for text in ("before", "after", "revert", "+2", "-3", "t2s:4", "was=@secret", "@-"):
            plans = bench.snapshot_line_plans({"a": "old\n"}, {"a": text + "\n"})
            self.assertTrue(plans)
            self.assertIn('replace a 1 "' + text + '"',
                          [args["manifest_text"] for plan in plans for _, args in plan["calls"]])
            self.assertFalse(any(args["manifest_text"] == "replace a 1 " + text
                                 for plan in plans for _, args in plan["calls"]))
        programs = [args["manifest_text"] for plan in bench.snapshot_line_plans(
            {"a": "head\ntail\n"}, {"a": "head\n\ntail\n"}) for _, args in plan["calls"]]
        self.assertIn('insert a 1 "\n"', programs)
        self.assertNotIn('insert a 1 ""', programs)
        programs = [args["manifest_text"] for plan in bench.snapshot_line_plans(
            {"a": "a\nb\nc\n"}, {"a": "H\na\nb\nZ\n"}) for _, args in plan["calls"]]
        self.assertIn('replace a 3 "Z"\ninsert a 1 before "H"', programs)

    def test_selection_is_fixed_by_input_cost_before_any_execution(self):
        counter = bench.Counter(None)
        files, expected = {"a": "old " * 100}, {"a": "new"}
        edits = [{"file": "a", "old": files["a"], "text": "new"}]
        plans = bench.mcp_plans(files, expected, edits, allow_snapshot=True)
        selected, evidence = bench.choose_plan(plans, counter)
        scores = [item["input_cost"]["utf8_bytes"] for item in evidence["candidates"]]
        self.assertEqual(bench.argument_cost(counter, selected["calls"])["utf8_bytes"], min(scores))
        self.assertEqual(selected["contract"], "snapshot")
        self.assertFalse(evidence["uses_observed_output_to_select"])
        self.assertTrue(all("input_sha256" in item for item in evidence["candidates"]))
        self.assertEqual(evidence, bench.choose_plan(plans, counter)[1])

    def test_native_known_file_trim_is_snapshot_only_and_probes_keep_supplied_old(self):
        class Capture:
            def __init__(self):
                self.arguments = []
            def request(self, request):
                self.arguments.append(request["arguments"])
                return {"ok": False, "error": "stop before filesystem observation"}
        for family, mode, old_key, new_key in (
            ("opencode", "opencode-relative-trim", "oldString", "newString"),
            ("gemini", "gemini-relative-trim", "old_string", "new_string"),
        ):
            worker = Capture()
            backend = bench.Backend("known-file", family, mode, {}, worker)
            self.assertEqual(backend.contract_cohort, "snapshot")
            edits = [{"file": "a", "old": "port=8080", "text": "port=3000"}]
            calls = backend.execute(Path("workspace"), {"a": "port=8080"}, {"a": "port=3000"},
                                    edits, "change port")
            self.assertEqual((worker.arguments[-1][old_key], worker.arguments[-1][new_key]), ("808", "300"))
            self.assertEqual(calls[0]["selection"]["selected_contract"], "guarded")
            with mock.patch.object(bench, "trim_snapshot_edits", side_effect=AssertionError("probe trimmed")):
                backend.execute(Path("workspace"), {"a": "port=8080"}, {"a": "port=3000"},
                                edits, "guard probe", probe=True)
            self.assertEqual(worker.arguments[-1][old_key], "port=8080")

    def test_cli_snapshot_reuses_known_file_trim_and_native_line_candidates(self):
        files, expected = {"a": "port=8080"}, {"a": "port=3000"}
        edits = [{"file": "a", "old": "port=8080", "text": "port=3000"}]
        compact = bench.cli_plans(files, expected, edits, True, format="compact")
        self.assertIn(bench.dump({"a": [["808", "300"]]}),
                      [program for plan in compact for _, program in plan["calls"]])
        native = bench.cli_plans(files, expected, edits, True, format="native")
        self.assertIn("edit a 808 300", [program for plan in native for _, program in plan["calls"]])
        line_files = {"a": "old\nbody\n"}
        line_expected = {"a": "new\nbody\n"}
        line_edits = [{"file": "a", "old": "old", "text": "new"}]
        native = bench.cli_plans(line_files, line_expected, line_edits, True)
        self.assertIn('replace a 1 "new"', [program for plan in native for _, program in plan["calls"]])
        self.assertFalse(any(plan["name"].startswith("snapshot-lines") for plan in compact))
        with mock.patch.object(bench, "trim_snapshot_edits", side_effect=AssertionError("guard trimmed")), \
             mock.patch.object(bench, "snapshot_line_plans", side_effect=AssertionError("guard became lines")):
            for format in ("native", "compact"):
                guarded = bench.cli_plans(files, expected, edits, False, format=format)
                probes = bench.cli_plans(files, expected, edits, True, True, format)
                self.assertTrue(all("port=8080" in program for plan in guarded for _, program in plan["calls"]))
                self.assertTrue(all("port=8080" in program for plan in probes for _, program in plan["calls"]))

    def test_relative_mcp_binds_case_cwd_without_global_chdir(self):
        seen = {}
        class FakeMcp:
            def __init__(self, binary, cwd, **options):
                seen["cwd"] = cwd
                seen["options"] = options
            def __enter__(self):
                return self
            def __exit__(self, *_):
                pass
            def call(self, tool, arguments):
                seen["arguments"] = arguments
                self.last_request = {"method": "tools/call", "params": {"name": tool, "arguments": arguments}}
                self.last_response = {"result": {"content": [{"type": "text", "text": "written"}]}}
                return self.last_response["result"]
        original_cwd = Path.cwd()
        with tempfile.TemporaryDirectory() as temporary, mock.patch.object(bench, "Mcp", FakeMcp):
            root = Path(temporary)
            backend = bench.Backend("relative-test", "xnip", "xnip-mcp-relative",
                                    {"executable": str(root / "not-run")})
            calls = backend.execute(root, {"src/a": "old"}, {"src/a": "new"},
                                    [{"file": "src/a", "old": "old", "text": "new"}], "fixture")
            self.assertEqual(seen["cwd"], root)
            self.assertEqual(seen["arguments"]["file"], "src/a")
            self.assertFalse(Path(seen["arguments"]["file"]).is_absolute())
            self.assertIn("serialized_mcp_result", calls[0])
            self.assertIn("setup_elapsed_ms", calls[0])
            text_backend = bench.Backend("relative-text", "xnip", "xnip-mcp-relative",
                                         {"executable": str(root / "not-run"), "receipt_mode": "text"})
            text_backend.execute(root, {"src/a": "old"}, {"src/a": "new"},
                                 [{"file": "src/a", "old": "old", "text": "new"}], "fixture")
            self.assertEqual(seen["options"]["receipts"], "text")
        self.assertEqual(Path.cwd(), original_cwd)

    def test_record_describes_selected_precondition_without_changing_permission_pool(self):
        class FakeBackend:
            name, family, contract_cohort, transport, receipt_mode, mode = (
                "snapshot-fixture", "xnip", "snapshot", "mcp", "structured", "xnip-mcp-snapshot")
            operation, actual = "snapshot-trimmed-file-pairs", "guarded"
            def execute(self, root, files, expected, edits, purpose, probe):
                bench.reset_files(root, expected)
                return [{"input": {"files": {"a": [["old", "new"]]}}, "native_result": {},
                         "returned_text": "written", "ok": True, "elapsed_ms": 1,
                         "selection": {"selected": self.operation, "selected_contract": self.actual}}]
        backend = FakeBackend()
        case = {"files": {"a": "old"}, "expected": {"a": "new"},
                "edits": [{"file": "a", "old": "old", "text": "new"}], "purpose": "test metadata"}
        with tempfile.TemporaryDirectory() as temporary:
            row = bench.record(backend, Path(temporary), case, bench.Counter(None))
            self.assertEqual(row["contract_cohort"], "snapshot")
            self.assertEqual(row["selected_contract"], "guarded")
            self.assertEqual(row["preconditions"], "old block/anchor or whole-file guard")
            backend.operation, backend.actual = "snapshot-lines-test", "snapshot"
            row = bench.record(backend, Path(temporary), case, bench.Counter(None))
            self.assertEqual(row["preconditions"], "known original coordinates; no supplied old guard")

    def test_native_bare_tokens_leave_payload_and_locator_prefixes_quoted(self):
        self.assertEqual(bench.native_quote("port=8080", compact=True), "port=8080")
        self.assertEqual(bench.native_quote("src/example.txt", compact=True), "src/example.txt")
        for value in ("@payload", "=/pattern/", "~/left/", "s/old/new/", "--files-from",
                      "with space", "", "line\nnext", "literal\\n"):
            self.assertTrue(bench.native_quote(value, compact=True).startswith('"'), value)

    def test_native_escaped_literals_use_only_escapes_supported_by_the_executor(self):
        self.assertEqual(bench.native_quote("\b\f\x00\n\r\t", escaped=True),
                         '"\b\f\x00\\n\\r\\t"')

    def test_native_multiline_preserves_literal_backslash_n_and_counts_whole_program(self):
        quoted = bench.native_quote('line\nnext\r\nliteral\\n "quote"\t')
        self.assertIn("line\nnext\r\n", quoted)
        self.assertIn("literal\\\\n", quoted)
        self.assertIn('\\"quote\\"', quoted)
        self.assertIn("\\t", quoted)
        files = {"src/a": "old\nbody"}
        edits = [{"file": "src/a", "old": "old\nbody", "text": "new\nbody"}]
        captured = {}
        def fake_run(command, **kwargs):
            captured.update(command=command, **kwargs)
            return subprocess.CompletedProcess(command, 0, b"", b"written\n")
        with tempfile.TemporaryDirectory() as temporary, mock.patch.object(bench.subprocess, "run", fake_run):
            root = Path(temporary)
            backend = bench.Backend("cli-test", "xnip", "xnip-cli-native-guarded",
                                    {"executable": str(root / "not-run")})
            call = backend.execute(root, files, files, edits, "fixture", probe=True)[0]
            self.assertEqual(captured["input"], call["input"].encode("utf8"))
            self.assertTrue(call["input"].startswith("edit "))
            self.assertIn("src/a", call["input"])
            self.assertIn("old\nbody", call["input"])
            self.assertEqual(captured["cwd"], root)
            self.assertIn("--from-stdin", call["invocation"])
            self.assertIn("--format", call["invocation"])
            self.assertIn("cold CLI", call["timing_scope"])

    def test_unique_context_uses_known_source_uniqueness_instead_of_output_trials(self):
        before = "def first():\n    return 0\n\ndef second():\n    return 0\n"
        after = before.replace("def second():\n    return 0", "def second():\n    return 1")
        self.assertIsNone(bench.unique_groups(before, after, 0))
        self.assertIsNotNone(bench.unique_groups(before, after, 1))
        payload = bench.unique_payload({"a": before}, {"a": after}, "patch", bench.Counter(None))
        self.assertIn("def second()", payload)
        self.assertNotIn("def first()", payload)
        self.assertIn("-    return 0", payload)
        self.assertIn("+    return 1", payload)

    def test_unclosed_whole_keeps_last_eof_and_separates_preceding_files(self):
        payload = bench.eof_aware_whole_payload({"a": "old\n", "b": "old"},
                                              {"a": "new\n", "b": "last"})
        self.assertEqual(payload, "a\n" + bench.FENCE + "\nnew\n" + bench.FENCE +
                         "\nb\n" + bench.FENCE + "\nlast")
        self.assertFalse(payload.endswith("\n"))
        terminated = bench.eof_aware_whole_payload({"a": "old\n"}, {"a": "new\n"})
        self.assertEqual(terminated, "a\n" + bench.FENCE + "\nnew\n" + bench.FENCE + "\n")


class ReceiptModeContract(unittest.TestCase):
    @staticmethod
    def fixture():
        calls = [{"input": {"files": {"a": [["old", "new"]]}}, "returned_text": "written"}]
        base = {"backend": "xnip-adaptive-guarded", "family": "xnip", "transport": "mcp",
                "receipt_mode": "structured", "contract_cohort": "guarded",
                "input_cost": {"tokens": 10}, "returned_text_cost": {"tokens": 1},
                "full_serialized_mcp_cost": {"tokens": 70},
                "outcomes": [{"byte_exact": True}] * 3, "calls": calls}
        text = {**base, "backend": base["backend"] + "-text", "receipt_mode": "text",
                "full_serialized_mcp_cost": {"tokens": 30}}
        return {"counter": {"unit": "tokens"}, "backends": [],
                "cases": [{"name": "fixture", "results": [base, text]}]}

    def test_same_input_and_text_prove_a_full_reply_layer_reduction(self):
        measured = report.receipt_comparisons(self.fixture())
        pair = measured["pairs"][0]
        self.assertTrue(pair["inputs_identical"])
        self.assertTrue(pair["returned_text_identical"])
        self.assertEqual(pair["saved_full_reply"], 40)
        self.assertAlmostEqual(pair["full_reply_ratio"], 30 / 70)
        self.assertEqual(measured["overall"]["paired_count"], 1)

    def test_equal_token_counts_do_not_prove_equal_inputs_or_receipt_effect(self):
        fixture = self.fixture()
        fixture["cases"][0]["results"][1]["calls"] = [
            {"input": {"different": "same-cost"}, "returned_text": "written"}]
        measured = report.receipt_comparisons(fixture)
        self.assertTrue(measured["pairs"][0]["input_cost_equal"])
        self.assertFalse(measured["pairs"][0]["inputs_identical"])
        self.assertIsNone(measured["pairs"][0]["full_reply_ratio"])
        self.assertEqual(measured["overall"]["paired_count"], 0)

    def test_unknown_evidence_or_nonexact_repeat_never_becomes_zero_saving(self):
        fixture = self.fixture()
        fixture["cases"][0]["results"][1]["full_serialized_mcp_cost"] = None
        measured = report.receipt_comparisons(fixture)
        self.assertIsNone(measured["pairs"][0]["saved_full_reply"])
        self.assertEqual(measured["overall"]["paired_count"], 0)
        fixture = self.fixture()
        fixture["cases"][0]["results"][1]["outcomes"] = [{"byte_exact": True}, {"byte_exact": False}]
        self.assertIsNone(report.receipt_comparisons(fixture)["pairs"][0]["full_reply_ratio"])


class FrontierContract(unittest.TestCase):
    @staticmethod
    def fixture():
        def row(name, family, supplied, returned, cohort="guarded", transport="native-function", exact=True, full=None):
            return {"backend": name, "family": family, "contract_cohort": cohort, "transport": transport,
                    "input_cost": {"tokens": supplied}, "returned_text_cost": {"tokens": returned},
                    "outcomes": [{"byte_exact": exact, "lf_normalized_equal": exact}] * 3,
                    "full_serialized_mcp_cost": {"tokens": full} if full is not None else None}
        rows = [
            row("xnip-edit", "xnip", 10, 2, transport="mcp", full=40),
            row("xnip-fast-input", "xnip", 5, 20, transport="mcp", full=45),
            row("xnip-cli", "xnip", 1, 1, transport="cli-stdin"),
            row("xnip-snapshot", "xnip", 1, 0, cohort="snapshot", transport="mcp", full=35),
            row("native-input", "codex", 6, 20, transport="cli-stdin"),
            row("native-exchange", "opencode", 8, 1),
            row("aider-whole-lf", "aider", 1, 0, cohort="snapshot"),
            row("invalid-cheap", "opencode", 1, 0, exact=False),
        ]
        return {"counter": {"unit": "tokens"},
                "backends": [{"name": r["backend"], "family": r["family"]} for r in rows],
                "cases": [{"name": "fixture", "results": rows}]}

    def test_frontier_separates_contracts_transports_and_independent_winners(self):
        frontier = report.frontier_statistics(self.fixture())
        task = frontier["tasks"][0]
        self.assertEqual(task["xnip"]["mcp"]["guarded"]["best_input"]["backend"], "xnip-fast-input")
        self.assertEqual(task["xnip"]["mcp"]["guarded"]["best_input_plus_captured_returned_text"]["backend"], "xnip-edit")
        self.assertEqual(task["xnip"]["cli-stdin"]["guarded"]["best_input"]["backend"], "xnip-cli")
        self.assertEqual(task["native"]["guarded"]["best_input"]["backend"], "native-input")
        self.assertEqual(task["native"]["guarded"]["best_input_plus_captured_returned_text"]["backend"], "native-exchange")
        self.assertEqual(task["native"]["snapshot"]["best_input"]["backend"], "aider-whole-lf")
        self.assertEqual(task["native"]["snapshot"]["best_input_plus_captured_returned_text"]["backend"], "native-exchange")
        self.assertNotIn("invalid-cheap", task["native"]["guarded"]["eligible_profiles"])
        self.assertEqual(task["xnip"]["mcp"]["snapshot"]["best_input"]["backend"], "xnip-snapshot")
        ET.fromstring(report.frontier_chart(frontier))

    def test_snapshot_enabled_symmetrically_accepts_guarded_candidates_on_both_sides(self):
        fixture = self.fixture()
        for row in fixture["cases"][0]["results"]:
            if row["contract_cohort"] == "snapshot":
                row["input_cost"]["tokens"] = 100
        task = report.frontier_statistics(fixture)["tasks"][0]
        self.assertEqual(task["native"]["snapshot"]["best_input"]["backend"], "native-input")
        self.assertEqual(task["native"]["snapshot"]["best_input"]["contract_cohort"], "guarded")
        self.assertEqual(task["xnip"]["mcp"]["snapshot"]["best_input"]["backend"], "xnip-fast-input")
        self.assertEqual(task["xnip"]["mcp"]["snapshot"]["best_input"]["contract_cohort"], "guarded")
        self.assertNotIn("aider-whole-lf", task["native"]["guarded"]["eligible_profiles"])
        self.assertNotIn("xnip-snapshot", task["xnip"]["mcp"]["guarded"]["eligible_profiles"])

    def test_full_mcp_result_invocation_and_unknown_return_are_independent_layers(self):
        fixture = self.fixture()
        fixture["cases"][0]["results"][2]["invocation_cost"] = {"tokens": 1000}
        frontier = report.frontier_statistics(fixture)
        records = {item["backend"]: item for item in frontier["profiles"]}
        self.assertEqual(records["xnip-edit"]["serialized_mcp_result"], 40)
        self.assertEqual(records["xnip-edit"]["input_plus_serialized_mcp_result"], 50)
        self.assertEqual(records["xnip-cli"]["invocation"], 1000)
        self.assertIsNone(records["xnip-cli"]["serialized_mcp_result"])
        self.assertIsNone(records["aider-whole-lf"]["captured_returned_text"])
        self.assertIsNone(records["aider-whole-lf"]["input_plus_captured_returned_text"])
        self.assertEqual(frontier["tasks"][0]["xnip"]["cli-stdin"]["guarded"]["best_input"]["backend"], "xnip-cli")

    def test_readme_frontier_counts_near_ties_and_missing_without_inventing_winners(self):
        frontier = {"unit": "tokens", "cohorts": [
            {"xnip_transport": "mcp", "contract_cohort": "guarded", "input_geomean": 1.0, "input_pairs": 3}],
            "tasks": [{"scenario": str(i), "comparisons": [
                {"xnip_transport": "mcp", "contract_cohort": "guarded", "input_ratio": ratio}]}
                for i, ratio in enumerate([2.0, 1 + 1e-12, 0.5, None])]}
        summaries = report.readme_frontier_summary(frontier)
        guarded = summaries[0]
        self.assertEqual((guarded["wins"], guarded["ties"], guarded["losses"], guarded["missing"]), (1, 1, 1, 1))
        self.assertEqual(guarded["task_count"], 4)
        self.assertEqual(summaries[1]["missing"], 4)
        svg = report.readme_frontier_chart(frontier, {"cases": [None] * 4, "backends": [None] * 5, "repeats": 3})
        ET.fromstring(svg)
        self.assertIn("4", svg)
        self.assertIn("invocation", svg.lower())
        self.assertIn("unknown", svg.lower())

    def test_snapshot_permission_and_selected_guarded_contract_remain_distinct(self):
        fixture = self.fixture()
        row = next(row for row in fixture["cases"][0]["results"] if row["backend"] == "xnip-snapshot")
        row.update(selected_contract="guarded", preconditions="old block/anchor or whole-file guard")
        frontier = report.frontier_statistics(fixture)
        task = frontier["tasks"][0]
        self.assertNotIn("xnip-snapshot", task["xnip"]["mcp"]["guarded"]["eligible_profiles"])
        self.assertEqual(task["xnip"]["mcp"]["snapshot"]["best_input"]["selected_contract"], "guarded")
        markdown = "\n".join(report.frontier_markdown(frontier))
        self.assertIn("xnip-snapshot; mcp; guarded)", markdown)
        self.assertIn("| fixture | xnip-snapshot | mcp | guarded |", markdown)

    def test_one_nonexact_repeat_excludes_a_profile_and_ties_are_deterministic(self):
        fixture = self.fixture()
        row = fixture["cases"][0]["results"][5]
        row["outcomes"] = [{"byte_exact": True}, {"byte_exact": False}, {"byte_exact": True}]
        frontier = report.frontier_statistics(fixture)
        self.assertNotIn("native-exchange", frontier["tasks"][0]["native"]["guarded"]["eligible_profiles"])
        duplicate = dict(fixture["cases"][0]["results"][4], backend="aaa-native")
        fixture["cases"][0]["results"].append(duplicate)
        fixture["backends"].append({"name": "aaa-native", "family": "codex"})
        self.assertEqual(report.frontier_statistics(fixture)["tasks"][0]["native"]["guarded"]["best_input"]["backend"], "aaa-native")



class HarnessContract(unittest.TestCase):
    class Worker:
        def __init__(self):
            self.requests = []

        def request(self, request):
            self.requests.append(request)
            return {"ok": True, "elapsed_ms": 1, "model_visible_result": "rendered",
                    "native_result": {"internal": "not provider content"}}

    def backend(self, family, mode="native", operation="edit"):
        from harness_backends import HarnessBackend
        worker = self.Worker()
        return HarnessBackend(family + "-" + mode + "-" + operation, family, mode,
                              operation, {"commit": "fixture"}, worker, bench.Counter(None))

    def test_observed_dsh_real_read_is_a_separate_call_and_bare_has_none(self):
        files = {"a": "old\n", "b": "other\n"}
        expected = {"a": "new\n", "b": "another\n"}
        edits = [{"file": name, "old": files[name], "text": expected[name]} for name in files]
        with tempfile.TemporaryDirectory() as temporary:
            observed = self.backend("dsh", "fs-observed")
            calls = observed.execute(Path(temporary), files, expected, edits, "fixture")
            self.assertEqual([call["tool"] for call in calls], ["read", "edit", "read", "edit"])
            self.assertEqual([call["role"] for call in calls], ["required-read", "edit"] * 2)
            self.assertEqual(next(request["arguments"]["limit"] for request in observed.worker.requests
                                  if request.get("tool") == "read"), 1)
            bare = self.backend("dsh", "fs-bare")
            self.assertEqual([call["tool"] for call in bare.execute(
                Path(temporary), files, expected, edits, "fixture")], ["edit", "edit"])
            self.assertFalse(observed.frontier_eligible)
            self.assertNotEqual(observed.frontier_group, bare.frontier_group)

    def test_pi_batch_keeps_original_coordinates_and_exact_supplied_guard_on_probes(self):
        files, expected = {"a": "start\nleft\n"}, {"a": "end\nright\n"}
        edits = [{"file": "a", "old": "start", "text": "end"},
                 {"file": "a", "old": "left", "text": "right"}]
        backend = self.backend("pi")
        calls = backend.execute(Path("/fixture"), files, expected, edits, "fixture")
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0]["input"], {"path": "a", "edits": [
            {"oldText": "start", "newText": "end"}, {"oldText": "left", "newText": "right"}]})
        edits = [{"file": "a", "old": "missing", "text": "new", "all": True}]
        calls = backend.execute(Path("/fixture"), files, expected, edits, "fixture", True)
        self.assertEqual(calls[0]["input"]["edits"][0]["oldText"], "missing")

    def test_single_edit_global_fallback_uses_guard_only_on_positive_case(self):
        files, expected = {"a": "same same"}, {"a": "new new"}
        edits = [{"file": "a", "old": "same", "text": "new", "all": True}]
        for family, mode, key in (("goose", "native", "before"), ("dsh", "str-bare", "old_str")):
            backend = self.backend(family, mode)
            calls = backend.execute(Path("/fixture"), files, expected, edits, "fixture")
            self.assertEqual(calls[0]["input"][key], "same same")
            calls = backend.execute(Path("/fixture"), files, expected, edits, "fixture", True)
            self.assertEqual(calls[0]["input"][key], "same")
        writer = self.backend("pi", operation="write")
        self.assertEqual(writer.contract_cohort, "snapshot")
        self.assertFalse(writer.supports_old_guard)

    def test_infrastructure_errors_abort_instead_of_becoming_tool_rejections(self):
        from harness_backends import check_worker, model_catalog
        with self.assertRaisesRegex(RuntimeError, "infrastructure"):
            check_worker({"ok": False, "adapter_error": "runtime source changed"})
        check_worker({"ok": False, "error": "native ambiguity"})
        self.assertEqual(model_catalog([{"name": "edit", "parameters": {"type": "object"},
            "deferLoading": True, "outputSchema": {"type": "string"}}]),
            [{"name": "edit", "description": "", "parameters": {"type": "object"}, "deferLoading": True}])

    def test_record_counts_reads_and_resets_even_after_failure(self):
        class Recorder:
            name, family, mode, operation = "test-observed", "dsh", "fs-observed", "edit"
            contract_cohort, transport, receipt_mode = "guarded", "native-function", None
            returned_text_boundary_known, metadata = True, {"commit": "fixture"}
            def __init__(self, fail=False):
                self.resets, self.fail = [], fail
            def reset_case(self, root):
                self.resets.append(root)
                self.workspace_existed_at_reset = root.exists()
            def execute(self, root, files, expected, edits, purpose, probe):
                if self.fail:
                    raise RuntimeError("execution failed")
                (root / "a").write_bytes(expected["a"].encode("utf8"))
                return [{"tool": tool, "input": args, "native_result": {},
                    "returned_text": text, "ok": True, "elapsed_ms": 1, "role": role}
                    for tool, args, text, role in [
                        ("read", {"path": "a", "limit": 1}, "old", "required-read"),
                        ("edit", {"old": "old", "text": "new"}, "written", "edit")]]
        case = {"files": {"a": "old"}, "expected": {"a": "new"},
            "edits": [{"file": "a", "old": "old", "text": "new"}], "purpose": "contract"}
        counter = bench.Counter(None)
        with tempfile.TemporaryDirectory() as temporary:
            backend = Recorder()
            row = bench.record(backend, Path(temporary), case, counter, 2)
            self.assertEqual(len(backend.resets), 2)
            self.assertTrue(backend.workspace_existed_at_reset)
            self.assertEqual((row["read_call_count"], row["edit_call_count"]), (1, 1))
            self.assertEqual(row["input_cost"], counter.total([
                bench.dump({"path": "a", "limit": 1}), bench.dump({"old": "old", "text": "new"})]))
            self.assertEqual(row["required_read_return_cost"], counter.measure("old"))
            self.assertTrue(row["outcomes"][0]["byte_exact"])
            failing = Recorder(True)
            with self.assertRaisesRegex(RuntimeError, "execution failed"):
                bench.record(failing, Path(temporary), case, counter)
            self.assertEqual(len(failing.resets), 1)

    def test_host_costs_keep_dto_and_unknown_rpc_separate_from_rendered_text(self):
        class Host:
            name, family, mode, operation = "pi-xnip", "pi", "xnip-mcp", "bridge"
            metadata, contract_cohort, transport, receipt_mode = {}, "guarded", "host-mcp-bridge", "text"
            def execute(self, root, files, expected, edits, purpose, probe):
                (root / "a").write_bytes(b"new")
                return [{"tool": "mcp__xnip__xnip_edit", "input": {"file": "a", "old": "old", "text": "new"},
                    "ok": True, "elapsed_ms": 1, "returned_text": "written", "returned_text_boundary_known": True,
                    "native_result": {"native_result": {"value": "x" * 10000}},
                    "host_setup": {"tool_definitions": [{"name": "mcp__xnip__xnip_edit"}],
                        "model_schema_projection": [{"name": "mcp__xnip__xnip_edit", "parameters": {}}],
                        "prompt": "bridge prompt", "server_instructions": "instructions"}}]
        case = {"files": {"a": "old"}, "expected": {"a": "new"}, "edits": [], "purpose": "contract"}
        with tempfile.TemporaryDirectory() as temporary:
            row = bench.record(Host(), Path(temporary), case, bench.Counter(None))
        layers = row["host_layer_costs"]
        self.assertEqual(layers["model_rendered_content"]["utf8_bytes"], 7)
        self.assertGreater(layers["internal_native_dto"]["utf8_bytes"], 10000)
        self.assertIsNone(layers["raw_mcp_tool_result"])
        self.assertIsNone(layers["full_jsonrpc_reply"])
        self.assertEqual(row["receipt_mode"], "text")

    def test_recursive_fingerprint_includes_rust_locks_and_nested_integration_sources(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            paths = ["benches/native/goose-runtime/src/main.rs", "benches/native/goose-runtime/Cargo.lock",
                "integrations/pi/index.mjs", "npm/xnip/package.json", "benches/harness_backends.py",
                "benches/native/target/skip.rs", "integrations/pi/node_modules/skip.js"]
            for relative in paths:
                file = root / relative
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_text(relative, encoding="utf8")
            with mock.patch.object(bench, "ROOT", root):
                hashes = bench.fingerprint()
            self.assertEqual(set(hashes), set(paths[:5]))
            self.assertEqual(hashes[paths[0]], hashlib.sha256(paths[0].encode()).hexdigest())

    def test_declared_new_family_return_boundaries_override_historical_fallback(self):
        fixture = fixture_report()
        fixture["backends"][1]["family"] = "pi"
        for case in fixture["cases"]:
            case["results"][0]["returned_text_cost"]["tokens"] = 5
            case["results"][1].update(family="pi", returned_text_boundary_known=True)
            case["results"][1]["returned_text_cost"]["tokens"] = 2
        stats = report.paired_statistics(fixture)
        self.assertEqual(stats["pairs"][1]["native_captured_returned_text"], 2)
        ET.fromstring(report.ratio_chart(stats))
        for case in fixture["cases"]:
            case["results"][1].update(family="goose", returned_text_boundary_known=False)
        stats = report.paired_statistics(fixture)
        self.assertIsNone(stats["pairs"][1]["native_captured_returned_text"])
        self.assertIsNone(stats["pairs"][1]["input_plus_captured_returned_text_ratio"])

    def test_dsh_policies_are_separate_and_never_win_the_pooled_frontier(self):
        fixture = FrontierContract.fixture()
        for name, group, cost in (("observed", "dsh-fs-observed", 3), ("bare", "dsh-fs-bare", 1)):
            row = dict(fixture["cases"][0]["results"][4], backend=name, family="dsh",
                frontier_group=group, frontier_eligible=False, returned_text_boundary_known=True)
            row["input_cost"] = {"tokens": cost}
            fixture["cases"][0]["results"].append(row)
            fixture["backends"].append({"name": name, "family": "dsh"})
        pooled = report.frontier_statistics(fixture)["tasks"][0]["native"]["guarded"]
        self.assertEqual(pooled["best_input"]["backend"], "native-input")
        groups = {item["policy_group"]: item for item in report.policy_frontiers(fixture)}
        self.assertEqual(groups["dsh-fs-observed"]["tasks"][0]["native"]["guarded"]["best_input"]["backend"], "observed")
        self.assertEqual(groups["dsh-fs-bare"]["tasks"][0]["native"]["guarded"]["best_input"]["backend"], "bare")
        self.assertNotIn("bare", groups["dsh-fs-observed"]["tasks"][0]["native"]["guarded"]["eligible_profiles"])


class PublicDisclosureContract(unittest.TestCase):
    def test_display_redaction_preserves_costs_inputs_and_original_hashes(self):
        from report_privacy import public_report
        home = r"C:\Users\private-person"
        original = {"metadata":{"executable":home+r"\AppData\tool.exe",
            "escaped":json.dumps({"cwd":home})},
            "cases":[{"files":{"a":"literal bytes\r\n"}, "expected":{"a":"changed"},
                "edits":[{"file":"a","old":"literal","text":"changed"}],
                "results":[{"calls":[{"input":{"file":"/workspace/a","old":"literal","text":"changed"},
                    "returned_text":"native output"}],"input_cost":{"tokens":42}}]}]}
        before=json.dumps(original,sort_keys=True)
        public=public_report(original,homes=[home])
        self.assertEqual(json.dumps(original,sort_keys=True),before)
        self.assertEqual(public["cases"],original["cases"])
        self.assertEqual(public["metadata"]["executable"],r"<HOME>\AppData\tool.exe")
        self.assertNotIn("private-person",json.dumps(public))
        ledger=public["disclosure_redaction"]
        self.assertEqual(ledger["changed_strings"],2)
        field=next(row for row in ledger["entries"] if row["json_pointer"]=="/metadata/executable")
        self.assertEqual(field["original_sha256"],hashlib.sha256(original["metadata"]["executable"].encode()).hexdigest())
        self.assertEqual(field["original_utf8_bytes"],len(original["metadata"]["executable"].encode()))
        self.assertFalse(ledger["original_values_retained"])

    def test_home_in_fixture_or_actual_tool_input_fails_instead_of_rewriting_payload(self):
        from report_privacy import public_report
        home="/home/private-person"
        cases=[{"cases":[{"files":{"a":home}}]},
               {"probes":[{"edits":[{"old":home,"text":"new"}]}]},
               {"scenarios":[{"serializations":[{"payload":home}]}]},
               {"calls":[{"input":{"file":home+"/a"}}]},
               {"raw_calls":[{"native_input":{"path":home+"/a"}}]},
               {"native_result":{"arguments":{"old_string":home}}}]
        for value in cases:
            with self.subTest(shape=list(value)),self.assertRaisesRegex(ValueError,"protected fixture/tool input"):
                public_report(value,homes=[home])

    def test_metadata_keys_are_redacted_without_collision_or_original_key_disclosure(self):
        from report_privacy import public_report
        value={"metadata":{"/home/private-person/a":"metadata value"}}
        result=public_report(value,homes=["/home/private-person"])
        self.assertEqual(result["metadata"],{"<HOME>/a":"metadata value"})
        self.assertNotIn("private-person",json.dumps(result))
        with self.assertRaisesRegex(ValueError,"collide"):
            public_report({"metadata":{"/home/private-person/a":1,"<HOME>/a":2}},
                          homes=["/home/private-person"])

    def test_sessions_are_outside_actual_homes_and_cleaned_after_use(self):
        import report_privacy
        homes=[Path(value).resolve() for value in report_privacy.home_values() if value]
        with report_privacy.fixture_session("contract-") as temporary:
            directory=Path(temporary).resolve()
            self.assertFalse(any(directory.is_relative_to(home) for home in homes))
            self.assertTrue(directory.is_dir())
        self.assertFalse(directory.exists())

    def test_preferred_repository_root_wins_only_when_outside_home(self):
        import report_privacy
        with tempfile.TemporaryDirectory() as temporary:
            base=Path(temporary); home=base/"home"
            preferred=base/"repo/target/native-bench/sessions"
            outside=base/"outside-temp"
            with report_privacy.fixture_session("safe-",homes=[home],preferred_root=preferred,
                    temporary_roots=[outside],configured_root="") as created:
                self.assertEqual(Path(created).parent,preferred.resolve())
            self.assertFalse(outside.exists())

    def test_checkout_and_system_temp_inside_home_use_actual_external_io(self):
        import report_privacy
        with tempfile.TemporaryDirectory() as temporary:
            base=Path(temporary); home=base/"home"; home.mkdir()
            preferred=home/"work/repo/target/native-bench/sessions"
            unsafe_temp=home/"work/_temp"
            outside=base/"outside-temp"
            with mock.patch.object(report_privacy,"system_temp_roots",return_value=[unsafe_temp,outside]):
                with report_privacy.fixture_session("runner-",homes=[home],preferred_root=preferred,
                        configured_root="") as created:
                    directory=Path(created)
                    self.assertEqual(directory.parent,outside.resolve())
                    file=directory/"fixture.txt"; file.write_bytes(b"original\r\nEOF")
                    payload={"calls":[{"input":{"file":str(file),"old":"original","text":"edited"}}]}
                    public=report_privacy.public_report(payload,homes=[home])
                    self.assertEqual(public["calls"],payload["calls"])
                    self.assertEqual(file.read_bytes(),b"original\r\nEOF")
            self.assertFalse(preferred.exists())
            self.assertFalse(unsafe_temp.exists())

    def test_home_userprofile_and_path_home_are_all_checked(self):
        import report_privacy
        with tempfile.TemporaryDirectory() as temporary:
            base=Path(temporary)
            with mock.patch.object(Path,"home",return_value=base/"path-home"), mock.patch.dict(os.environ,{
                    "HOME":str(base/"env-home"),"USERPROFILE":str(base/"profile"),
                    "XNIP_BENCH_SESSION_ROOT":""}):
                with report_privacy.fixture_session("union-",
                        preferred_root=base/"profile/repo/sessions",
                        temporary_roots=[base/"env-home/temp",base/"safe-temp"]) as created:
                    self.assertEqual(Path(created).parent,(base/"safe-temp").resolve())

    def test_no_safe_or_writable_candidate_fails_without_creating_home_directories(self):
        import report_privacy
        with tempfile.TemporaryDirectory() as temporary:
            base=Path(temporary); home=base/"home"; home.mkdir()
            with self.assertRaisesRegex(ValueError,"no writable benchmark session directory"):
                report_privacy.fixture_session("blocked-",homes=[home],
                    preferred_root=home/"repo/sessions",temporary_roots=[home/"tmp"],configured_root="")
            self.assertEqual(list(home.iterdir()),[])
            blocked=base/"not-a-directory"; blocked.write_bytes(b"not a writable directory")
            with self.assertRaisesRegex(ValueError,"no writable benchmark session directory"):
                report_privacy.fixture_session("blocked-",homes=[home],
                    preferred_root=blocked,temporary_roots=[blocked],configured_root="")

    def test_configured_session_root_is_explicit_and_never_falls_back_if_unsafe(self):
        import report_privacy
        with tempfile.TemporaryDirectory() as temporary:
            base=Path(temporary); home=base/"home"
            with report_privacy.fixture_session("configured-",homes=[home],
                    preferred_root=base/"preferred",temporary_roots=[base/"tmp"],
                    configured_root=base/"configured") as created:
                self.assertEqual(Path(created).parent,(base/"configured").resolve())
            with self.assertRaisesRegex(ValueError,"configured XNIP_BENCH_SESSION_ROOT"):
                report_privacy.fixture_session("unsafe-",homes=[home],
                    preferred_root=base/"preferred",temporary_roots=[base/"tmp"],
                    configured_root=home/"unsafe")
            self.assertFalse((base/"preferred").exists())
            self.assertFalse((base/"tmp").exists())
            self.assertFalse((home/"unsafe").exists())

    def test_resolved_alias_and_home_substring_collision_choose_another_root(self):
        import report_privacy
        with tempfile.TemporaryDirectory() as temporary:
            base=Path(temporary); home=base/"home"
            for home_value,preferred in [(home,base/"public/../home/repo"),
                    (home,base/"home-backup/repo"),("/home/runner",base/"nested/home/runner-copy/repo")]:
                with report_privacy.fixture_session("alias-",homes=[home_value],preferred_root=preferred,
                        temporary_roots=[base/"outside"],configured_root="") as created:
                    self.assertEqual(Path(created).parent,(base/"outside").resolve())


    def test_unusable_system_temp_lookup_still_keeps_platform_fallback(self):
        import report_privacy
        with mock.patch.object(tempfile,"gettempdir",side_effect=FileNotFoundError("no usable temp")):
            roots=report_privacy.system_temp_roots()
        if os.name=="posix":
            self.assertEqual(roots,[Path("/tmp")])
        elif os.name=="nt":
            windows=os.environ.get("SystemRoot") or os.environ.get("WINDIR")
            self.assertEqual(roots,[Path(windows)/"Temp"] if windows else [])

    def test_protected_error_reports_safe_pointer_without_original_home_value(self):
        from report_privacy import public_report
        home="/home/private-person"
        with self.assertRaises(ValueError) as caught:
            public_report({"cases":[{"results":[{"calls":[{"input":{"file":home+"/private.txt"}}]}]}]},
                          homes=[home])
        message=str(caught.exception)
        self.assertIn("/cases/0/results/0/calls/0/input/file",message)
        self.assertNotIn(home,message)
        self.assertNotIn("private-person",message)

if __name__ == "__main__":
    unittest.main()
