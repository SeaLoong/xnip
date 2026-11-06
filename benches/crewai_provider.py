#!/usr/bin/env python3
"""Actual CrewAI BaseTool calls beside MCP sessions with matching lifetimes."""
from __future__ import annotations

import argparse
import asyncio
from contextlib import contextmanager
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
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests"))
from crewai_runtime import fixture
from host_profiles import fixtures
from report_privacy import fixture_session, public_report
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from mcp.shared.exceptions import McpError
import mcp.client.stdio


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
    for case in fixtures():
        edits = case.get("edits", [])
        calls = []
        if edits:
            calls.append(("xnip_edit", edits[0], case.get("error")) if len(edits) == 1
                         else ("xnip_apply", {"edits": edits}, case.get("error")))
        calls.extend(("xnip_edit", arguments, None) for arguments in case.get("operations", []))
        result.append({**case, "calls": calls})
    for name, before in [("bom-crlf", "\ufeffhead\r\nold\r\n尾巴"), ("mixed-eol", "head\r\nold\nend\r\n")]:
        result.append({"name": name, "cohort": "crewai-mcp", "files": {"example.txt": before},
                       "expected": {"example.txt": before.replace("old", "new")},
                       "calls": [("xnip_edit", {"file": "example.txt", "old": "old", "text": "new"}, None)]})
    result.append({"name": "preview", "cohort": "crewai-mcp", "files": {"example.txt": "old\r\n"},
                   "expected": {"example.txt": "old\r\n"},
                   "calls": [("xnip_edit", {"file": "example.txt", "old": "old", "text": "new", "dry_run": True}, None)]})
    for size in (64 * 1024, 1024 * 1024, 5 * 1024 * 1024):
        tail = "\nmode=old\n"
        before = "x" * (size - len(tail)) + tail
        result.append({"name": f"large-{size}", "cohort": "crewai-mcp", "files": {"example.txt": before},
                       "expected": {"example.txt": before.replace("mode=old", "mode=new")},
                       "calls": [("xnip_edit", {"file": "example.txt", "old": "mode=old", "text": "mode=new"}, None)]})
    return result


def sources():
    names = ["Cargo.toml", "Cargo.lock", "integrations/catalog.json", "benches/crewai_provider.py",
             "benches/host_profiles.py", "benches/edit_cost.py", "benches/report_privacy.py",
             "tests/crewai_runtime.py", "benches/native/crewai_setup.py", "benches/native/crewai_requirements.in",
             "benches/native/crewai_requirements.txt", "integrations/crewai/pyproject.toml",
             "integrations/crewai/xnip_crewai.py", "tests/host_profiles_runtime.py",
             "integrations/host-profiles/xnip_profiles.py", "benches/native/host_profiles_setup.py"]
    names.extend(path.relative_to(ROOT).as_posix() for path in (ROOT / "src").rglob("*.rs"))
    hashes = {name: sha((ROOT / name).read_bytes()) for name in sorted(names)}
    return {"sha256": sha(encoded(hashes)), "files_sha256": hashes}


class Direct:
    model_calls = 0

    def __init__(self, cwd, executable):
        self.parameters = StdioServerParameters(command=str(executable),
            args=["mcp", "--cwd", str(cwd), "--receipts", "text"],
            cwd=str(cwd), env={**os.environ, "XNIP_AUTO_UPDATE": "0"})
        self.schemas = asyncio.run(self.discover())

    async def discover(self):
        async with stdio_client(self.parameters) as streams:
            async with ClientSession(*streams) as session:
                await session.initialize()
                return {tool.name: tool.inputSchema for tool in (await session.list_tools()).tools}

    async def invoke(self, name, arguments):
        entry = {"name": name, "arguments": arguments}
        async with stdio_client(self.parameters) as streams:
            async with ClientSession(*streams) as session:
                await session.initialize()
                try:
                    reply = await session.call_tool(name, arguments)
                    entry["result"] = reply.model_dump(mode="json", exclude_none=True)
                    projection = "\n".join(block.text for block in reply.content if block.type == "text")
                    exception = None
                except McpError as error:
                    entry["rpc_error"] = error.error.model_dump(mode="json", exclude_none=True)
                    projection, exception = None, {"type": type(error).__name__, "message": str(error)}
        return {"projection": projection, "exception": exception, "rpc": [entry]}

    def call(self, name, arguments):
        return asyncio.run(self.invoke(name, arguments))


@contextmanager
def runtime(host, cwd, executable):
    if host == "direct-mcp":
        processes = []
        spawn = mcp.client.stdio._create_platform_compatible_process

        async def trace_spawn(*args, **kwargs):
            process = await spawn(*args, **kwargs)
            processes.append(process)
            return process

        with patch.object(mcp.client.stdio, "_create_platform_compatible_process", trace_spawn):
            yield Direct(cwd, executable)
        assert processes and all(process.returncode == 0 for process in processes)
    else:
        with fixture(cwd, executable) as sdk:
            class Runtime:
                schemas = {name: tool.args_schema.model_json_schema() for name, tool in sdk.tools.items()}

                @property
                def model_calls(self):
                    return sdk.model.calls

                def call(self, name, arguments):
                    before = len(sdk.calls)
                    try:
                        projection, _ = sdk.call(name, arguments)
                        exception = None
                    except Exception as error:
                        projection, exception = None, {"type": type(error).__name__, "message": str(error)}
                    return {"projection": projection, "exception": exception, "rpc": sdk.calls[before:]}

            yield Runtime()


def validate_call(call, expected):
    assert call["rpc"], "Actual tool invocation needs its remote MCP outcome"
    for actual in call["rpc"]:
        assert actual["name"] == call["name"] and actual["arguments"] == call["arguments"]
        assert "infrastructure_error" not in actual
        if expected is None:
            assert "rpc_error" not in actual and "result" in actual
            assert not actual["result"].get("isError", False)
        else:
            assert actual["rpc_error"]["code"] == expected["code"]
            assert expected["contains"] in actual["rpc_error"]["message"]
    if expected is None:
        assert len(call["rpc"]) == 1 and call["exception"] is None
        assert isinstance(call["projection"], str)
    else:
        assert call["exception"] and expected["contains"] in call["exception"]["message"]
        assert call["projection"] is None
    return True


def sample(active, case, cwd):
    for path in list(cwd.iterdir()):
        shutil.rmtree(path) if path.is_dir() else path.unlink()
    for name, content in case["files"].items():
        path = cwd / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content.encode())
    captured = []

    def call(phase, name, arguments, error):
        started = time.perf_counter()
        outcome = active.call(name, arguments)
        item = {"phase": phase, "name": name, "arguments": arguments, **outcome,
                "elapsed_ms": (time.perf_counter() - started) * 1000,
                "argument_bytes": len(encoded(arguments)), "projection_bytes": len(encoded(outcome["projection"])),
                "exception_bytes": len(encoded(outcome["exception"])) if outcome["exception"] else 0,
                "rpc_bytes": len(encoded(outcome["rpc"])), "rpc_attempts": len(outcome["rpc"])}
        validate_call(item, error)
        captured.append(item)

    for name in case["files"]:
        call("read", "xnip_peek", {"file": name, "all": True, "max_bytes": 512}, None)
    for name, arguments, error in case["calls"]:
        call("mutation", name, arguments, error)
    actual = {path.relative_to(cwd).as_posix(): path.read_bytes() for path in cwd.rglob("*") if path.is_file()}
    outcomes = {name: {"expected": file_info(content), "actual": file_info(actual[name]) if name in actual else None,
                       "byte_exact": actual.get(name) == content.encode()} for name, content in case["expected"].items()}
    unexpected = sorted(set(actual) - set(case["expected"]))
    exact = not unexpected and all(value["byte_exact"] for value in outcomes.values())
    assert exact and active.model_calls == 0
    return {"calls": captured, "outcomes": outcomes, "unexpected_files": unexpected,
            "byte_exact": exact, "correct": exact, "model_calls": active.model_calls,
            "tool_calls": len(captured), "tool_ms": sum(item["elapsed_ms"] for item in captured),
            "read_ms": sum(item["elapsed_ms"] for item in captured if item["phase"] == "read"),
            "mutation_ms": sum(item["elapsed_ms"] for item in captured if item["phase"] == "mutation"),
            **{key: sum(item[key] for item in captured) for key in
               ("argument_bytes", "projection_bytes", "exception_bytes", "rpc_bytes", "rpc_attempts")}}


def validate(report):
    assert len(report["results"]) == len(report["scenarios"]) * 2
    assert len({(row["provider"], row["scenario"]) for row in report["results"]}) == len(report["results"])
    for row in report["results"]:
        assert row["provider"] in {"direct-mcp", "crewai"}
        case = next(case for case in report["scenarios"] if case["name"] == row["scenario"])
        assert len(row["samples"]) == report["methodology"]["repeats"]
        assert len(row["warmups"]) == report["methodology"]["warmups"]
        for item in row["samples"] + row["warmups"]:
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
                assert actual["rpc_attempts"] == len(actual["rpc"])
            assert set(item["outcomes"]) == set(case["expected"])
            for name, outcome in item["outcomes"].items():
                assert outcome["expected"] == case["expected"][name]
                assert outcome["byte_exact"] == (outcome["expected"] == outcome["actual"])
            assert item["byte_exact"] == (not item["unexpected_files"] and all(v["byte_exact"] for v in item["outcomes"].values()))
            assert item["correct"] == item["byte_exact"] and item["correct"] and item["model_calls"] == 0
            for metric in ("tool_ms", "argument_bytes", "projection_bytes", "exception_bytes", "rpc_bytes", "rpc_attempts"):
                field = "elapsed_ms" if metric == "tool_ms" else metric
                assert item[metric] == sum(call[field] for call in item["calls"])
        assert row["correct_runs"] == sum(item["correct"] for item in row["samples"])
        for metric in row["summary"]:
            assert row["summary"][metric] == stats([item[metric] for item in row["samples"]])
    for sdk in (row for row in report["results"] if row["provider"] == "crewai"):
        direct = next(row for row in report["results"] if row["provider"] == "direct-mcp" and row["scenario"] == sdk["scenario"])
        for one, two in zip(sdk["samples"], direct["samples"]):
            assert one["outcomes"] == two["outcomes"]
            assert [call["rpc"][-1] for call in one["calls"]] == [call["rpc"][-1] for call in two["calls"]]
    return report


def run(executable, repeats=3, warmups=1, names=None):
    executable = Path(executable).resolve()
    binary, source = sha(executable.read_bytes()), sources()
    selected = [case for case in cases() if names is None or case["name"] in names]
    assert selected and repeats > 0 and warmups >= 0
    sdk_sources = {}
    for name in ("crewai", "mcp"):
        distribution = importlib.metadata.distribution(name)
        for path in distribution.files:
            if str(path).endswith(".py"):
                sdk_sources[str(path)] = sha(distribution.locate_file(path).read_bytes())
    report = {"host": "crewai", "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "methodology": {"repeats": repeats, "warmups": warmups,
            "boundary": "Actual public CrewAI BaseTool.run and fresh-per-call MCP connections beside direct ClientSession with the same connection lifecycle",
            "model_boundary": "Actual Agent and registered MCPNativeTools; BaseLLM call/acall counters stay zero",
            "retry_boundary": "CrewAI owns retry policy; every actual attempt is retained, and each rejected attempt needs its specific remote code/message",
            "rpc_observation": "Observer delegates actual ClientSession.call_tool; startup/transport/process errors fail sampling"},
        "provenance": {"binary": {"sha256": binary}, "source": source, "python": platform.python_version(),
            "platform": platform.platform(), "sdk_versions": {name: importlib.metadata.version(name) for name in ("crewai", "mcp")},
            "sdk_source_sha256": sdk_sources},
        "scenarios": [{"name": case["name"], "cohort": case["cohort"], "read_files": list(case["files"]),
            "mutations": [{"name": name, "arguments": arguments, "error": error} for name, arguments, error in case["calls"]],
            "expected": {name: file_info(value) for name, value in case["expected"].items()}} for case in selected],
        "setup": [], "results": []}
    with fixture_session("crewai-bench-") as temporary:
        cwd = Path(temporary)
        for host in ("direct-mcp", "crewai"):
            started = time.perf_counter()
            with runtime(host, cwd, executable) as active:
                report["setup"].append({"provider": host, "setup_ms": (time.perf_counter() - started) * 1000,
                    "tools": sorted(active.schemas), "schemas": active.schemas})
                for case in selected:
                    measured, warmed = [], []
                    for index in range(warmups + repeats):
                        result = sample(active, case, cwd)
                        (warmed if index < warmups else measured).append(result)
                    report["results"].append({"provider": host, "scenario": case["name"], "samples": measured, "warmups": warmed,
                        "correct_runs": sum(item["correct"] for item in measured),
                        "summary": {key: stats([item[key] for item in measured]) for key in
                            ("tool_ms", "read_ms", "mutation_ms", "tool_calls", "rpc_attempts", "argument_bytes", "projection_bytes", "exception_bytes", "rpc_bytes")}})
    assert sha(executable.read_bytes()) == binary and sources() == source
    for name in ("crewai", "mcp"):
        distribution = importlib.metadata.distribution(name)
        for path in distribution.files:
            if str(path).endswith(".py"):
                assert sha(distribution.locate_file(path).read_bytes()) == sdk_sources[str(path)]
    return validate(report)


def render(report):
    lines = ["# CrewAI MCP tool benchmark", "",
        f"CrewAI {report['provenance']['sdk_versions']['crewai']}; {len(report['scenarios'])} fixtures, "
        f"{len(report['results'])} groups and {sum(len(row['samples']) for row in report['results'])} measured workflows. Zero model calls.", "",
        "The SDK lane uses the actual MCPToolResolver, MCPNativeTool, Agent registry and public BaseTool.run. The direct lane uses public MCP ClientSession. Each invocation starts and closes a new MCP connection in both lanes, matching CrewAI's own native tool lifecycle. Initial discovery/Agent registration is recorded separately. This integration supplies MCP tools through CrewAI's public resolver.", "",
        "Every real MCP outcome is retained through an observer that delegates the original SDK method. Reads must succeed before their requested mutation. Rejected requests require the expected typed remote error and message; startup, transport or SDK failures fail the experiment. CrewAI's retry policy remains active: ambiguous edits may have several real remote attempts and their backoff remains inside timing. All resulting files, absences and byte hashes must match the independent fixture oracle. Direct and CrewAI final RPC outcomes must agree.", "",
        "Timing includes complete public tool calls, fresh process creation, initialize, SDK argument/result processing, retry policy and connection closure. Fixture reset, byte accounting and file validation run outside it. Counts describe serialized JSON payloads and projected tool text rather than model billing. Each warm-up is retained; with three measured samples, nearest-rank p95 is the maximum.", "",
        f"Binary SHA-256 `{report['provenance']['binary']['sha256']}`; source `{report['provenance']['source']['sha256']}`. "
        f"Python {report['provenance']['python']}; MCP {report['provenance']['sdk_versions']['mcp']}.", "",
        "| Fixture | Direct mutation median / p95 ms | CrewAI mutation median / p95 ms | Direct / CrewAI RPC attempts | Direct / CrewAI correct |",
        "|---|---:|---:|---:|---:|"]
    for case in report["scenarios"]:
        rows = [next(row for row in report["results"] if row["provider"] == provider and row["scenario"] == case["name"])
                for provider in ("direct-mcp", "crewai")]
        values = [f"{row['summary']['mutation_ms']['median']:.2f} / {row['summary']['mutation_ms']['p95']:.2f}" for row in rows]
        attempts = [row["summary"]["rpc_attempts"]["median"] for row in rows]
        lines.append(f"| {case['name']} | {values[0]} | {values[1]} | {attempts[0]:g} / {attempts[1]:g} | "
                     f"{rows[0]['correct_runs']}/{len(rows[0]['samples'])} / {rows[1]['correct_runs']}/{len(rows[1]['samples'])} |")
    lines += ["", "## Reproduce", "", "```sh", "python benches/native/crewai_setup.py",
        "target/native-hosts/crewai-1.15.36/.venv/bin/python benches/crewai_provider.py --executable target/release/xnip --repeats 3 --warmups 1", "```", "",
        "[SDK integration, cache and lifecycle](../crewai.md). Raw JSON includes actual arguments, projections, typed errors, per-attempt results, SDK source hashes, setup and independent file goals.", ""]
    return "\n".join(lines)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, default=ROOT / "target/release/xnip")
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--scenarios")
    parser.add_argument("--output-prefix", type=Path, default=ROOT / "docs/benchmarks/crewai-provider-results")
    args = parser.parse_args()
    report = public_report(run(args.executable, args.repeats, args.warmups, args.scenarios.split(",") if args.scenarios else None))
    args.output_prefix.parent.mkdir(parents=True, exist_ok=True)
    args.output_prefix.with_suffix(".json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    args.output_prefix.with_suffix(".md").write_text(render(report))
