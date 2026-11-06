#!/usr/bin/env python3
"""Actual Google ADK MCP tools beside a direct MCP session; zero model calls."""
from __future__ import annotations

import argparse
import asyncio
import hashlib
import importlib.metadata
import json
import math
import os
from pathlib import Path
import platform
import shutil
import statistics
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests"))
from google_adk_runtime import fixture
from edit_cost import scenarios as original_scenarios
from report_privacy import fixture_session, public_report


def encoded(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def file_info(data):
    data = data.encode() if isinstance(data, str) else data
    return {"bytes": len(data), "sha256": sha(data)}


def stats(values):
    values = sorted(values)
    return {"runs": len(values), "median": statistics.median(values), "p95": values[math.ceil(len(values) * .95) - 1],
            "min": values[0], "max": values[-1]}


def cases():
    result = []
    for case in original_scenarios():
        call = ("xnip_edit", case.edits[0], None) if len(case.edits) == 1 else ("xnip_apply", {"edits": case.edits}, None)
        result.append({"name": case.name, "cohort": "original13", "files": case.files, "expected": case.expected(), "calls": [call]})

    def one(name, before, old, text, expected, error=None, **extra):
        return {"name": name, "cohort": "adk-mcp", "files": {"example.txt": before},
                "expected": {"example.txt": expected},
                "calls": [("xnip_edit", {"file": "example.txt", "old": old, "text": text, **extra}, error)]}

    missing = {"code": -32600, "contains": "old text was not found"}
    ambiguous = {"code": -32600, "contains": "old text is ambiguous"}
    write_guard = {"code": -32600, "contains": "precondition failed"}
    result += [
        one("bom-crlf", "\ufeffhead\r\nold\r\n尾巴", "old", "new", "\ufeffhead\r\nnew\r\n尾巴"),
        one("mixed-eol", "head\r\nold\nend\r\n", "old", "new", "head\r\nnew\nend\r\n"),
        one("noop", "same\n", "same", "same", "same\n"),
        one("reject-missing", "old\n", "missing", "new", "old\n", missing),
        one("reject-ambiguous", "old old\n", "old", "new", "old old\n", ambiguous),
        one("reject-stale-write", "current\n", "stale\n", "new\n", "current\n", write_guard, operation="write"),
        one("preview", "old\r\n", "old", "new", "old\r\n", dry_run=True),
        {"name": "write-create", "cohort": "adk-mcp", "files": {}, "expected": {"created.txt": "new\r\n尾巴"},
         "calls": [("xnip_edit", {"file": "created.txt", "operation": "create", "text": "new\r\n尾巴"}, None)]},
        {"name": "reject-late-batch", "cohort": "adk-mcp", "files": {"a.txt": "old\n", "b.txt": "old\n"},
         "expected": {"a.txt": "old\n", "b.txt": "old\n"},
         "calls": [("xnip_apply", {"edits": [{"file": "a.txt", "old": "old", "text": "new"},
                                               {"file": "b.txt", "old": "missing", "text": "new"}]}, missing)]},
        {"name": "create-rename-delete", "cohort": "adk-mcp", "files": {},
         "expected": {"nested/final.txt": "created\r\n"}, "calls": [("xnip_apply", {
             "format": "json", "sequential": True, "manifest_text": json.dumps([
                 {"op": "create", "file": "nested/new.txt", "text": "created\r\n"},
                 {"op": "rename", "file": "nested/new.txt", "to-file": "nested/final.txt"},
                 {"op": "create", "file": "remove.txt", "text": "temporary"},
                 {"op": "delete", "file": "remove.txt"}])}, None)]},
    ]
    for size in (64 * 1024, 1024 * 1024, 5 * 1024 * 1024):
        tail = "\nmode=old\n"
        before = "x" * (size - len(tail)) + tail
        result.append(one(f"large-{size}", before, "mode=old", "mode=new", before.replace("mode=old", "mode=new")))
    return result


def sources():
    selected = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", ROOT / "integrations/catalog.json",
                Path(__file__), ROOT / "benches/edit_cost.py", ROOT / "benches/report_privacy.py",
                ROOT / "tests/google_adk_runtime.py", ROOT / "benches/native/google_adk_setup.py",
                ROOT / "benches/native/google_adk_requirements.in", ROOT / "benches/native/google_adk_requirements.txt",
                ROOT / "integrations/google-adk/pyproject.toml", ROOT / "integrations/google-adk/xnip_google_adk.py"]
    selected.extend((ROOT / "src").rglob("*.rs"))
    hashes = {path.relative_to(ROOT).as_posix(): sha(path.read_bytes()) for path in sorted(selected)}
    return {"sha256": sha(encoded(hashes)), "files_sha256": hashes}


def validate_call(call, expected):
    assert len(call["rpc"]) == 1, "Tool call requires one real underlying MCP outcome"
    actual = call["rpc"][0]
    assert actual["name"] == call["name"] and actual["arguments"] == call["arguments"]
    assert "infrastructure_error" not in actual
    if expected is None:
        assert "rpc_error" not in actual and "result" in actual
        assert not actual["result"].get("isError", False)
        assert call["projection"] is not None and "error" not in call["projection"]
        assert call["exception"] is None
    else:
        assert actual["rpc_error"]["code"] == expected["code"]
        assert expected["contains"] in actual["rpc_error"]["message"]
        assert call["exception"] or (isinstance(call["projection"], dict) and "error" in call["projection"])
    return True


async def sample(active, case, cwd):
    for path in list(cwd.iterdir()):
        shutil.rmtree(path) if path.is_dir() else path.unlink()
    for name, content in case["files"].items():
        path = cwd / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content.encode())
    captured = []

    async def call(phase, name, arguments, error):
        started = time.perf_counter()
        outcome = await active.call(name, arguments)
        elapsed_ms = (time.perf_counter() - started) * 1000
        item = {"phase": phase, "name": name, "arguments": arguments, **outcome, "elapsed_ms": elapsed_ms,
                "argument_bytes": len(encoded(arguments)), "projection_bytes": len(encoded(outcome["projection"])),
                "exception_bytes": len(encoded(outcome["exception"])) if outcome["exception"] else 0,
                "rpc_bytes": len(encoded(outcome["rpc"]))}
        validate_call(item, error)
        captured.append(item)

    for name in case["files"]:
        await call("read", "xnip_peek", {"file": name, "all": True, "max_bytes": 512}, None)
    for name, arguments, error in case["calls"]:
        await call("mutation", name, arguments, error)
    actual = {path.relative_to(cwd).as_posix(): path.read_bytes() for path in cwd.rglob("*") if path.is_file()}
    outcomes = {name: {"expected": file_info(content), "actual": file_info(actual[name]) if name in actual else None,
                       "byte_exact": actual.get(name) == content.encode()}
                for name, content in case["expected"].items()}
    unexpected = sorted(set(actual) - set(case["expected"]))
    exact = not unexpected and all(value["byte_exact"] for value in outcomes.values())
    assert exact and active.model_calls == 0
    return {"calls": captured, "outcomes": outcomes, "unexpected_files": unexpected,
            "byte_exact": exact, "correct": exact, "model_calls": active.model_calls,
            "tool_calls": len(captured), "tool_ms": sum(item["elapsed_ms"] for item in captured),
            "read_ms": sum(item["elapsed_ms"] for item in captured if item["phase"] == "read"),
            "mutation_ms": sum(item["elapsed_ms"] for item in captured if item["phase"] == "mutation"),
            **{key: sum(item[key] for item in captured) for key in ("argument_bytes", "projection_bytes", "exception_bytes", "rpc_bytes")}}


def validate(report):
    assert len(report["results"]) == len(report["scenarios"]) * 2
    assert len({(row["provider"], row["scenario"]) for row in report["results"]}) == len(report["results"])
    for row in report["results"]:
        assert row["provider"] in {"direct-mcp", "google-adk"}
        case = next(case for case in report["scenarios"] if case["name"] == row["scenario"])
        assert len(row["samples"]) == report["methodology"]["repeats"]
        for item in row["samples"]:
            wanted = [("read", "xnip_peek", {"file": name, "all": True, "max_bytes": 512}, None) for name in case["read_files"]]
            wanted.extend(("mutation", call["name"], call["arguments"], call["error"]) for call in case["mutations"])
            assert item["tool_calls"] == len(wanted) == len(item["calls"])
            for actual, (phase, name, arguments, error) in zip(item["calls"], wanted):
                assert (actual["phase"], actual["name"], actual["arguments"]) == (phase, name, arguments)
                validate_call(actual, error)
                assert actual["argument_bytes"] == len(encoded(arguments))
                assert actual["projection_bytes"] == len(encoded(actual["projection"]))
                assert actual["exception_bytes"] == (len(encoded(actual["exception"])) if actual["exception"] else 0)
                assert actual["rpc_bytes"] == len(encoded(actual["rpc"]))
            assert set(item["outcomes"]) == set(case["expected"])
            for name, outcome in item["outcomes"].items():
                assert outcome["expected"] == case["expected"][name]
                assert outcome["byte_exact"] == (outcome["expected"] == outcome["actual"])
            assert item["byte_exact"] == (not item["unexpected_files"] and all(v["byte_exact"] for v in item["outcomes"].values()))
            assert item["correct"] == item["byte_exact"] and item["correct"]
            assert item["model_calls"] == 0
            for metric in ("tool_ms", "argument_bytes", "projection_bytes", "exception_bytes", "rpc_bytes"):
                field = "elapsed_ms" if metric == "tool_ms" else metric
                assert item[metric] == sum(call[field] for call in item["calls"])
        assert row["correct_runs"] == sum(item["correct"] for item in row["samples"])
        for metric in row["summary"]:
            assert row["summary"][metric] == stats([item[metric] for item in row["samples"]])
    for adk in (row for row in report["results"] if row["provider"] == "google-adk"):
        direct = next(row for row in report["results"] if row["provider"] == "direct-mcp" and row["scenario"] == adk["scenario"])
        for one, two in zip(adk["samples"], direct["samples"]):
            assert one["outcomes"] == two["outcomes"]
            assert [call["rpc"] for call in one["calls"]] == [call["rpc"] for call in two["calls"]]
    return report


async def run(executable, repeats=3, warmups=1, names=None):
    executable = Path(executable).resolve()
    binary = sha(executable.read_bytes())
    source = sources()
    selected = [case for case in cases() if names is None or case["name"] in names]
    assert selected and repeats > 0 and warmups >= 0
    sdk_sources = {}
    for name in ("google-adk", "mcp"):
        distribution = importlib.metadata.distribution(name)
        for path in distribution.files:
            if str(path).endswith(".py"):
                sdk_sources[str(path)] = sha(distribution.locate_file(path).read_bytes())
    report = {"host": "google-adk", "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "methodology": {"repeats": repeats, "warmups": warmups,
          "boundary": "Actual public McpTool.run_async, confirmation/authentication/projection, beside direct MCP ClientSession",
          "model_boundary": "Actual idle LlmAgent and ToolContext; BaseLlm.generate_content_async counter stays zero",
          "rpc_observation": "ClientSession.call_tool spy delegates original method and records actual results/typed remote errors; transport errors fail sampling",
          "projection_boundary": "ADK removes MCP 2 resultType and handles remote errors through its actual error projection; raw RPC values remain comparable"},
        "provenance": {"binary": {"sha256": binary}, "source": source, "python": platform.python_version(),
          "platform": platform.platform(), "sdk_versions": {name: importlib.metadata.version(name) for name in ("google-adk", "mcp")},
          "sdk_source_sha256": sdk_sources},
        "scenarios": [{"name": case["name"], "cohort": case["cohort"], "read_files": list(case["files"]),
          "mutations": [{"name": name, "arguments": arguments, "error": error} for name, arguments, error in case["calls"]],
          "expected": {name: file_info(value) for name, value in case["expected"].items()}} for case in selected],
        "setup": [], "results": []}
    with fixture_session("google-adk-bench-") as temporary:
        cwd = Path(temporary)
        for host in ("direct-mcp", "google-adk"):
            started = time.perf_counter()
            async with fixture(cwd, executable, host=host) as runtime:
                setup = {"provider": host, "setup_ms": (time.perf_counter() - started) * 1000,
                         "tools": sorted(runtime.schemas), "raw_schema_sha256": sha(encoded(runtime.schemas))}
                if host == "google-adk":
                    setup["declarations"] = runtime.declarations
                    setup["declaration_bytes"] = len(encoded(runtime.declarations))
                report["setup"].append(setup)
                for case in selected:
                    measured, warmed = [], []
                    for index in range(warmups + repeats):
                        result = await sample(runtime, case, cwd)
                        (warmed if index < warmups else measured).append(result)
                    report["results"].append({"provider": host, "scenario": case["name"], "samples": measured, "warmups": warmed,
                        "correct_runs": sum(item["correct"] for item in measured),
                        "summary": {key: stats([item[key] for item in measured]) for key in
                          ("tool_ms", "read_ms", "mutation_ms", "tool_calls", "argument_bytes", "projection_bytes", "exception_bytes", "rpc_bytes")}})
    assert sha(executable.read_bytes()) == binary and sources() == source
    for name in ("google-adk", "mcp"):
        distribution = importlib.metadata.distribution(name)
        for path in distribution.files:
            if str(path).endswith(".py"):
                assert sha(distribution.locate_file(path).read_bytes()) == sdk_sources[str(path)]
    return validate(report)


def render(report):
    lines = ["# Google ADK MCP toolset benchmark", "",
        f"Google ADK {report['provenance']['sdk_versions']['google-adk']}; {len(report['scenarios'])} fixtures, "
        f"{len(report['results'])} groups and {sum(len(row['samples']) for row in report['results'])} measured workflows. Zero model calls.", "",
        "Both providers call the same xnip MCP process with the same public arguments. The direct lane uses ClientSession; the ADK lane uses its actual public McpToolset, prefixed-tool registry, idle LlmAgent/ToolContext and McpTool.run_async. The public pre-model metadata path produces the actual ADK function declarations without requesting model output. ADK confirmation, authentication hooks and result/error projection belong to the SDK. The production integration supplies the public toolset rather than replacing builtin file tools.", "",
        "One real ClientSession observer delegates the original method in both lanes and retains each actual result or typed remote error. Successful reads must precede the requested mutation, and rejected cases require their specific remote code/message. Startup/transport/process failures fail the experiment. The resulting file set and independent byte hashes must match each fixture, and both lanes must have identical raw RPC outcomes. ADK's projected response remains separate: MCP 2 resultType is removed, and remote errors use its actual SDK error handling.", "",
        "Timing includes each complete public tool call. Fixture reset, serialized-byte accounting and file checks run outside it. Setup includes SDK construction/discovery, and the first lane includes cold import costs. Each provider keeps one persistent session. Actual JSON payload bytes, rather than model billing, are recorded. Warm-up counts are retained in the JSON methodology; with three measured samples nearest-rank p95 is the maximum.", "",
        f"Binary SHA-256 `{report['provenance']['binary']['sha256']}`; source `{report['provenance']['source']['sha256']}`. "
        f"Python {report['provenance']['python']}; MCP {report['provenance']['sdk_versions']['mcp']}.", "",
        "| Fixture | Direct mutation median / p95 ms | ADK mutation median / p95 ms | Direct / ADK projected JSON bytes | Direct / ADK correct |",
        "|---|---:|---:|---:|---:|"]
    for case in report["scenarios"]:
        rows = [next(row for row in report["results"] if row["provider"] == provider and row["scenario"] == case["name"])
                for provider in ("direct-mcp", "google-adk")]
        values = [f"{row['summary']['mutation_ms']['median']:.2f} / {row['summary']['mutation_ms']['p95']:.2f}" for row in rows]
        projected = [row["summary"]["projection_bytes"]["median"] for row in rows]
        lines.append(f"| {case['name']} | {values[0]} | {values[1]} | {projected[0]:,.0f} / {projected[1]:,.0f} | "
                     f"{rows[0]['correct_runs']}/{len(rows[0]['samples'])} / {rows[1]['correct_runs']}/{len(rows[1]['samples'])} |")
    lines += ["", "## Reproduce", "", "```sh", "python benches/native/google_adk_setup.py",
              "target/native-hosts/google-adk-2.13.0/.venv/bin/python benches/google_adk_provider.py --executable target/release/xnip --repeats 3 --warmups 1", "```", "",
              "[SDK integration and lifecycle](../google-adk.md). The raw JSON retains actual arguments/projections/RPC errors, independent file goals, SDK source hashes and setup costs.", ""]
    return "\n".join(lines)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, default=ROOT / "target/release/xnip")
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--scenarios")
    parser.add_argument("--output-prefix", type=Path, default=ROOT / "docs/benchmarks/google-adk-provider-results")
    args = parser.parse_args()
    report = asyncio.run(run(args.executable, args.repeats, args.warmups, args.scenarios.split(",") if args.scenarios else None))
    report = public_report(report)
    args.output_prefix.parent.mkdir(parents=True, exist_ok=True)
    args.output_prefix.with_suffix(".json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    args.output_prefix.with_suffix(".md").write_text(render(report))
    print(json.dumps({"groups": len(report["results"]), "samples": sum(len(row["samples"]) for row in report["results"])}))
