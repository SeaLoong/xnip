#!/usr/bin/env python3
"""Actual Aider scripting replies on original and xnip-backed IO; zero models."""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
BOOTSTRAP_STARTED = time.perf_counter()
sys.path.insert(0, str(ROOT / "tests"))
sys.path.insert(0, str(ROOT / "benches"))
from aider_native_runtime import AiderFixture, BINARY, diff_response
from aider_native_contract import response_for, lf
from edit_cost import scenarios
BOOTSTRAP_MS = (time.perf_counter() - BOOTSTRAP_STARTED) * 1000


def digest(value):
    return hashlib.sha256(value).hexdigest()


def file_info(value):
    return {"bytes": len(value), "sha256": digest(value)}


def machine_provenance():
    processor = platform.processor()
    cpuinfo = Path("/proc/cpuinfo")
    if cpuinfo.exists():
        processor = next((line.split(":", 1)[1].strip() for line in cpuinfo.read_text().splitlines()
                          if line.startswith("model name")), processor)
    return {"platform": platform.platform(), "architecture": platform.machine(),
            "cpu": processor, "logical_cpus": os.cpu_count()}


def stats(values):
    ordered = sorted(values)
    return {"runs": len(ordered), "min": ordered[0], "median": statistics.median(ordered),
            "p95": ordered[math.ceil(len(ordered) * .95) - 1], "max": ordered[-1]}


def fixture_definitions():
    cases = [{"name": case.name, "cohort": "original13", "files": case.files,
              "goal": case.expected(), "desired": case.expected(), "purpose": case.purpose}
             for case in scenarios()]
    one = lambda name, before, goal, **kwargs: {"name": name, "cohort": "aider-extension",
        "files": {"example.txt": before}, "goal": {"example.txt": goal},
        "desired": {"example.txt": goal}, **kwargs}
    cases.extend([
        {"name": "aider-create", "cohort": "aider-extension", "files": {},
         "response_files": {"created.txt": ""}, "desired": {"created.txt": "created\n"},
         "goal": {"created.txt": "created\n"}, "purpose": "official new-file confirmation and IO write"},
        one("aider-whole-overwrite", "old\n", "new\n", purpose="complete supplied replacement"),
        one("aider-noop", "same\n", "same\n", purpose="official format no-op handling"),
        one("aider-reject-missing", "old\n", "old\n", formats=["diff", "udiff"],
            search="missing\n", replacement="new\n", error=True,
            purpose="original match failure preserves storage"),
        {"name": "aider-approval-denied", "cohort": "aider-extension",
         "files": {"chat.txt": "chat\n", "other.txt": "old\n"},
         "response_files": {"other.txt": "old\n"}, "desired": {"other.txt": "new\n"},
         "goal": {"chat.txt": "chat\n", "other.txt": "old\n"},
         "chat_files": ["chat.txt"], "yes": False,
         "purpose": "official confirmation declines an existing non-chat target"},
        one("aider-dry-run", "old\n", "old\n", desired={"example.txt": "new\n"}, dry_run=True,
            purpose="original SDK dry-run retains supplied file bytes"),
        one("aider-latin1", "caf\xe9\nold\n", "caf\xe9\nnew\n", encoding="latin-1",
            purpose="host-selected legacy encoding reaches core as exact bytes"),
        one("aider-crlf-output", "head\nold\nend\n", "head\r\nnew\r\nend\r\n",
            line_endings="crlf", purpose="explicit original IO CRLF output policy"),
    ])
    for size in [65536, 1048576, 5242880]:
        end = "\nmode=old\n"
        before = "x" * (size - len(end)) + end
        cases.append(one(f"aider-large-{size}", before, before.replace("mode=old", "mode=new"),
                         cohort="aider-large", purpose="full supplied format representation and guarded persistence"))
    return cases


def source_provenance():
    files = [ROOT / name for name in ["Cargo.toml", "Cargo.lock", "integrations/catalog.json",
        "benches/aider_provider.py", "benches/edit_cost.py", "tests/aider_native_runtime.py",
        "tests/aider_native_contract.py", "benches/native/aider_native_setup.py",
        "benches/native/aider_requirements.in", "benches/native/aider_requirements.txt"]]
    for directory in ["src", "integrations/aider-native", "integrations/native-python", "benches/native/aider_native"]:
        files.extend(path for path in (ROOT / directory).rglob("*") if path.is_file()
                     and path.suffix in (".rs", ".py", ".toml", ".json", ".txt"))
    hashes = {str(path.relative_to(ROOT)): digest(path.read_bytes()) for path in sorted(set(files))}
    return {"sha256": digest(json.dumps(hashes, sort_keys=True).encode()), "files_sha256": hashes}


def stored_files(cwd):
    ignored = {".aider-test-model.json", ".aider-test-analytics.json", ".aider-test-anchor"}
    return sorted(str(path.relative_to(cwd)) for path in cwd.rglob("*")
                  if path.is_file() and str(path.relative_to(cwd)) not in ignored)


def encoded_files(values, encoding):
    return {name: value.encode(encoding) for name, value in values.items()}


def host_goal(case, edit_format):
    if case.get("error") or case.get("dry_run") or case.get("yes") is False or case["name"] == "aider-noop":
        return encoded_files(case["goal"], case.get("encoding", "utf-8"))
    result = {}
    for name, text in case["goal"].items():
        normalized = lf(text)
        if normalized and not normalized.endswith("\n"):
            normalized += "\n"
        newline = "\r\n" if case.get("line_endings") == "crlf" else "\n"
        result[name] = normalized.replace("\n", newline).encode(case.get("encoding", "utf-8"))
    return result


def format_response(runtime, case, edit_format):
    before = case.get("response_files", case["files"])
    if "search" in case:
        name = runtime.coder.get_rel_fname(str(runtime.cwd / next(iter(before))))
        if edit_format == "diff":
            return diff_response(name, case["search"], case["replacement"])
        fence = "`" * 3
        return f"{fence}diff\n--- {name}\n+++ {name}\n@@\n-{case['search']}+{case['replacement']}{fence}\n"
    return response_for(runtime, before, case["desired"], edit_format)


def reset_files(cwd, case):
    for name in stored_files(cwd):
        (cwd / name).unlink()
    for name, content in encoded_files(case["files"], case.get("encoding", "utf-8")).items():
        path = cwd / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)


def bridge_snapshot(runtime):
    client = runtime.session.io._client if runtime.session else None
    if client is None:
        return {"request_bytes": 0, "response_bytes": 0, "actions": {}}
    client.drain()
    return {"request_bytes": client.request_bytes, "response_bytes": client.response_bytes,
            "actions": dict(client.actions)}


def sample(runtime, case, edit_format, message):
    before = bridge_snapshot(runtime)
    started = time.perf_counter()
    result = runtime.apply(message)
    elapsed_ms = (time.perf_counter() - started) * 1000
    after = bridge_snapshot(runtime)
    bridge = {key: after[key] - before[key] for key in ["request_bytes", "response_bytes"]}
    bridge["total_bytes"] = sum(bridge.values())
    bridge["actions"] = dict(Counter(after["actions"]) - Counter(before["actions"]))
    goals = encoded_files(case["goal"], case.get("encoding", "utf-8"))
    expected = host_goal(case, edit_format)
    actual = {name: (runtime.cwd / name).read_bytes() for name in expected}
    outcomes = {name: {"expected": file_info(content), "goal": file_info(goals[name]),
        "actual": file_info(actual[name]), "byte_exact": content == actual[name],
        "goal_exact": goals[name] == actual[name]} for name, content in expected.items()}
    unexpected = sorted(set(stored_files(runtime.cwd)) - set(expected))
    returned = json.dumps(result, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
    is_error = bool(result["errors"])
    outcome_matches = is_error == case.get("error", False)
    byte_exact = not unexpected and all(item["byte_exact"] for item in outcomes.values())
    return {"edit_ms": elapsed_ms, "input_bytes": len(message.encode()), "input_sha256": digest(message.encode()),
        "result_bytes": len(returned), "result_sha256": digest(returned), "bridge": bridge,
        "is_error": is_error, "outcome_matches": outcome_matches, "byte_exact": byte_exact,
        "goal_exact": not unexpected and all(item["goal_exact"] for item in outcomes.values()),
        "correct": outcome_matches and byte_exact, "outcomes": outcomes, "unexpected_files": unexpected,
        "model_calls": runtime.model_call_count, "result": result}


def validate_report(report):
    assert report["schema_version"] == 1
    assert report["methodology"]["measured_runs"] > 0
    expected_rows = sum(len(case["formats"]) * 2 for case in report["scenarios"])
    assert len(report["results"]) == expected_rows
    identities = [(row["provider"], row["scenario"], row["format"]) for row in report["results"]]
    assert len(set(identities)) == len(identities)
    for row in report["results"]:
        assert row["provider"] in ["aider-original", "xnip-native"]
        scenario = next(case for case in report["scenarios"] if case["name"] == row["scenario"])
        assert row["format"] in scenario["formats"]
        assert len(row["samples"]) == report["methodology"]["measured_runs"]
        for value in row["samples"]:
            assert value["model_calls"] == 0
            assert value["outcome_matches"] == (value["is_error"] == scenario["expected_error"])
            assert sorted(value["outcomes"]) == sorted(scenario["expected"][row["format"]]), "complete file oracle"
            for name, outcome in value["outcomes"].items():
                assert outcome["expected"] == scenario["expected"][row["format"]][name]
                assert outcome["goal"] == scenario["goal"][name]
                assert outcome["byte_exact"] == (outcome["expected"] == outcome["actual"])
                assert outcome["goal_exact"] == (outcome["goal"] == outcome["actual"])
            assert value["byte_exact"] == (not value["unexpected_files"] and all(x["byte_exact"] for x in value["outcomes"].values()))
            assert value["goal_exact"] == (not value["unexpected_files"] and all(x["goal_exact"] for x in value["outcomes"].values()))
            assert value["correct"] == (value["byte_exact"] and value["outcome_matches"])
            assert value["bridge"]["total_bytes"] == value["bridge"]["request_bytes"] + value["bridge"]["response_bytes"]
            assert value["correct"], (row["scenario"], row["format"], value)
        for field in ["edit_ms", "input_bytes", "result_bytes"]:
            assert row["summary"][field] == stats([sample[field] for sample in row["samples"]])
        assert row["correct_runs"] == sum(value["correct"] for value in row["samples"])
        assert row["goal_exact_runs"] == sum(value["goal_exact"] for value in row["samples"])
        if row["provider"] == "xnip-native":
            original = next(item for item in report["results"] if item["provider"] == "aider-original"
                and item["scenario"] == row["scenario"] and item["format"] == row["format"])
            catalogs = [item["catalog_sha256"] for item in report["setup"]
                        if item["scenario"] == row["scenario"] and item["format"] == row["format"]]
            assert len(catalogs) == 2 and len(set(catalogs)) == 1, "same actual Coder and raw prompt templates"
            for value, baseline in zip(row["samples"], original["samples"]):
                for field in ["input_sha256", "result_sha256", "outcomes", "is_error"]:
                    assert value[field] == baseline[field], (row["scenario"], row["format"], field)
    return report


def run_benchmark(executable=BINARY, repeats=3, warmups=1, selected=None, formats=("diff", "whole", "udiff")):
    assert repeats > 0 and warmups >= 0
    assert formats and set(formats).issubset({"diff", "whole", "udiff"})
    executable = Path(executable).resolve()
    binary = executable.read_bytes()
    source = source_provenance()
    all_cases = fixture_definitions()
    cases = [case for case in all_cases if (selected is None or case["name"] in selected)
             and set(formats).intersection(case.get("formats", formats))]
    assert cases and (selected is None or set(selected) == {case["name"] for case in cases})
    receipt = json.loads((ROOT / "target/native-hosts/aider-5dc9490/native-host.json").read_text())
    report = {"schema_version": 1, "provenance": {"binary": {"path": str(executable), "sha256": digest(binary)},
        "source": source, "host": receipt, "python": sys.version, "machine": machine_provenance()},
        "bootstrap_ms": BOOTSTRAP_MS, "methodology": {"measured_runs": repeats, "warmups": warmups,
        "boundary": "real Coder.create/apply_updates: parser, dry-run matcher, authorization and original/native IO",
        "formats": list(formats), "model_calls": 0}, "scenarios": [], "setup": [], "results": []}
    for case in cases:
        chosen = [format for format in formats if format in case.get("formats", formats)]
        report["scenarios"].append({"name": case["name"], "cohort": case["cohort"], "purpose": case.get("purpose"),
            "formats": chosen, "expected_error": case.get("error", False),
            "expected": {fmt: {name: file_info(value) for name, value in host_goal(case, fmt).items()} for fmt in chosen},
            "goal": {name: file_info(value) for name, value in encoded_files(case["goal"], case.get("encoding", "utf-8")).items()}})
    with tempfile.TemporaryDirectory(dir=ROOT / "target", prefix="aider-provider-") as folder:
        cwd = Path(folder)
        for native in [False, True]:
            provider = "xnip-native" if native else "aider-original"
            for case in cases:
                for edit_format in formats:
                    if edit_format not in case.get("formats", formats): continue
                    reset_files(cwd, case)
                    started = time.perf_counter()
                    runtime = AiderFixture(cwd, native=native, edit_format=edit_format, executable=executable,
                        files=case.get("chat_files", case["files"]), yes=case.get("yes", True),
                        encoding=case.get("encoding", "utf-8"), line_endings=case.get("line_endings", "lf"),
                        dry_run=case.get("dry_run", False))
                    setup_ms = (time.perf_counter() - started) * 1000
                    try:
                        message = format_response(runtime, case, edit_format)
                        catalog = {"class": type(runtime.coder).__module__ + "." + type(runtime.coder).__name__,
                            "edit_format": runtime.coder.edit_format,
                            "prompt_templates": {name: getattr(runtime.coder.gpt_prompts, name)
                                for name in dir(runtime.coder.gpt_prompts) if not name.startswith("_")
                                and isinstance(getattr(runtime.coder.gpt_prompts, name), (str, list, dict))}}
                        report["setup"].append({"provider": provider, "scenario": case["name"], "format": edit_format,
                            "setup_ms": setup_ms, "catalog_sha256": digest(json.dumps(catalog, sort_keys=True).encode())})
                        values, warmed = [], []
                        initial_chat = set(runtime.coder.abs_fnames)
                        for index in range(warmups + repeats):
                            reset_files(cwd, case)
                            # Restore the initial chat through Aider's public
                            # command seam after removing each workflow's files.
                            # A newly authorized target otherwise stays in chat
                            # while fixture reset has removed it from storage.
                            for filename in set(runtime.coder.abs_fnames) - initial_chat:
                                runtime.coder.drop_rel_fname(filename)
                            result = sample(runtime, case, edit_format, message)
                            (warmed if index < warmups else values).append(result)
                        report["results"].append({"provider": provider, "scenario": case["name"], "format": edit_format,
                            "correct_runs": sum(value["correct"] for value in values),
                            "goal_exact_runs": sum(value["goal_exact"] for value in values),
                            "summary": {field: stats([value[field] for value in values]) for field in ["edit_ms", "input_bytes", "result_bytes"]},
                            "samples": values, "warmups": warmed})
                    finally:
                        runtime.close()
    assert executable.read_bytes() == binary, "measured binary stayed fixed"
    assert source_provenance() == source, "measured source stayed fixed"
    return validate_report(report)


def render_report(report):
    measured = sum(len(row["samples"]) for row in report["results"])
    lines = ["# Aider scripting IO provider benchmark", "",
        f"Pinned commit `{report['provenance']['host']['commit']}`; {len(report['scenarios'])} fixtures across diff/whole/udiff, {len(report['results'])} groups and {measured} measured workflows. Zero model completions.", "",
        "The real Coder is created through its scripting IO parameter. Every measured reply enters its original apply_updates pipeline: native parser, dry-run matching, file authorization and final format executor. The original lane writes through InputOutput; the native lane delegates its reading/presentation/confirmation and persists text through xnip core. Both lanes retain identical format classes, raw prompt templates, supplied reply bytes and actual apply_updates receipts/diagnostics.", "",
        "Aider describes this Python scripting API as version-sensitive. This cohort pins its source revision and full hashed Python dependencies; it measures this local scripting seam. SDK construction, source/model metadata setup, fixture reset, reply generation and file validation stay outside edit timing. Complete SEARCH/REPLACE or whole-file representations and three-line unified diffs are supplied from known fixture contents. Interactive user/model latency and final agent-facing message assembly have their own boundaries.", "",
        "The host-selected LF policy and each format's explicit trailing-newline representation are declared separately from the original literal byte goal. Both independent file oracles and original/native result parity are retained. Aider's initial empty-file creation stays in its authorization pipeline. Git commit scheduling remains under Aider; benchmarks select auto_commits=False and use_git=False. Model.send_completion and the actual LiteLLM completion method are trapped and counted at zero.", "",
        f"Executable SHA-256 `{report['provenance']['binary']['sha256']}`; source `{report['provenance']['source']['sha256']}`. One warm-up and three measured workflows per group by default; nearest-rank p95 with three samples is the maximum.", "",
        "| Fixture | Format | Original median / p95 ms | Native median / p95 ms | Input bytes | Native IPC bytes | Original / native host-goal exact |", "|---|---|---:|---:|---:|---:|---:|"]
    for scenario in report["scenarios"]:
        for fmt in scenario["formats"]:
            original = next(row for row in report["results"] if row["provider"] == "aider-original" and row["scenario"] == scenario["name"] and row["format"] == fmt)
            native = next(row for row in report["results"] if row["provider"] == "xnip-native" and row["scenario"] == scenario["name"] and row["format"] == fmt)
            first, second = original["summary"]["edit_ms"], native["summary"]["edit_ms"]
            traffic = stats([value["bridge"]["total_bytes"] for value in native["samples"]])["median"]
            count = report["methodology"]["measured_runs"]
            lines.append(f"| {scenario['name']} | {fmt} | {first['median']:.2f} / {first['p95']:.2f} | {second['median']:.2f} / {second['p95']:.2f} | {native['summary']['input_bytes']['median']:,.0f} | {traffic:,.0f} | {original['correct_runs']}/{count} / {native['correct_runs']}/{count} |")
    lines.extend(["", "## Reproduce", "", "```sh", "python3.13 benches/native/aider_native_setup.py",
        "target/native-hosts/aider-5dc9490/aider-venv/bin/python benches/aider_provider.py --executable target/release/xnip",
        "```", "", "See [SDK setup and restoration](../aider-native.md) and the JSON companion for per-sample receipts, exact input/output/file hashes, SDK sources, dependencies, initialization costs and bridge counters."])
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, default=BINARY)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--scenarios", help="comma-separated fixture names")
    parser.add_argument("--formats", default="diff,whole,udiff")
    parser.add_argument("--output-prefix", type=Path, default=ROOT / "docs/benchmarks/aider-provider-results")
    args = parser.parse_args()
    report = run_benchmark(args.executable, args.repeats, args.warmups,
                           args.scenarios.split(",") if args.scenarios else None, args.formats.split(","))
    args.output_prefix.parent.mkdir(parents=True, exist_ok=True)
    args.output_prefix.with_suffix(".json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    args.output_prefix.with_suffix(".md").write_text(render_report(report))
    print(json.dumps({"scenarios": len(report["scenarios"]), "groups": len(report["results"]),
                      "samples": sum(len(row["samples"]) for row in report["results"]), "model_calls": 0}))
