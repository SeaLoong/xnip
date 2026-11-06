#!/usr/bin/env python3
"""Real DeepAgents, LangGraph and PydanticAI file-tool measurements."""
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
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests"))
from edit_cost import scenarios as original_scenarios
from python_agents_runtime import Runtime, model_guard
from xnip_native_client import NativeClient
from native.python_agents_setup import prepare as verify_sdk_host

FRAMEWORKS = ("deepagents", "langgraph", "pydantic-ai")
MEASURES = ("read_ms", "mutation_ms", "tool_ms", "tool_calls", "argument_bytes", "result_bytes", "exception_bytes", "tool_bytes")


def digest(value):
    return hashlib.sha256(value).hexdigest()


def encoded(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()


def file_info(value):
    value = value.encode() if isinstance(value, str) else value
    return {"bytes": len(value), "sha256": digest(value)}


def stats(values):
    values = sorted(values)
    return {"runs": len(values), "min": values[0], "median": statistics.median(values),
            "p95": values[(len(values) * 95 + 99) // 100 - 1], "max": values[-1]}


def fixtures():
    cases = [{"name": case.name, "cohort": "original13", "files": case.files, "edits": case.edits,
              "expected": case.expected(), "purpose": case.purpose} for case in original_scenarios()]
    def one(name, source, old, text, expected, **fields):
        return {"name": name, "cohort": "python-sdk-extension", "files": {"example.txt": source},
                "edits": [{"file": "example.txt", "old": old, "text": text}],
                "expected": {"example.txt": expected}, **fields}
    cases += [
        one("bom-crlf", "\ufeffhead\r\nmode=old\r\n尾巴", "mode=old", "mode=new",
            "\ufeffhead\r\nmode=new\r\n尾巴", purpose="host EOL policy with UTF-8 BOM"),
        one("mixed-eol", "head\r\nold\r\nblock\nend\r\n", "old\r\nblock", "new\r\nblock",
            "head\r\nnew\r\nblock\nend\r\n", purpose="independent literal bytes and SDK-normalized bytes"),
        one("reject-missing", "old\n", "missing", "new", "old\n", error=True,
            purpose="host/core missing-match failure preserves storage"),
        one("reject-ambiguous", "old old\n", "old", "new", "old old\n", error=True,
            purpose="host/core uniqueness check preserves storage"),
        one("noop", "same\n", "same", "same", "same\n", purpose="unchanged bytes and actual SDK receipts"),
        {"name": "write-create", "cohort": "python-sdk-extension", "files": {}, "edits": [],
         "writes": [{"file": "new.txt", "text": "created\r\n尾巴"}],
         "expected": {"new.txt": "created\r\n尾巴"}, "purpose": "SDK tool creates a file"},
        {"name": "write-overwrite", "cohort": "python-sdk-extension", "files": {"example.txt": "old\r\n"}, "edits": [],
         "writes": [{"file": "example.txt", "text": "new\r\n尾巴"}],
         "expected": {"example.txt": "new\r\n尾巴"}, "purpose": "SDK whole-file write keeps supplied bytes"},
    ]
    for size in [65536, 1048576, 5242880]:
        tail = "\nmode=old\n"
        prefix = "x" * (size - len(tail))
        source = prefix + tail
        cases.append(one(f"large-{size}", source, "mode=old", "mode=new", prefix + "\nmode=new\n",
                         purpose=f"small edit in an exact {size}-byte file"))
    return cases


def host_expected(framework, fixture):
    expected = dict(fixture["expected"])
    if framework == "deepagents" and not fixture.get("error"):
        edited = {edit["file"] for edit in fixture["edits"]}
        for name in edited:
            expected[name] = expected[name].replace("\r\n", "\n").replace("\r", "\n")
    return expected


def expected_receipts(framework, fixture):
    """Independent SDK receipts for the prescribed calls and source strings."""
    files = dict(fixture["files"])
    rules = []
    for edit in fixture["edits"]:
        name, old, text = edit["file"], edit["old"], edit["text"]
        before = files[name]
        if framework == "deepagents":
            before, old, text = [value.replace("\r\n", "\n").replace("\r", "\n") for value in [before, old, text]]
        if fixture.get("error"):
            fragment = ("String not found in file" if framework == "deepagents" else "old text was not found") if fixture["name"] == "reject-missing" else ("appears 2 times" if framework == "deepagents" else "old text is ambiguous")
            rules.append({"file": name, "kind": "error", "contains": fragment})
            continue
        after = before.replace(old, text)
        rules.append({"file": name, "kind": "deepagents-edit", "occurrences": before.count(old)} if framework == "deepagents"
                     else {"file": name, "kind": "core-receipt", "content": "written" if before != after else "unchanged"})
        files[name] = after
    for write in fixture.get("writes", []):
        name, text = write["file"], write["text"]
        rules.append({"file": name, "kind": "deepagents-write"} if framework == "deepagents" else
                     {"file": name, "kind": "core-receipt", "content": "written" if files.get(name) != text else "unchanged"})
        files[name] = text
    return rules


def receipt_matches(rule, result, arguments):
    content = result["content"]
    if rule["kind"] == "error":
        return result["status"] == "error" and rule["contains"] in str(content)
    if result["status"] != "success":
        return False
    if rule["kind"] == "core-receipt":
        return content == rule["content"]
    path = arguments["file_path"]
    if not path.endswith("/" + rule["file"]):
        return False
    if rule["kind"] == "deepagents-write":
        return content == f"Updated file {path}"
    return content == f"Successfully replaced {rule['occurrences']} instance(s) of the string in '{path}'"


def provenance():
    names = ["Cargo.toml", "Cargo.lock", "integrations/catalog.json", "benches/edit_cost.py",
             "benches/python_agents_provider.py", "tests/python_agents_runtime.py",
             "benches/native/python_agents_setup.py", "benches/native/python_agents_requirements.in",
             "benches/native/python_agents_requirements.txt"]
    for directory in ["src", "integrations/native-python", "integrations/python-agents"]:
        names.extend(str(path.relative_to(ROOT)) for path in (ROOT / directory).rglob("*")
                     if path.is_file() and path.suffix in {".rs", ".py", ".toml"})
    files = {name: digest((ROOT / name).read_bytes()) for name in sorted(names)}
    return {"sha256": digest(encoded(files)), "files_sha256": files}


async def sample(runtime, fixture, directory, model_calls):
    shutil.rmtree(directory, ignore_errors=True)
    directory.mkdir(parents=True)
    for name, value in fixture["files"].items():
        path = directory / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(value.encode())
    calls = []
    client = getattr(runtime.owner, "client", None)
    native_client = client if isinstance(client, NativeClient) else None
    def bridge_snapshot():
        return {"request_bytes": native_client.request_bytes, "response_bytes": native_client.response_bytes,
                "actions": dict(native_client.actions)} if native_client else None
    before_bridge = bridge_snapshot()
    relative = directory.relative_to(runtime.cwd).as_posix()
    async def call(phase, name, arguments, function):
        a = encoded(arguments)
        started = time.perf_counter()
        try:
            result = await function()
            elapsed = (time.perf_counter() - started) * 1000
        except Exception as error:
            elapsed = (time.perf_counter() - started) * 1000
            result = {"name": name, "content": str(error), "status": "error",
                      "exception_type": type(error).__name__}
        r = None if "exception_type" in result else encoded(result["content"])
        exception_bytes = len(result["content"].encode()) if "exception_type" in result else 0
        calls.append({"phase": phase, "name": name, "elapsed_ms": elapsed,
                      "argument_bytes": len(a), "result_bytes": len(r) if r is not None else 0,
                      "exception_bytes": exception_bytes,
                      "argument_sha256": digest(a), "result_sha256": digest(r) if r is not None else None,
                      "is_error": result["status"] == "error", "arguments": json.loads(a),
                      "result_status": result["status"],
                      **({"result": result} if phase == "mutation" else {})})
        return result
    for name in fixture["files"]:
        target = f"{relative}/{name}"
        tool, arguments = runtime.request("read", target)
        result = await call("read", tool, arguments, lambda tool=tool, arguments=arguments: runtime.call(tool, arguments))
        assert result["status"] == "success", result
    for edit in fixture["edits"]:
        target = f"{relative}/{edit['file']}"
        tool, arguments = runtime.request("edit", target, old=edit["old"], text=edit["text"], replace_all=edit.get("all", False))
        await call("mutation", tool, arguments, lambda tool=tool, arguments=arguments: runtime.call(tool, arguments))
    for write in fixture.get("writes", []):
        target = f"{relative}/{write['file']}"
        tool, arguments = runtime.request("write", target, text=write["text"])
        await call("mutation", tool, arguments, lambda tool=tool, arguments=arguments: runtime.call(tool, arguments))
    outcomes = {}
    expected = host_expected(runtime.framework, fixture)
    for name, goal in expected.items():
        value = (directory / name).read_bytes()
        outcomes[name] = {"expected": file_info(goal), "literal_goal": file_info(fixture["expected"][name]),
                          "actual": file_info(value), "byte_exact": value == goal.encode(),
                          "literal_byte_exact": value == fixture["expected"][name].encode()}
    actual_files = sorted(path.relative_to(directory).as_posix() for path in directory.rglob("*") if path.is_file())
    unexpected = sorted(set(actual_files) - set(expected))
    mutations = [item for item in calls if item["phase"] == "mutation"]
    outcome_matches = all(receipt_matches(rule, item["result"], item["arguments"])
                          for rule, item in zip(expected_receipts(runtime.framework, fixture), mutations))
    byte_exact = all(item["byte_exact"] for item in outcomes.values()) and not unexpected
    total = lambda name: sum(item[name] for item in calls)
    after_bridge = bridge_snapshot()
    bridge = None
    if after_bridge:
        actions = {name: count - before_bridge["actions"].get(name, 0)
                   for name, count in after_bridge["actions"].items()
                   if count != before_bridge["actions"].get(name, 0)}
        bridge = {"actions": actions, "requests": sum(actions.values()),
                  **{name: after_bridge[name] - before_bridge[name] for name in ["request_bytes", "response_bytes"]}}
        bridge["total_bytes"] = bridge["request_bytes"] + bridge["response_bytes"]
    return {"read_ms": sum(item["elapsed_ms"] for item in calls if item["phase"] == "read"),
            "mutation_ms": sum(item["elapsed_ms"] for item in mutations), "tool_ms": total("elapsed_ms"),
            "tool_calls": len(calls), "argument_bytes": total("argument_bytes"), "result_bytes": total("result_bytes"),
            "exception_bytes": total("exception_bytes"),
            "tool_bytes": total("argument_bytes") + total("result_bytes") + total("exception_bytes"), "calls": calls, "outcomes": outcomes,
            "unexpected_files": unexpected, "outcome_matches": outcome_matches, "byte_exact": byte_exact,
            "correct": byte_exact and outcome_matches, "model_calls": model_calls(), "native_bridge": bridge}


def validate_report(report):
    expected_rows = {(host, provider, fixture["name"]) for host in FRAMEWORKS
                     for provider in (["filesystem-original", "xnip-native"] if host == "deepagents" else ["xnip-tools"])
                     for fixture in report["scenarios"]}
    assert {(row["framework"], row["provider"], row["scenario"]) for row in report["results"]} == expected_rows
    assert len(report["results"]) == len(expected_rows)
    fixtures_by_name = {fixture["name"]: fixture for fixture in report["scenarios"]}
    for row in report["results"]:
        fixture = fixtures_by_name[row["scenario"]]
        assert len(row["samples"]) == report["methodology"]["measured_runs"]
        for item in row["samples"]:
            assert item["model_calls"] == 0
            assert item["tool_calls"] == len(item["calls"])
            assert sum(call["phase"] == "read" for call in item["calls"]) == fixture["read_calls"]
            for call in item["calls"]:
                assert math.isfinite(call["elapsed_ms"]) and call["elapsed_ms"] >= 0
                assert call["argument_sha256"] == digest(encoded(call["arguments"]))
                assert call["argument_bytes"] == len(encoded(call["arguments"]))
                if call["phase"] == "read":
                    assert call["result_status"] == "success"
                else:
                    if "exception_type" in call["result"]:
                        assert call["result_sha256"] is None and call["result_bytes"] == 0
                        assert call["exception_bytes"] == len(call["result"]["content"].encode())
                    else:
                        assert call["result_sha256"] == digest(encoded(call["result"]["content"]))
                        assert call["result_bytes"] == len(encoded(call["result"]["content"]))
                        assert call["exception_bytes"] == 0
                    assert call["result_status"] == call["result"]["status"]
                    assert call["is_error"] == (call["result"]["status"] == "error")
            for field in ["argument_bytes", "result_bytes", "exception_bytes"]:
                assert item[field] == sum(call[field] for call in item["calls"])
            assert item["tool_bytes"] == item["argument_bytes"] + item["result_bytes"] + item["exception_bytes"]
            assert abs(item["tool_ms"] - item["read_ms"] - item["mutation_ms"]) < 0.001
            expected = fixture["host_expected"][row["framework"]]
            assert set(item["outcomes"]) == set(expected), "complete expected file oracle"
            for name, outcome in item["outcomes"].items():
                assert outcome["expected"] == expected[name]
                assert outcome["literal_goal"] == fixture["literal_expected"][name]
                assert outcome["byte_exact"] == (outcome["expected"] == outcome["actual"])
                assert outcome["literal_byte_exact"] == (outcome["literal_goal"] == outcome["actual"])
            assert item["byte_exact"] == (all(o["byte_exact"] for o in item["outcomes"].values()) and not item["unexpected_files"])
            mutations = [call for call in item["calls"] if call["phase"] == "mutation"]
            rules = fixture["expected_receipts"][row["framework"]]
            assert len(mutations) == len(rules)
            receipt_oracle = all(receipt_matches(rule, call["result"], call["arguments"]) for rule, call in zip(rules, mutations))
            assert receipt_oracle, "expected SDK receipt/error oracle"
            assert item["outcome_matches"] == receipt_oracle
            assert item["correct"] == (item["byte_exact"] and item["outcome_matches"])
            assert item["correct"]
            if item["native_bridge"] is not None:
                bridge = item["native_bridge"]
                assert bridge["requests"] == sum(bridge["actions"].values())
                assert bridge["total_bytes"] == bridge["request_bytes"] + bridge["response_bytes"]
        for field in MEASURES:
            assert row["summary"][field] == stats([sample[field] for sample in row["samples"]])
        assert row["correct_runs"] == sum(sample["correct"] for sample in row["samples"])
        assert row["literal_goal_runs"] == sum(all(o["literal_byte_exact"] for o in sample["outcomes"].values())
                                               and not sample["unexpected_files"] for sample in row["samples"])
    for row in [item for item in report["results"] if item["framework"] == "deepagents" and item["provider"] == "xnip-native"]:
        original = next(item for item in report["results"] if item["framework"] == "deepagents"
                        and item["provider"] == "filesystem-original" and item["scenario"] == row["scenario"])
        for left, right in zip(row["samples"], original["samples"]):
            assert [c["argument_sha256"] for c in left["calls"]] == [c["argument_sha256"] for c in right["calls"]]
            assert [c["result_sha256"] for c in left["calls"]] == [c["result_sha256"] for c in right["calls"]]
            assert left["outcomes"] == right["outcomes"]


async def run_benchmark(executable, repeats=3, warmups=1, names=None, build_profile="release"):
    executable = Path(executable).resolve()
    sdk_host = verify_sdk_host(Path(sys.prefix).parent, check=True)
    source, binary = provenance(), file_info(executable.read_bytes())
    selected = [item for item in fixtures() if names is None or item["name"] in names]
    assert selected
    if names is not None:
        assert {item["name"] for item in selected} == set(names)
    report = {"schema": 1, "created_at": __import__("datetime").datetime.now(__import__("datetime").timezone.utc).isoformat(),
        "methodology": {"measured_runs": repeats, "warmups": warmups,
            "dispatch": "DeepAgents filesystem tools and LangGraph native tools via compiled StateGraph/ToolNode; PydanticAI MCPToolset.call_tool",
            "timing": "complete SDK tool call; fixture reset, hashing and serialization accounting are outside timing",
            "payload": "actual SDK tool arguments plus ToolMessage.content or PydanticAI tool result; thrown exception messages counted separately; transport/identity/status envelopes are separate",
            "sampling": "no model node in compiled tool graphs; actual LangChain/Pydantic model methods are trapped and counted",
            "p95": "nearest rank; with three samples this is the maximum"},
        "provenance": {"binary": {**binary, "path": str(executable), "build_profile": build_profile}, "source": source,
            "python": sys.version, "platform": platform.platform(),
            "sdk_host": {name: sdk_host[name] for name in ["sdks", "sources", "lock_sha256"]},
            "sdks": {name: importlib.metadata.version(name) for name in ["deepagents", "langgraph", "langchain-core", "pydantic-ai-slim", "mcp"]}},
        "scenarios": [{"name": item["name"], "cohort": item["cohort"], "purpose": item.get("purpose", ""),
            "read_calls": len(item["files"]),
            "expected_error": item.get("error", False), "literal_expected": {name: file_info(value) for name, value in item["expected"].items()},
            "host_expected": {host: {name: file_info(value) for name, value in host_expected(host, item).items()} for host in FRAMEWORKS}}
            | {"expected_receipts": {host: expected_receipts(host, item) for host in FRAMEWORKS}}
            for item in selected], "setup": [], "results": []}
    with tempfile.TemporaryDirectory(prefix="python-agent-provider-", dir=ROOT / "target") as temporary, model_guard() as model_calls:
        cwd = Path(temporary)
        for framework in FRAMEWORKS:
            catalog = None
            for native in ([False, True] if framework == "deepagents" else [True]):
                started = time.perf_counter()
                async with Runtime(framework, cwd, executable, native=native) as runtime:
                    setup_ms = (time.perf_counter() - started) * 1000
                    current = runtime.catalog()
                    if catalog is not None:
                        assert catalog == current
                    catalog = current
                    provider = ("xnip-native" if native else "filesystem-original") if framework == "deepagents" else "xnip-tools"
                    report["setup"].append({"framework": framework, "provider": provider, "setup_ms": setup_ms,
                                            "tool_schema_sha256": digest(encoded(current)), "catalog": current})
                    for fixture in selected:
                        samples, warms = [], []
                        for iteration in range(warmups + repeats):
                            result = await sample(runtime, fixture, cwd / "cases" / fixture["name"] / str(iteration), model_calls)
                            assert result["correct"], f"{framework}/{provider}/{fixture['name']}: warm/measured oracle"
                            (warms if iteration < warmups else samples).append(result)
                        report["results"].append({"framework": framework, "provider": provider, "scenario": fixture["name"],
                            "correct_runs": sum(item["correct"] for item in samples), "warmups": warms, "samples": samples,
                            "literal_goal_runs": sum(all(o["literal_byte_exact"] for o in item["outcomes"].values())
                                                     and not item["unexpected_files"] for item in samples),
                            "summary": {field: stats([item[field] for item in samples]) for field in MEASURES}})
                        print(f"{framework} {provider} {fixture['name']}: {report['results'][-1]['summary']['mutation_ms']['median']:.2f}ms", file=sys.stderr, flush=True)
        assert model_calls() == 0
    assert file_info(executable.read_bytes()) == binary, "binary changed while measuring"
    assert provenance() == source, "source changed while measuring"
    assert verify_sdk_host(Path(sys.prefix).parent, check=True) == sdk_host, "installed SDK/adapter sources changed while measuring"
    validate_report(report)
    return report


def render_report(report):
    lines = ["# Python agent SDK tool benchmarks", "",
        f"DeepAgents {report['provenance']['sdks']['deepagents']}, LangGraph {report['provenance']['sdks']['langgraph']} and PydanticAI {report['provenance']['sdks']['pydantic-ai-slim']}. "
        f"{len(report['scenarios'])} fixtures; {report['methodology']['measured_runs']} measured runs and {report['methodology']['warmups']} warm-up per fixture/provider. Zero model calls.", "",
        "DeepAgents compares its original FilesystemBackend and xnip regular-file persistence through the same official FilesystemMiddleware tools, permissions wrapper and compiled LangGraph ToolNode. Matching, schemas and rendered results remain official SDK behavior. Both lanes disable large-result eviction during this file-tool cohort; ordinary applications retain their middleware policy.", "",
        "LangGraph provides a tool registry and graph executor rather than a builtin file editor. Its lane executes the registered core-backed StructuredTools through a real StateGraph and ToolNode. PydanticAI executes its official MCPToolset.call_tool path against a real xnip stdio server. Their lanes measure supported SDK tool pipelines without inventing an original editor baseline.", "",
        "The original thirteen byte fixtures remain unchanged. DeepAgents' edit policy uses Python universal newlines and its official string-replacement helper, so its host-specific oracle normalizes edited text to LF; the original literal byte goal is retained independently. Other hosts use xnip literal byte semantics. Complete expected files, unexpected files and SDK receipts/errors have independent oracles. Raw JSON retains actual argument objects and mutation results alongside their hashes; missing/ambiguous errors must have their specific cause, and successful receipts distinguish writes, no-ops and replacement counts.", "",
        "Warm timing includes the complete SDK tool call. Fixture setup, file hashing and JSON accounting occur outside timing. SDK startup is reported separately. Payload counts cover actual SDK tool arguments and ToolMessage.content or the PydanticAI tool result; thrown exception messages have a separate byte field. Identity/status envelopes and generated message IDs are separate. Raw samples also retain actual native JSONL byte counters where that client is owned by the adapter; MCP transport-envelope bytes remain unknown. These measurements describe tool execution and local payloads, with zero agent/model generation or billing estimates.", "",
        f"Binary SHA-256 `{report['provenance']['binary']['sha256']}` ({report['provenance']['binary']['build_profile']}); maintained source `{report['provenance']['source']['sha256']}`. Raw JSON retains package versions, source fingerprints and all measured samples. With three samples nearest-rank p95 is the maximum.", "",
        "| Framework | Provider | SDK/session setup ms |", "|---|---|---:|"]
    for item in report["setup"]:
        lines.append(f"| {item['framework']} | {item['provider']} | {item['setup_ms']:.2f} |")
    lines += ["", "| Framework | Provider | Fixture | Mutation median / p95 ms | Read median ms | SDK tool bytes | Host goal+outcome / literal goal |", "|---|---|---|---:|---:|---:|---:|"]
    for item in report["results"]:
        summary = item["summary"]
        lines.append(f"| {item['framework']} | {item['provider']} | {item['scenario']} | {summary['mutation_ms']['median']:.2f} / {summary['mutation_ms']['p95']:.2f} | {summary['read_ms']['median']:.2f} | {summary['tool_bytes']['median']:,.0f} | {item['correct_runs']}/{len(item['samples'])} / {item['literal_goal_runs']}/{len(item['samples'])} |")
    lines += ["", "## Reproduce", "", "```sh", "python benches/native/python_agents_setup.py", "python benches/native/python_agents_setup.py --check",
              "target/native-hosts/python-agents/.venv/bin/python benches/python_agents_provider.py --executable target/release/xnip --repeats 3 --warmups 1", "```", "",
              "On Windows use `.venv/Scripts/python.exe` and `xnip.exe`. `--scenarios` selects comma-separated fixtures; `--output-prefix` selects local output files. Finish builds and SDK preparation before collecting samples.", ""]
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, default=ROOT / "target/release/xnip")
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--scenarios")
    parser.add_argument("--output-prefix", type=Path, default=ROOT / "docs/benchmarks/python-agents-provider-results")
    parser.add_argument("--build-profile", default="release")
    parser.add_argument("--render", type=Path)
    options = parser.parse_args()
    if options.render:
        report = json.loads(options.render.read_text())
        validate_report(report)
    else:
        report = asyncio.run(run_benchmark(options.executable, options.repeats, options.warmups,
            options.scenarios.split(",") if options.scenarios else None, options.build_profile))
    options.output_prefix.parent.mkdir(parents=True, exist_ok=True)
    options.output_prefix.with_suffix(".json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    options.output_prefix.with_suffix(".md").write_text(render_report(report))


if __name__ == "__main__":
    main()
