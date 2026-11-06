#!/usr/bin/env python3
"""Real SDK→xnip MCP tool pipelines beside direct MCP; zero model calls."""

from __future__ import annotations

import argparse
import asyncio
from contextlib import asynccontextmanager
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
import tempfile
import time

from edit_cost import Mcp, scenarios as original_scenarios

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests"))
HOSTS = ("agents-sdk", "autogen", "smolagents")
REJECTION_REASONS = {
    "reject-missing": "old text was not found",
    "reject-ambiguous": "old text is ambiguous",
    "reject-stale-write": "write content precondition failed",
    "reject-late-batch": "exact replacement target does not exist",
}


def encoded(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(value).hexdigest()


def file_info(value):
    value = value.encode() if isinstance(value, str) else value
    return {"bytes": len(value), "sha256": digest(value)}


def stats(values):
    values = sorted(values)
    return {"runs": len(values), "median": statistics.median(values),
            "p95": values[math.ceil(len(values) * .95) - 1], "min": values[0], "max": values[-1]}


def scenarios():
    cases = []
    for case in original_scenarios():
        tool = "xnip_edit" if len(case.edits) == 1 else "xnip_apply"
        arguments = case.edits[0] if len(case.edits) == 1 else {"edits": case.edits}
        cases.append({"name": case.name, "cohort": "original13", "files": case.files,
                      "expected": case.expected(), "calls": [(tool, arguments, False)]})

    def one(name, before, old, text, expected, error=False, **arguments):
        return {"name": name, "cohort": "mcp-sdk", "files": {"example.txt": before},
                "expected": {"example.txt": expected},
                "calls": [("xnip_edit", {"file": "example.txt", "old": old,
                                         "text": text, **arguments}, error)]}

    cases += [
        one("bom-crlf", "\ufeffhead\r\nold\r\n尾巴", "old", "new", "\ufeffhead\r\nnew\r\n尾巴"),
        one("mixed-eol", "head\r\nold\nend\r\n", "old", "new", "head\r\nnew\nend\r\n"),
        one("noop", "same\n", "same", "same", "same\n"),
        one("reject-missing", "old\n", "missing", "new", "old\n", True),
        one("reject-ambiguous", "old old\n", "old", "new", "old old\n", True),
        one("reject-stale-write", "current\n", "stale\n", "new\n", "current\n", True, operation="write"),
        one("preview", "old\r\n", "old", "new", "old\r\n", dry_run=True),
        {"name": "write-create", "cohort": "mcp-sdk", "files": {},
         "expected": {"created.txt": "new\r\n尾巴"},
         "calls": [("xnip_edit", {"file": "created.txt", "operation": "write", "text": "new\r\n尾巴"}, False)]},
        {"name": "reject-late-batch", "cohort": "mcp-sdk", "files": {"example.txt": "old\n"},
         "expected": {"example.txt": "old\n"}, "calls": [("xnip_apply", {"edits": [
             {"file": "example.txt", "old": "old", "text": "new"},
             {"file": "absent", "old": "missing", "text": "new"}]}, True)]},
        {"name": "create-rename-delete", "cohort": "mcp-sdk", "files": {},
         "expected": {"nested/final.txt": "created\r\n"}, "calls": [("xnip_apply", {
             "format": "json", "sequential": True, "manifest_text": json.dumps([
                 {"op": "create", "file": "nested/new.txt", "text": "created\r\n"},
                 {"op": "rename", "file": "nested/new.txt", "to-file": "nested/final.txt"},
                 {"op": "create", "file": "remove.txt", "text": "temp"},
                 {"op": "delete", "file": "remove.txt"}])}, False)]},
    ]
    for size in (64 * 1024, 1024 * 1024, 5 * 1024 * 1024):
        tail = "\nmode=old\n"
        source = "x" * (size - len(tail)) + tail
        cases.append(one(f"large-{size}", source, "mode=old", "mode=new", source.replace("mode=old", "mode=new")))
    return cases


@asynccontextmanager
async def runtime(host, cwd, executable, direct=False):
    if direct:
        with Mcp(Path(executable), Path(cwd)) as client:
            class Direct:
                model_calls = 0
                schemas = {t["name"]: t["inputSchema"] for t in client.request("tools/list", {})["tools"]}

                async def call(self, name, arguments):
                    result = client.call(name, arguments)
                    return {"projection": result, "is_error": result.get("isError", False) or "jsonrpc_error" in result,
                            "receipt": result.get("structuredContent")}
            yield Direct()
    elif host in ("agents-sdk", "autogen"):
        from agent_sdks_runtime import fixture
        async with fixture(host, cwd, executable) as active:
            yield active
    else:
        from smolagents_contract import fixture
        from smolagents.models import get_tool_json_schema
        from smolagents.utils import AgentToolCallError, AgentToolExecutionError
        with fixture(cwd, executable=executable) as active:
            class Smol:
                schemas = {t.name: get_tool_json_schema(t)["function"]["parameters"]
                           for t in active.collection.tools}

                @property
                def model_calls(self):
                    return active.model_calls

                async def call(self, name, arguments):
                    try:
                        result = active.call(name, arguments)
                    except (AgentToolCallError, AgentToolExecutionError) as error:
                        return {"projection": None, "is_error": True, "receipt": None,
                                "exception": {"type": type(error).__name__, "message": str(error)}}
                    return {"projection": result, "is_error": False,
                            "receipt": result if isinstance(result, dict) else None}
            yield Smol()


def validate_call(call, provider, reason):
    """Check each SDK's actual public result against the requested guard."""
    assert call["is_error"] == call["expected_error"]
    if reason is None:
        assert call["projection"] is not None and call["exception"] is None
        if provider == "direct-mcp":
            assert "jsonrpc_error" not in call["projection"] and not call["projection"].get("isError", False)
        elif provider == "autogen":
            assert call["projection"]["type"] == "ToolResult"
            assert call["projection"]["name"] == call["name"] and not call["projection"]["is_error"]
        return True
    assert call["is_error"]
    if provider == "direct-mcp":
        assert call["exception"] is None
        error = call["projection"]["jsonrpc_error"]
        assert error["code"] == -32600 and reason in error["message"]
    elif provider == "autogen":
        assert call["exception"] is None
        projection = call["projection"]
        assert projection["type"] == "ToolResult" and projection["name"] == call["name"] and projection["is_error"]
        text = "\n".join(block["content"] for block in projection["result"] if block["type"] == "TextResultContent")
        assert reason in text
    else:
        assert call["projection"] is None
        error = call["exception"]
        if provider == "agents-sdk":
            assert error["type"] == "McpError" and reason in error["message"]
        elif provider == "smolagents":
            assert error["type"] in {"AgentToolCallError", "AgentToolExecutionError"}
            _, marker, remote_message = error["message"].partition("McpError: ")
            assert marker and reason in remote_message
        else:
            raise AssertionError(provider)
    return True


async def sample(active, case, cwd, provider):
    for path in sorted(cwd.iterdir()):
        if path.is_dir():
            shutil.rmtree(path)
        else:
            path.unlink()
    for name, content in case["files"].items():
        path = cwd / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content.encode())
    calls = []

    async def call(phase, name, arguments, expected_error=False):
        started = time.perf_counter()
        result = await active.call(name, arguments)
        elapsed = (time.perf_counter() - started) * 1000
        projection = result["projection"]
        reason = REJECTION_REASONS[case["name"]] if expected_error else None
        actual = {"phase": phase, "name": name, "elapsed_ms": elapsed,
                      "arguments": arguments, "argument_bytes": len(encoded(arguments)),
                      "argument_sha256": digest(encoded(arguments)),
                      "projection": projection, "projection_bytes": 0 if projection is None else len(encoded(projection)),
                      "projection_sha256": None if projection is None else digest(encoded(projection)),
                      "exception_bytes": len(result.get("exception", {}).get("message", "").encode()),
                      "is_error": result["is_error"], "expected_error": expected_error,
                      "expected_reason": reason, "exception": result.get("exception")}
        validate_call(actual, provider, reason)
        calls.append(actual)

    for name in case["files"]:
        await call("read", "xnip_peek", {"file": name, "all": True, "max_bytes": 512})
    for name, arguments, error in case["calls"]:
        await call("edit", name, arguments, error)
    actual = {p.relative_to(cwd).as_posix(): p.read_bytes() for p in cwd.rglob("*") if p.is_file()}
    outcomes = {name: {"expected": file_info(content), "actual": file_info(actual[name]) if name in actual else None,
                       "byte_exact": actual.get(name) == content.encode()}
                for name, content in case["expected"].items()}
    unexpected = sorted(set(actual) - set(case["expected"]))
    exact = not unexpected and all(item["byte_exact"] for item in outcomes.values())
    outcome_matches = all(validate_call(item, provider, item["expected_reason"]) for item in calls)
    return {"calls": calls, "tool_calls": len(calls),
            "read_ms": sum(c["elapsed_ms"] for c in calls if c["phase"] == "read"),
            "edit_ms": sum(c["elapsed_ms"] for c in calls if c["phase"] == "edit"),
            "tool_ms": sum(c["elapsed_ms"] for c in calls),
            "argument_bytes": sum(c["argument_bytes"] for c in calls),
            "projection_bytes": sum(c["projection_bytes"] for c in calls),
            "exception_bytes": sum(c["exception_bytes"] for c in calls),
            "outcomes": outcomes, "unexpected_files": unexpected,
            "byte_exact": exact, "outcome_matches": outcome_matches,
            "correct": exact and outcome_matches, "model_calls": active.model_calls}


def source_provenance(host):
    sources = [Path(__file__), ROOT / "benches/edit_cost.py", ROOT / "Cargo.toml", ROOT / "Cargo.lock",
               ROOT / "integrations/catalog.json"]
    sources.extend((ROOT / "src").rglob("*.rs"))
    if host == "smolagents":
        sources += [ROOT / "tests/smolagents_contract.py", ROOT / "benches/native/smolagents_setup.py",
                    ROOT / "benches/native/smolagents_requirements.txt"]
        sources.extend((ROOT / "integrations/smolagents").glob("*.py"))
        sources.append(ROOT / "integrations/smolagents/pyproject.toml")
    else:
        sources += [ROOT / "tests/agent_sdks_runtime.py", ROOT / "benches/native/agent_sdks_setup.py",
                    ROOT / "benches/native/agent_sdks_requirements.txt"]
        project = "agents-sdk" if host == "agents-sdk" else "autogen"
        sources.extend((ROOT / "integrations" / project).glob("*.py"))
        sources.append(ROOT / "integrations" / project / "pyproject.toml")
    hashes = {path.relative_to(ROOT).as_posix(): digest(path.read_bytes()) for path in sorted(set(sources))}
    return {"files_sha256": hashes, "sha256": digest(encoded(hashes))}


def validate(report):
    assert len(report["results"]) == len(report["scenarios"]) * 2
    assert len({(r["provider"], r["scenario"]) for r in report["results"]}) == len(report["results"])
    for row in report["results"]:
        assert row["provider"] in ["direct-mcp", report["host"]]
        case = next(c for c in report["scenarios"] if c["name"] == row["scenario"])
        assert len(row["samples"]) == report["methodology"]["repeats"]
        assert len(row["warmups"]) == report["methodology"]["warmups"]
        for item in row["samples"] + row["warmups"]:
            assert item["model_calls"] == 0
            assert item["tool_calls"] == len(item["calls"])
            expected_calls = [("read", "xnip_peek", {"file": name, "all": True, "max_bytes": 512}, False, None)
                              for name in case["read_files"]]
            expected_calls += [("edit", call["name"], call["arguments"], call["expected_error"],
                                REJECTION_REASONS[case["name"]] if call["expected_error"] else None)
                               for call in case["mutations"]]
            assert [(c["phase"], c["name"], c["arguments"], c["expected_error"], c["expected_reason"])
                    for c in item["calls"]] == expected_calls
            assert set(item["outcomes"]) == set(case["expected"])
            for name, outcome in item["outcomes"].items():
                assert outcome["expected"] == case["expected"][name]
                assert outcome["byte_exact"] == (outcome["expected"] == outcome["actual"])
            assert item["byte_exact"] == (not item["unexpected_files"] and all(o["byte_exact"] for o in item["outcomes"].values()))
            assert item["outcome_matches"] == all(validate_call(c, row["provider"], c["expected_reason"]) for c in item["calls"])
            assert item["correct"] == (item["byte_exact"] and item["outcome_matches"])
            for call in item["calls"]:
                assert math.isfinite(call["elapsed_ms"]) and call["elapsed_ms"] >= 0
                assert call["argument_bytes"] == len(encoded(call["arguments"]))
                assert call["argument_sha256"] == digest(encoded(call["arguments"]))
                assert call["projection_bytes"] == (0 if call["projection"] is None else len(encoded(call["projection"])))
                assert call["projection_sha256"] == (None if call["projection"] is None else digest(encoded(call["projection"])))
                assert call["exception_bytes"] == len((call["exception"] or {}).get("message", "").encode())
            for key in ("read_ms", "edit_ms"):
                phase = "read" if key == "read_ms" else "edit"
                assert abs(item[key] - sum(c["elapsed_ms"] for c in item["calls"] if c["phase"] == phase)) < .0001
            assert abs(item["tool_ms"] - item["read_ms"] - item["edit_ms"]) < .0001
            assert item["argument_bytes"] == sum(c["argument_bytes"] for c in item["calls"])
            assert item["projection_bytes"] == sum(c["projection_bytes"] for c in item["calls"])
            assert item["exception_bytes"] == sum(c["exception_bytes"] for c in item["calls"])
            assert item["correct"], (row["scenario"], item)
        assert row["correct_runs"] == sum(s["correct"] for s in row["samples"])
        for measure, value in row["summary"].items():
            assert value == stats([s[measure] for s in row["samples"]])
    for row in [r for r in report["results"] if r["provider"] == report["host"]]:
        direct = next(r for r in report["results"] if r["provider"] == "direct-mcp" and r["scenario"] == row["scenario"])
        for a, b in zip(row["samples"], direct["samples"], strict=True):
            assert a["outcomes"] == b["outcomes"]
            assert [(c["name"], c["argument_sha256"], c["is_error"]) for c in a["calls"]] == [
                (c["name"], c["argument_sha256"], c["is_error"]) for c in b["calls"]]


async def run(host, executable, repeats=3, warmups=1, names=None):
    executable = Path(executable).resolve()
    binary_hash = digest(executable.read_bytes())
    source = source_provenance(host)
    selected = [c for c in scenarios() if names is None or c["name"] in names]
    assert selected
    distributions = ["smolagents", "mcpadapt", "mcp"] if host == "smolagents" else ["openai-agents", "autogen-agentchat", "autogen-ext", "mcp"]
    sdk_sources = {}
    for name in distributions:
        distribution = importlib.metadata.distribution(name)
        for entry in distribution.files or []:
            if str(entry).endswith(".py"):
                sdk_sources[str(entry)] = digest(distribution.locate_file(entry).read_bytes())
    report = {"host": host, "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
              "methodology": {"repeats": repeats, "warmups": warmups,
                "timing": "complete public SDK tool call; fixture reset, serialization and file oracle are outside timing",
                "model_boundary": "idle configured SDK; direct tool pipeline only; no Agent.run or model generation",
                "projection_boundary": "direct lane is MCP CallToolResult; SDK lane is actual host output or exception; these are distinct layers"},
              "provenance": {"binary": {"path": str(executable), "sha256": binary_hash},
                "source": source, "python": sys.version, "platform": platform.platform(),
                "sdk_versions": {n: importlib.metadata.version(n) for n in distributions},
                "sdk_source_sha256": sdk_sources},
              "scenarios": [{"name": c["name"], "cohort": c["cohort"], "read_files": list(c["files"]),
                "mutations": [{"name": name, "arguments": arguments, "expected_error": error}
                              for name, arguments, error in c["calls"]],
                "expected": {name: file_info(v) for name, v in c["expected"].items()}} for c in selected],
              "setup": [], "results": []}
    with tempfile.TemporaryDirectory(prefix="xnip-sdk-bench-") as temporary:
        cwd = Path(temporary)
        for direct in (True, False):
            provider = "direct-mcp" if direct else host
            started = time.perf_counter()
            async with runtime(host, cwd, str(executable), direct) as active:
                report["setup"].append({"provider": provider, "elapsed_ms": (time.perf_counter() - started) * 1000,
                                        "tool_names": sorted(active.schemas), "schema_sha256": digest(encoded(active.schemas))})
                for case in selected:
                    measurements, warming = [], []
                    for index in range(warmups + repeats):
                        result = await sample(active, case, cwd, provider)
                        (warming if index < warmups else measurements).append(result)
                    row = {"provider": provider, "scenario": case["name"], "samples": measurements,
                           "warmups": warming, "correct_runs": sum(s["correct"] for s in measurements),
                           "summary": {k: stats([s[k] for s in measurements]) for k in
                             ["read_ms", "edit_ms", "tool_ms", "tool_calls", "argument_bytes", "projection_bytes", "exception_bytes"]}}
                    report["results"].append(row)
    assert digest(executable.read_bytes()) == binary_hash
    assert source_provenance(host) == source
    for name in distributions:
        distribution = importlib.metadata.distribution(name)
        for entry in distribution.files or []:
            if str(entry).endswith(".py"):
                assert digest(distribution.locate_file(entry).read_bytes()) == sdk_sources[str(entry)]
    validate(report)
    return report


def render(report):
    host = report["host"]
    lines = [f"# {host} SDK MCP bridge benchmark", "",
             f"{len(report['scenarios'])} scenarios; {report['methodology']['repeats']} measured workflows and {report['methodology']['warmups']} warm-up per provider/scenario. Zero model calls.", "",
             "Both lanes execute the same xnip MCP editing engine. The direct lane uses a real JSON-RPC stdio client; the SDK lane invokes the framework's actual public tool pipeline. This measures transport/adaptation overhead and observed SDK result projection. Agent loops, model generation, billed tokens and native builtin replacement have their own boundaries.", "",
             "OpenAI Agents uses Agent.get_all_tools and FunctionTool.on_invoke_tool; its documented failure_error_function=None setting preserves MCP errors as exceptions. AutoGen constructs an AssistantAgent with the actual McpWorkbench and calls the workbench's public call_tool. smolagents uses ToolCallingAgent.execute_tool_call with actual MCPAdapt tools. SDK options and model traps are retained in the fixture source.", "",
             "Arguments are equal across providers. Direct MCP results and SDK projections are different returned layers; their byte counts are listed separately. Exceptions have zero returned projection bytes and their actual message bytes in a separate column. Full projected outputs, exceptions, input/result hashes and independent file goals are retained in JSON. Setup includes SDK import/discovery; warm timing includes the complete tool call. Fixture resets, byte accounting and file verification are outside timing. Three measured samples make nearest-rank p95 the maximum.", "",
             "Each rejected call requires its fixture's specific remote reason. Direct MCP additionally checks JSON-RPC code -32600; Agents requires its actual McpError, AutoGen its actual named ToolResult text, and smolagents its McpError-bearing SDK exception. Successful reads precede the mutation. Connection, startup and unrelated tool failures fail sampling, and warm-up outcomes pass the same checks.", "",
             f"Binary SHA-256 `{report['provenance']['binary']['sha256']}`; source `{report['provenance']['source']['sha256']}`. SDK versions: `{json.dumps(report['provenance']['sdk_versions'], sort_keys=True)}`.", "",
             "| Scenario | Direct edit median / p95 ms | SDK edit median / p95 ms | Direct / SDK read ms | Direct / SDK projected bytes | Direct / SDK exception bytes | Correct direct / SDK |",
             "|---|---:|---:|---:|---:|---:|---:|"]
    for case in report["scenarios"]:
        rows = [next(r for r in report["results"] if r["provider"] == p and r["scenario"] == case["name"])
                for p in ("direct-mcp", host)]
        a, b = [r["summary"] for r in rows]
        lines.append(f"| {case['name']} | {a['edit_ms']['median']:.2f} / {a['edit_ms']['p95']:.2f} | {b['edit_ms']['median']:.2f} / {b['edit_ms']['p95']:.2f} | {a['read_ms']['median']:.2f} / {b['read_ms']['median']:.2f} | {a['projection_bytes']['median']:,.0f} / {b['projection_bytes']['median']:,.0f} | {a['exception_bytes']['median']:,.0f} / {b['exception_bytes']['median']:,.0f} | {rows[0]['correct_runs']} / {rows[1]['correct_runs']} |")
    lines += ["", "## Reproduce", "", "```sh",
              f"python benches/{host.replace('-', '_')}_provider.py --executable target/release/xnip --repeats 3 --warmups 1", "```", ""]
    return "\n".join(lines)


def main(default_host=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", choices=HOSTS, default=default_host, required=default_host is None)
    parser.add_argument("--executable", type=Path, default=ROOT / "target/release/xnip")
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--scenarios")
    parser.add_argument("--output-prefix", type=Path)
    args = parser.parse_args()
    report = asyncio.run(run(args.host, args.executable, args.repeats, args.warmups,
                             args.scenarios.split(",") if args.scenarios else None))
    prefix = args.output_prefix or ROOT / "docs/benchmarks" / (args.host + "-provider-results")
    prefix.parent.mkdir(parents=True, exist_ok=True)
    prefix.with_suffix(".json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    prefix.with_suffix(".md").write_text(render(report))
    print(json.dumps({"host": args.host, "scenarios": len(report["scenarios"]),
                      "groups": len(report["results"]), "samples": sum(len(r["samples"]) for r in report["results"]),
                      "json": str(prefix.with_suffix('.json'))}))


if __name__ == "__main__":
    main()
