#!/usr/bin/env python3
"""Official OpenHands file_editor on original and xnip-backed storage."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import time
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def file_info(data):
    return {"bytes": len(data), "sha256": digest(data)}


def stats(values):
    ordered = sorted(values)
    return {"runs": len(values), "min": ordered[0], "median": statistics.median(ordered),
            "p95": ordered[math.ceil(len(ordered) * .95) - 1], "max": ordered[-1]}


def scenarios():
    from edit_cost import scenarios as shared
    cases = []
    for case in shared():
        operations = []
        for edit in case.edits:
            old, new = edit["old"].replace("\r\n", "\n"), edit["text"].replace("\r\n", "\n")
            if edit.get("all"):
                source = case.files[edit["file"]].replace("\r\n", "\n")
                old, new = source, source.replace(old, new)
            operations.append({"command": "str_replace", "file": edit["file"], "old_str": old, "new_str": new})
        cases.append({"name": case.name, "cohort": "original13", "purpose": case.purpose,
                      "files": {name: text.encode() for name, text in case.files.items()},
                      "expected": {name: text.encode() for name, text in case.expected().items()}, "operations": operations})
    def one(name, before, old, new, expected, **fields):
        return {"name": name, "cohort": "openhands-extension", "files": {"example.txt": before},
                "expected": {"example.txt": expected},
                "operations": [{"command": "str_replace", "file": "example.txt", "old_str": old, "new_str": new}], **fields}
    cases += [
        one("openhands-missing", b"old\n", "missing", "new", b"old\n", error=True, error_message="did not appear verbatim"),
        one("openhands-ambiguous", b"same same\n", "same", "new", b"same same\n", error=True, error_message="Multiple occurrences"),
        one("openhands-bom-crlf", "\ufeffold\r\nEOF".encode(), "old", "new", "\ufeffnew\r\nEOF".encode()),
        one("openhands-mixed-eol", b"head\r\nold\nend\r\n", "old", "new", b"head\r\nnew\nend\r\n"),
        {"name": "openhands-create", "cohort": "openhands-extension", "files": {},
         "expected": {"created.txt": "created\r\n尾巴".encode()},
         "operations": [{"command": "create", "file": "created.txt", "file_text": "created\r\n尾巴"}]},
        {"name": "openhands-insert", "cohort": "openhands-extension", "files": {"example.txt": b"first\nlast\n"},
         "expected": {"example.txt": b"first\nmiddle\nlast\n"},
         "operations": [{"command": "insert", "file": "example.txt", "insert_line": 1, "new_str": "middle"}]},
        {"name": "openhands-undo", "cohort": "openhands-extension", "files": {"example.txt": b"old\n"},
         "expected": {"example.txt": b"old\n"},
         "operations": [{"command": "str_replace", "file": "example.txt", "old_str": "old", "new_str": "new"},
                        {"command": "undo_edit", "file": "example.txt"}]},
    ]
    for size in [65536, 1048576, 5242880]:
        tail = b"\nmode=old\n"
        source = b"x" * (size - len(tail)) + tail
        cases.append(one(f"openhands-large-{size}", source, "mode=old", "mode=new", source.replace(b"mode=old", b"mode=new")))
    return cases


def provenance():
    names = ["Cargo.toml", "Cargo.lock", "integrations/catalog.json", "benches/openhands_provider.py",
             "benches/edit_cost.py", "benches/native/openhands_setup.py", "benches/native/openhands_requirements.txt",
             "tests/openhands_native_contract.py"]
    for directory in ["src", "integrations/openhands-native", "integrations/native-python"]:
        names += [str(p.relative_to(ROOT)) for p in (ROOT / directory).rglob("*") if p.suffix in [".rs", ".py", ".toml"]]
    files = {name: digest((ROOT / name).read_bytes()) for name in sorted(names)}
    return {"sha256": digest(json.dumps(files, sort_keys=True).encode()), "files_sha256": files}


def load_runtime(executable):
    os.environ["XNIP_TEST_BINARY"] = str(executable)
    spec = importlib.util.spec_from_file_location("openhands_native_runtime", ROOT / "tests/openhands_native_contract.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def actual_files(directory):
    return sorted(str(p.relative_to(directory)) for p in directory.rglob("*") if p.is_file())


def sample(runtime, module, fixture, directory, counter):
    import shutil
    shutil.rmtree(directory, ignore_errors=True)
    directory.mkdir(parents=True)
    for name, data in fixture["files"].items():
        target = directory / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    calls = []
    if counter:
        counter.drain()
        before = (counter.request_bytes, counter.response_bytes, dict(counter.actions))
    def call(phase, operation):
        args = {k: v for k, v in operation.items() if k != "file"}
        args["path"] = str(directory / operation["file"])
        started = time.perf_counter()
        result = module.call(runtime, **args)
        elapsed = (time.perf_counter() - started) * 1000
        raw_args = json.dumps(args, ensure_ascii=False, separators=(",", ":")).encode()
        raw_result = result.model_dump_json(exclude_none=True).encode()
        calls.append({"phase": phase, "command": args["command"], "elapsed_ms": elapsed,
                      "argument_bytes": len(raw_args), "result_bytes": len(raw_result),
                      "argument_sha256": digest(raw_args), "result_sha256": digest(raw_result),
                      "is_error": result.is_error,
                      **({"error": raw_result.decode()} if result.is_error else {})})
    for name in fixture["files"]:
        call("read", {"command": "view", "file": name})
    for operation in fixture["operations"]:
        call("mutation", operation)
    if counter:
        counter.drain()
        actions = {name: count - before[2].get(name, 0) for name, count in counter.actions.items() if count != before[2].get(name, 0)}
        bridge = {"request_bytes": counter.request_bytes - before[0], "response_bytes": counter.response_bytes - before[1], "actions": actions}
    else:
        bridge = {"request_bytes": 0, "response_bytes": 0, "actions": {}}
    bridge["total_bytes"] = bridge["request_bytes"] + bridge["response_bytes"]
    outcomes = {name: {"expected": file_info(expected), "actual": file_info((directory / name).read_bytes()),
                       "byte_exact": (directory / name).read_bytes() == expected} for name, expected in fixture["expected"].items()}
    unexpected = sorted(set(actual_files(directory)) - set(fixture["expected"]))
    mutation = [call for call in calls if call["phase"] == "mutation"]
    return {"read_ms": sum(call["elapsed_ms"] for call in calls if call["phase"] == "read"),
            "mutation_ms": sum(call["elapsed_ms"] for call in mutation), "tool_ms": sum(call["elapsed_ms"] for call in calls),
            "tool_calls": len(calls), "argument_bytes": sum(call["argument_bytes"] for call in calls),
            "result_bytes": sum(call["result_bytes"] for call in calls), "calls": calls, "bridge": bridge,
            "outcomes": outcomes, "unexpected_files": unexpected,
            "byte_exact": all(value["byte_exact"] for value in outcomes.values()) and not unexpected,
            "outcome_matches": all(call["is_error"] == fixture.get("error", False) for call in mutation), "model_calls": 0}


MEASURES = ["read_ms", "mutation_ms", "tool_ms", "tool_calls", "argument_bytes", "result_bytes"]


def validate_report(report):
    assert len(report["results"]) == 2 * len(report["scenarios"])
    assert len({(r["provider"], r["scenario"]) for r in report["results"]}) == len(report["results"])
    for row in report["results"]:
        assert row["provider"] in ["openhands-original", "xnip-native"]
        fixture = next(s for s in report["scenarios"] if s["name"] == row["scenario"])
        assert len(row["samples"]) == report["methodology"]["measured_runs"]
        for value in row["samples"]:
            assert value["model_calls"] == 0 and value["outcome_matches"]
            assert sum(call["phase"] == "read" for call in value["calls"]) == fixture["read_calls"]
            assert all(not call["is_error"] for call in value["calls"] if call["phase"] == "read")
            mutations = [call for call in value["calls"] if call["phase"] == "mutation"]
            assert len(mutations) == fixture["mutation_calls"]
            assert value["outcome_matches"] == all(call["is_error"] == fixture["expected_error"] for call in mutations)
            if fixture["expected_error"]:
                assert all(fixture["expected_error_message"] in call["error"] for call in mutations)
            assert math.isclose(value["tool_ms"], value["read_ms"] + value["mutation_ms"], abs_tol=1e-6)
            assert sorted(value["outcomes"]) == sorted(fixture["expected"])
            for name, outcome in value["outcomes"].items():
                assert outcome["expected"] == fixture["expected"][name]
                assert outcome["byte_exact"] == (outcome["expected"] == outcome["actual"])
            assert value["byte_exact"] == (all(v["byte_exact"] for v in value["outcomes"].values()) and not value["unexpected_files"])
            assert value["tool_calls"] == len(value["calls"])
            for field in ["argument_bytes", "result_bytes"]:
                assert value[field] == sum(call[field] for call in value["calls"])
        for field in MEASURES:
            assert row["summary"][field] == stats([s[field] for s in row["samples"]])
    for native in [row for row in report["results"] if row["provider"] == "xnip-native"]:
        original = next(row for row in report["results"] if row["provider"] == "openhands-original" and row["scenario"] == native["scenario"])
        for a, b in zip(original["samples"], native["samples"], strict=True):
            assert a["outcomes"] == b["outcomes"], native["scenario"]
            assert [(c["argument_sha256"], c["result_sha256"], c["is_error"]) for c in a["calls"]] == [(c["argument_sha256"], c["result_sha256"], c["is_error"]) for c in b["calls"]], native["scenario"]


def run_benchmark(executable, repeats=3, warmups=1, names=None, build_profile="release"):
    from pydantic_core import to_jsonable_python
    sys.path.insert(0, str(ROOT / "benches"))
    executable = Path(executable).resolve()
    binary = digest(executable.read_bytes())
    source = provenance()
    host_directory = Path(sys.prefix).parent
    subprocess.run([sys.executable, str(ROOT / "benches/native/openhands_setup.py"), "--check",
                    "--destination", str(host_directory)], check=True, capture_output=True, text=True)
    host_receipt = json.loads((host_directory / "native-host.json").read_text())
    started = time.perf_counter()
    module = load_runtime(executable)
    fixtures = [case for case in scenarios() if names is None or case["name"] in names]
    if names is not None:
        assert set(names) == {case["name"] for case in fixtures}
    report = {"created_at": datetime.now(timezone.utc).isoformat(),
              "methodology": {"measured_runs": repeats, "warmup_runs": warmups,
                              "entry": "Conversation.execute_tool public tool-only API; confirmation is covered by Agent.step E2E",
                              "timing": "action construction and actual SDK execution; counters, receipt drain, fixture IO and oracle outside timing"},
              "provenance": {"binary": {"sha256": binary, "build_profile": build_profile}, "source": source,
                             "python": platform.python_version(), "platform": platform.platform(), "sdk": "1.60.1",
                             "host": host_receipt},
              "bootstrap_ms": (time.perf_counter() - started) * 1000,
              "scenarios": [{"name": s["name"], "cohort": s["cohort"], "read_calls": len(s["files"]),
                             "mutation_calls": len(s["operations"]), "expected_error": s.get("error", False),
                             "expected_error_message": s.get("error_message"),
                             "expected": {n: file_info(b) for n, b in s["expected"].items()}} for s in fixtures],
              "setup": [], "results": []}
    with tempfile.TemporaryDirectory(prefix="openhands-provider-", dir=ROOT / "target") as temporary, \
            patch.object(module.LLM, "completion", side_effect=AssertionError("benchmark called model")) as sync, \
            patch.object(module.LLM, "acompletion", side_effect=AssertionError("benchmark called async model")) as async_:
        cwd = Path(temporary)
        schema = None
        for native in [False, True]:
            setup_start = time.perf_counter()
            runtime = module.conversation(str(cwd), native=native)
            try:
                module.call(runtime, "view", cwd)
                tool = runtime.agent.tools_map["file_editor"]
                catalog = to_jsonable_python(tool.to_mcp_tool())
                if schema is not None:
                    assert catalog == schema
                schema = catalog
                report["setup"].append({"provider": "xnip-native" if native else "openhands-original",
                                        "setup_ms": (time.perf_counter() - setup_start) * 1000,
                                        "schema_sha256": digest(json.dumps(catalog, sort_keys=True).encode())})
                counter = tool.executor.client if native else None
                for fixture in fixtures:
                    values = [sample(runtime, module, fixture, cwd / fixture["name"] / str(i), counter) for i in range(warmups + repeats)]
                    measured = values[warmups:]
                    report["results"].append({"provider": "xnip-native" if native else "openhands-original", "scenario": fixture["name"],
                                              "summary": {name: stats([v[name] for v in measured]) for name in MEASURES},
                                              "warmups": values[:warmups], "samples": measured})
                    assert sync.call_count + async_.call_count == 0
            finally:
                runtime.close()
        assert sync.call_count + async_.call_count == 0
    assert digest(executable.read_bytes()) == binary
    assert provenance()["sha256"] == source["sha256"]
    validate_report(report)
    return report


def render_report(report):
    lines = ["# OpenHands SDK storage provider benchmark", "",
             f"OpenHands SDK/tools 1.60.1; {len(report['scenarios'])} fixtures, {report['methodology']['measured_runs']} measured runs and {report['methodology']['warmup_runs']} warm-up per provider. Zero model calls.", "",
             "Both providers execute the original file_editor schema, action validation, matching, snippets and history through Conversation.execute_tool. The public tool-only API skips interactive confirmation; the separate Agent.step E2E covers confirmation, hooks, resource scheduling and interruption.", "",
             "Warm execution includes action construction and SDK dispatch. Setup, fixture resets, byte hashing, result serialization accounting and waiting for asynchronous cleanup receipts are separate. Native IPC counters include every actual request/response. Three samples make nearest-rank p95 the maximum.", "",
             "The independent byte goal preserves untouched storage. Both providers retain OpenHands universal-newline behavior; normalization differences remain visible. Compatibility separately requires identical actual SDK observations and stored bytes.", "",
             f"Binary SHA-256 `{report['provenance']['binary']['sha256']}` ({report['provenance']['binary']['build_profile']}); maintained source `{report['provenance']['source']['sha256']}`.", "",
             "| Fixture | Original mutation median / p95 ms | Native mutation median / p95 ms | Original / native byte-goal runs | Native IPC bytes |",
             "|---|---:|---:|---:|---:|"]
    for fixture in report["scenarios"]:
        original, native = [next(row for row in report["results"] if row["provider"] == provider and row["scenario"] == fixture["name"]) for provider in ["openhands-original", "xnip-native"]]
        a, b = original["summary"]["mutation_ms"], native["summary"]["mutation_ms"]
        lines.append(f"| {fixture['name']} | {a['median']:.2f} / {a['p95']:.2f} | {b['median']:.2f} / {b['p95']:.2f} | {sum(s['byte_exact'] for s in original['samples'])}/{len(original['samples'])} / {sum(s['byte_exact'] for s in native['samples'])}/{len(native['samples'])} | {int(statistics.median(s['bridge']['total_bytes'] for s in native['samples'])):,} |")
    lines += ["", "## Reproduce", "", "```sh", "python benches/native/openhands_setup.py", "target/native-hosts/openhands-1.60.1/.venv/bin/python benches/openhands_provider.py --executable target/release/xnip", "```", ""]
    return "\n".join(lines)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, default=ROOT / "target/release/xnip")
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--scenarios", help="comma-separated fixture names")
    parser.add_argument("--build-profile", default="release")
    parser.add_argument("--output-prefix", type=Path, default=ROOT / "docs/benchmarks/openhands-provider-results")
    args = parser.parse_args()
    report = run_benchmark(args.executable, args.repeats, args.warmups, args.scenarios.split(",") if args.scenarios else None, args.build_profile)
    args.output_prefix.parent.mkdir(parents=True, exist_ok=True)
    args.output_prefix.with_suffix(".json").write_text(json.dumps(report, indent=2) + "\n")
    args.output_prefix.with_suffix(".md").write_text(render_report(report))
    print(json.dumps({"fixtures": len(report["scenarios"]), "samples": sum(len(row["samples"]) for row in report["results"])}))
