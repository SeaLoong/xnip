#!/usr/bin/env python3
"""Shared stdio MCP and actual Goose-client measurements for generated profiles.

GUI profiles share the explicit stdio lane. That lane measures the configured
server launch and file tools, while Goose additionally uses its official client.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests"))
from host_profiles_runtime import ProfileMcp, module, profiles, setup, verify_cli, verify_continue
from edit_cost import scenarios
from report_privacy import fixture_session, public_report


def dump(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


def sha(value):
    return hashlib.sha256(value).hexdigest()


def info(content):
    data = content.encode() if isinstance(content, str) else content
    return {"bytes": len(data), "sha256": sha(data)}


def stats(values):
    ordered = sorted(values)
    return {"runs": len(values), "median": statistics.median(values),
            "p95": ordered[math.ceil(len(values) * .95) - 1], "min": ordered[0], "max": ordered[-1]}


def fixtures():
    original = [{"name": item.name, "cohort": "original13", "files": item.files,
                 "edits": item.edits, "expected": item.expected(), "purpose": item.purpose}
                for item in scenarios()]
    return [*original,
        {"name": "noop", "cohort": "mcp-extension", "files": {"a.txt": "same\r\n"},
         "edits": [{"file": "a.txt", "old": "same", "text": "same"}],
         "expected": {"a.txt": "same\r\n"}, "purpose": "ordinary unchanged receipt"},
        {"name": "reject-stale", "cohort": "mcp-extension", "files": {"a.txt": "external\r\n"},
         "edits": [{"file": "a.txt", "old": "old", "text": "new"}],
         "error": {"code": -32600, "contains": "old text was not found"},
         "expected": {"a.txt": "external\r\n"}, "purpose": "an old-content guard retains newer bytes"},
        {"name": "reject-ambiguous", "cohort": "mcp-extension", "files": {"a.txt": "old old\n"},
         "edits": [{"file": "a.txt", "old": "old", "text": "new"}],
         "error": {"code": -32600, "contains": "old text is ambiguous"},
         "expected": {"a.txt": "old old\n"}, "purpose": "ambiguous literal preserves storage"},
        {"name": "write-create", "cohort": "mcp-extension", "files": {},
         "operations": [{"file": "nested/created.txt", "operation": "create", "text": "\ufeffα\r\n尾巴"}],
         "expected": {"nested/created.txt": "\ufeffα\r\n尾巴"}, "purpose": "complete-file lifecycle through the configured MCP tool"},
        {"name": "delete", "cohort": "mcp-extension", "files": {"a.txt": "old\r\n"},
         "operations": [{"file": "a.txt", "operation": "delete"}], "expected": {},
         "purpose": "file deletion and complete absence oracle"},
        {"name": "rename", "cohort": "mcp-extension", "files": {"a.txt": "\ufeffold\r\n"},
         "operations": [{"file": "a.txt", "operation": "rename", "to": "nested/moved.txt"}],
         "expected": {"nested/moved.txt": "\ufeffold\r\n"}, "purpose": "rename preserves bytes and removes the original path"},
        {"name": "reject-late-batch", "cohort": "mcp-extension", "files": {"a.txt": "old\n", "b.txt": "old\n"},
         "edits": [{"file": "a.txt", "old": "old", "text": "new"}, {"file": "b.txt", "old": "missing", "text": "new"}],
         "error": {"code": -32600, "contains": "old text was not found"},
         "expected": {"a.txt": "old\n", "b.txt": "old\n"},
         "purpose": "later planning failure preserves every earlier target"},
    ]


def source_provenance():
    names = ["Cargo.toml", "Cargo.lock", "integrations/catalog.json", "benches/host_profiles.py",
             "benches/edit_cost.py", "benches/report_privacy.py", "tests/host_profiles_runtime.py",
             "integrations/host-profiles/xnip_profiles.py", "benches/native/host_profiles_setup.py",
             "benches/native/host_profiles_clis/package.json", "benches/native/host_profiles_clis/package-lock.json",
             "benches/native/host_profiles_sources.json", "benches/native/goose_sources.json",
             "benches/native/goose_adapter.py", "benches/native/goose_adapter/Cargo.lock"]
    names += ["benches/native/goose_adapter/Cargo.toml", "benches/native/goose_adapter/build.rs"]
    names += [p.relative_to(ROOT).as_posix() for p in (ROOT / "src").rglob("*.rs")]
    names += [p.relative_to(ROOT).as_posix() for p in (ROOT / "benches/native/goose_adapter/src").rglob("*.rs")]
    files = {name: sha((ROOT / name).read_bytes()) for name in sorted(names)}
    return {"sha256": sha(dump(files).encode()), "files_sha256": files}


def calls_for(case, prefix):
    result = [("read", {"name": "xnip_peek", "arguments": {"file": prefix + "/" + name, "all": True, "max_bytes": 0}})
              for name in case["files"]]
    changes = [{**edit, "file": prefix + "/" + edit["file"]} for edit in case.get("edits", [])]
    if changes:
        result.append(("mutation", {"name": "xnip_edit", "arguments": changes[0]}) if len(changes) == 1
                      else ("mutation", {"name": "xnip_apply", "arguments": {"edits": changes}}))
    for operation in case.get("operations", []):
        arguments = {**operation, "file": prefix + "/" + operation["file"]}
        if "to" in arguments:
            arguments["to"] = prefix + "/" + arguments["to"]
        result.append(("mutation", {"name": "xnip_edit", "arguments": arguments}))
    return result


class GooseClient:
    def __init__(self, metadata):
        self.process = subprocess.Popen([metadata["binary"]], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, text=True, encoding="utf-8", bufsize=1)

    def bridge(self, configuration, cwd, calls):
        request = {"action": "bridge", "root": str(cwd), "command": configuration["command"],
                   "arguments": configuration["args"], "calls": calls, "capture_tool_errors": True}
        self.process.stdin.write(dump(request) + "\n")
        self.process.stdin.flush()
        return json.loads(self.process.stdout.readline())

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=15)
        self.process.stdout.close()
        self.process.stderr.close()
        if self.process.returncode:
            raise RuntimeError(f"Goose client fixture exited {self.process.returncode}")


def validate_calls(planned, captured, expected_error):
    """A rejected edit needs an actual remote response to its exact request."""
    assert len(captured) == len(planned), "Every planned call must have an actual client result"
    failures = []
    for wanted, item in zip(planned, captured):
        assert {"name": item["name"], "arguments": item["arguments"]} == wanted
        assert item["phase"] in {"read", "mutation"}
        if item["error"]:
            failures.append(item)
        else:
            assert item["result"] is not None and not item["result"].get("isError", False)
    if expected_error is None:
        assert not failures
    else:
        assert len(failures) == 1 and failures[0]["phase"] == "mutation"
        error = failures[0]["error"]
        assert error["code"] == expected_error["code"]
        assert expected_error["contains"] in error["message"]
    return True


def reset_case(project, case, prefix):
    import shutil
    directory = project / prefix
    shutil.rmtree(directory, ignore_errors=True)
    directory.mkdir(parents=True)
    for name, content in case["files"].items():
        file = directory / name
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_bytes(content.encode())
    return directory


def sample(lane, configuration, other, goose, project, case, prefix):
    directory = reset_case(project, case, prefix)
    calls = calls_for(case, prefix)
    captured = []
    started = time.perf_counter()
    if lane == "shared-stdio-mcp":
        with ProfileMcp(configuration, other) as client:
            catalog = client.catalog
            for phase, call in calls:
                at = time.perf_counter()
                try:
                    reply = client.call(call["name"], call["arguments"])
                    error = None
                except RuntimeError:
                    reply = None
                    error = client.last_response["error"]
                elapsed = (time.perf_counter() - at) * 1000
                captured.append({"phase": phase, **call, "result": reply, "error": error, "tool_ms": elapsed,
                    "request_bytes": client.last_request_bytes,
                    "response_bytes": client.last_response_bytes})
        tool_ms = sum(call["tool_ms"] for call in captured)
    else:
        raw = goose.bridge(configuration, other, [call for _, call in calls])
        if not raw.get("ok"):
            raise RuntimeError("Goose connection/transport failed: " + raw.get("adapter_error", "missing successful result"))
        catalog = raw.get("tools", {}).get("tools")
        assert len(raw["calls"]) == len(calls), "Goose must return each actual tool-call outcome"
        for (phase, call), item in zip(calls, raw.get("calls", [])):
            assert item["native_input"] == call
            captured.append({"phase": phase, **call, "result": item.get("result"), "error": item.get("rpc_error"),
                             "tool_ms": item["elapsed_ms"]})
        tool_ms = sum(call["tool_ms"] for call in captured)
        assert raw["model_calls"] == 0
    workflow_ms = (time.perf_counter() - started) * 1000
    for item in captured:
        item["argument_bytes"] = len(dump(item["arguments"]).encode())
        item["result_json_bytes"] = len(dump(item["result"]).encode()) if item["result"] is not None else 0
        item["error_json_bytes"] = len(dump(item["error"]).encode()) if item["error"] is not None else 0
    counted = {name: sum(item[name] for item in captured)
               for name in ("argument_bytes", "result_json_bytes", "error_json_bytes")}
    counted["tool_json_bytes"] = sum(counted.values())
    expected = case["expected"]
    actual_files = {p.relative_to(directory).as_posix(): p.read_bytes() for p in directory.rglob("*") if p.is_file()}
    outcomes = {name: {"expected": info(content), "actual": info(actual_files[name]),
                       "byte_exact": actual_files[name] == content.encode()}
                for name, content in expected.items()}
    unexpected = sorted(set(actual_files) - set(expected))
    byte_exact = set(actual_files) == set(expected) and all(item["byte_exact"] for item in outcomes.values())
    outcome_matches = validate_calls([call for _, call in calls], captured, case.get("error"))
    assert byte_exact and outcome_matches, (lane, case["name"], outcomes)
    if catalog is not None:
        assert {tool["name"] for tool in catalog} == {"xnip_edit", "xnip_apply", "xnip_peek", "xnip_find", "xnip_recover"}
    return {"workflow_ms": workflow_ms, "tool_ms": tool_ms, **counted,
            "planned_calls": [call for _, call in calls], "calls": captured,
            "outcomes": outcomes, "unexpected_files": unexpected,
            "byte_exact": byte_exact, "outcome_matches": outcome_matches, "correct": byte_exact and outcome_matches,
            "model_calls": 0}


def validate_report(report):
    assert {item["host"] for item in report["profiles"]} == set(profiles.HOST_FILES)
    assert len(report["profiles"]) == len(profiles.HOST_FILES)
    assert all(item["configured_call_passed"] and item["discovered_tools"] == 5 for item in report["profiles"])
    assert len(report["results"]) == len(report["scenarios"]) * 2
    assert len({(row["lane"], row["scenario"]) for row in report["results"]}) == len(report["results"])
    for row in report["results"]:
        case = next(item for item in report["scenarios"] if item["name"] == row["scenario"])
        assert row["lane"] in {"shared-stdio-mcp", "goose-official-client"}
        assert len(row["samples"]) == report["methodology"]["repeats"]
        for item in row["samples"]:
            assert item["model_calls"] == 0
            assert item["tool_ms"] == sum(call["tool_ms"] for call in item["calls"])
            for call in item["calls"]:
                assert call["argument_bytes"] == len(dump(call["arguments"]).encode())
                assert call["result_json_bytes"] == (len(dump(call["result"]).encode()) if call["result"] is not None else 0)
                assert call["error_json_bytes"] == (len(dump(call["error"]).encode()) if call["error"] is not None else 0)
            for name in ("argument_bytes", "result_json_bytes", "error_json_bytes"):
                assert item[name] == sum(call[name] for call in item["calls"])
            assert item["tool_json_bytes"] == item["argument_bytes"] + item["result_json_bytes"] + item["error_json_bytes"]
            assert set(item["outcomes"]) == set(case["expected"])
            for name, outcome in item["outcomes"].items():
                assert outcome["expected"] == case["expected"][name]
                assert outcome["byte_exact"] == (outcome["actual"] == outcome["expected"])
            assert not item["unexpected_files"]
            assert item["byte_exact"] == all(value["byte_exact"] for value in item["outcomes"].values())
            assert [call["phase"] for call in item["calls"]] == ["read"] * case["read_calls"] + ["mutation"] * case["mutation_calls"]
            assert item["outcome_matches"] == validate_calls(item["planned_calls"], item["calls"], case["expected_error"])
            assert item["correct"] == (item["byte_exact"] and item["outcome_matches"])
            assert item["correct"]
            assert item["planned_calls"]
        assert row["correct_runs"] == sum(item["correct"] for item in row["samples"])
        assert row["workflow_ms"] == stats([item["workflow_ms"] for item in row["samples"]])
        assert row["tool_json_bytes"] == stats([item["tool_json_bytes"] for item in row["samples"]])
        tool_values = [item["tool_ms"] for item in row["samples"] if item["tool_ms"] is not None]
        assert row["tool_ms"] == (stats(tool_values) if tool_values else None)
    return report


def run_benchmark(binary, repeats=3, warmups=1, names=None):
    binary = Path(binary).resolve()
    selected = [case for case in fixtures() if names is None or case["name"] in names]
    assert selected and repeats > 0 and warmups >= 0
    source = source_provenance()
    binary_hash = sha(binary.read_bytes())
    cli = setup.prepare(check=True)
    goose_module = module("profiles_benchmark_goose", ROOT / "benches/native/goose_adapter.py")
    goose_metadata = goose_module.prepare(check=True)
    report = {"schema_version": 1, "methodology": {"repeats": repeats, "warmups": warmups,
        "boundary": "fresh local connection workflow; actual tool-call latency additionally recorded",
        "profile_scope": "GUI entries share a stdio MCP transport lane; only Goose additionally executes its official client",
        "error_scope": "Goose fixture records typed remote MCP errors per call; startup/transport errors fail the run"},
        "provenance": {"binary": {"sha256": binary_hash, "bytes": binary.stat().st_size},
          "source": source, "python": platform.python_version(), "platform": platform.platform(),
          "cli_parsers": cli, "goose": {key: goose_metadata[key] for key in ["commit", "version", "binary_sha256", "dependency_lock_sha256"]}},
        "profiles": [], "scenarios": [{"name": case["name"], "cohort": case["cohort"], "purpose": case["purpose"],
            "read_calls": len(case["files"]), "mutation_calls": len(case.get("operations", [])) or 1,
            "expected_error": case.get("error"), "expected": {name: info(value) for name, value in case["expected"].items()}}
            for case in selected], "results": []}
    with fixture_session("host-profiles-") as temporary:
        base = Path(temporary)
        project, other = base / "project", base / "host-cwd"
        project.mkdir(); other.mkdir()
        generated = profiles.generate(binary, project, base / "profiles")
        for host, entry in generated["profiles"].items():
            configuration = profiles.launch(host, Path(entry["file"]))
            target = project / "profile-probe.txt"
            target.write_bytes(b"old\r\n")
            with ProfileMcp(configuration, other) as client:
                reply = client.call("xnip_edit", {"file": "profile-probe.txt", "old": "old", "text": "new"})
                assert not reply.get("isError", False)
                assert target.read_bytes() == b"new\r\n"
                catalog_sha256 = sha(dump(client.catalog).encode())
            evidence = {"host": host, "profile": profiles.HOST_FILES[host], "benchmark_lane": "shared-stdio-mcp",
                        "catalog_sha256": catalog_sha256, "discovered_tools": len(client.catalog), "configured_call_passed": True}
            if host in setup.VERSIONS:
                evidence["host_configuration_reader"] = verify_cli(host, configuration, base / "readers" / host)
            elif host == "continue":
                evidence["host_configuration_reader"] = verify_continue(entry["file"])
            if host == "goose":
                evidence["additional_benchmark_lane"] = "goose-official-client"
            report["profiles"].append(evidence)
        goose = GooseClient(goose_metadata)
        try:
            for lane, host in [("shared-stdio-mcp", "cursor"), ("goose-official-client", "goose")]:
                configuration = profiles.launch(host, Path(generated["profiles"][host]["file"]))
                for case in selected:
                    samples, warmed = [], []
                    for index in range(warmups + repeats):
                        result = sample(lane, configuration, other, goose, project, case, f"cases/{case['name']}/{index}")
                        (warmed if index < warmups else samples).append(result)
                    tool_values = [item["tool_ms"] for item in samples if item["tool_ms"] is not None]
                    report["results"].append({"lane": lane, "scenario": case["name"], "samples": samples,
                        "warmups": warmed, "correct_runs": sum(item["correct"] for item in samples),
                        "workflow_ms": stats([item["workflow_ms"] for item in samples]),
                        "tool_json_bytes": stats([item["tool_json_bytes"] for item in samples]),
                        "tool_ms": stats(tool_values) if tool_values else None})
        finally:
            goose.close()
    assert sha(binary.read_bytes()) == binary_hash, "Measured binary changed during sampling"
    assert sha(Path(goose_metadata["binary"]).read_bytes()) == goose_metadata["binary_sha256"], "Goose client binary changed during sampling"
    assert source_provenance() == source, "Measured source changed during sampling"
    return validate_report(report)


def render_report(report):
    lines = ["# MCP host profiles and Goose client benchmark", "",
        f"{len(report['profiles'])} generated host profiles; {len(report['scenarios'])} fixtures, "
        f"{len(report['results'])} lane/fixture groups and {sum(len(row['samples']) for row in report['results'])} measured workflows. Zero model calls.", "",
        "The shared stdio lane executes the generated command, negotiates real MCP, discovers the actual xnip catalog and calls its file tools. GUI profiles bind to that shared transport lane; these measurements describe the configured local bridge. Codex, Claude Code and Copilot CLI additionally read isolated configurations through their actual management commands. Continue uses its published configuration schema. Goose additionally executes its pinned official McpClient through the existing source-verified fixture. Agent/model loops and GUI rendering remain separate execution layers.", "",
        "Every sample starts a new MCP connection. Workflow time includes local spawn/connection, discovery, tool calls and graceful disconnect; fixture reset and byte checks are outside timing. Actual client timing is retained for every tool call, including typed remote JSON-RPC rejections. Goose's fixture captures the official client's remote error while retaining every completed prefix call; startup, transport and process failures fail the run. Rejected fixtures require the specific remote error code and guard message. These fields describe different transport boundaries. Warm-up counts come from the raw methodology; with three measured samples nearest-rank p95 is the maximum.", "",
        "Tool JSON bytes count canonical UTF-8 JSON for the actual public arguments and client result/error objects. The shared lane also retains measured JSONL request/response frame bytes. Goose client DTOs and any GUI/model-visible projection retain separate boundaries; these byte counts describe local calls, rather than billing.", "",
        f"Binary SHA-256 `{report['provenance']['binary']['sha256']}`; maintained source `{report['provenance']['source']['sha256']}`. "
        f"Python {report['provenance']['python']}; Goose {report['provenance']['goose']['version']}.", "",
        "| Profile | Configuration evidence | Measured lane |", "|---|---|---|"]
    for item in report["profiles"]:
        level = item.get("host_configuration_reader", {}).get("scope", "generated configuration + actual stdio discovery/edit")
        lane = item["benchmark_lane"] + ("; " + item["additional_benchmark_lane"] if "additional_benchmark_lane" in item else "")
        lines.append(f"| {item['host']} | {level} | {lane} |")
    lines += ["", "| Fixture | Shared workflow median / p95 ms | Goose workflow median / p95 ms | Shared / Goose tool JSON bytes | Shared / Goose correct |",
              "|---|---:|---:|---:|---:|"]
    for case in report["scenarios"]:
        rows = [next(row for row in report["results"] if row["scenario"] == case["name"] and row["lane"] == lane)
                for lane in ("shared-stdio-mcp", "goose-official-client")]
        fields = [f"{row['workflow_ms']['median']:.2f} / {row['workflow_ms']['p95']:.2f}" for row in rows]
        json_bytes = [row['tool_json_bytes']['median'] for row in rows]
        lines.append(f"| {case['name']} | {fields[0]} | {fields[1]} | {json_bytes[0]:,.0f} / {json_bytes[1]:,.0f} | {rows[0]['correct_runs']}/{len(rows[0]['samples'])} / {rows[1]['correct_runs']}/{len(rows[1]['samples'])} |")
    lines += ["", "## Reproduce", "", "```sh", "python benches/native/host_profiles_setup.py",
              "python benches/native/goose_adapter.py --setup", "python tests/host_profiles_contract.py",
              "python benches/host_profiles.py --executable target/release/xnip --repeats 3 --warmups 1", "```", "",
              "[Host setup and restoration](../host-profiles.md). The JSON companion retains actual call inputs/results, schema and binary/source hashes, independent file outcomes, CLI parser evidence and warm-up samples.", ""]
    return "\n".join(lines)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, default=ROOT / "target/release/xnip")
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--scenarios", help="Comma-separated fixture names")
    parser.add_argument("--output-prefix", type=Path, default=ROOT / "docs/benchmarks/host-profiles-results")
    args = parser.parse_args()
    report = run_benchmark(args.executable, args.repeats, args.warmups,
                           args.scenarios.split(",") if args.scenarios else None)
    report = public_report(report)
    args.output_prefix.parent.mkdir(parents=True, exist_ok=True)
    args.output_prefix.with_suffix(".json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    args.output_prefix.with_suffix(".md").write_text(render_report(report))
    print(json.dumps({"groups": len(report["results"]), "samples": sum(len(row["samples"]) for row in report["results"])}))
