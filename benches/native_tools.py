#!/usr/bin/env python3
"""Paired execution of real native edit tools and xnip; never invokes a model."""
from __future__ import annotations
import argparse
import contextlib
import dataclasses
import difflib
import hashlib
import json
import os
from pathlib import Path
import queue
import statistics
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
from edit_cost import Counter, Mcp, Scenario, scenarios, dump, execution_environment
from native.codex_adapter import discover as codex_discover, apply_patch as codex_apply
from native_report import write_report

FENCE = chr(96) * 3
TASKS = {
    "tiny-config": "Change the configured server port from 8080 to 3000, preserving other settings.",
    "large-local-change": "Set item_1000 to 42 and leave the other declarations unchanged.",
    "multiline-function": "Replace total's accumulator loop with sum(item.price for item in items).",
    "unicode": "Change the greeting to 高效编辑 🚀 while preserving the other Unicode text.",
    "crlf": "Set port to 3000 while preserving the original CRLF line endings.",
    "no-final-newline": "Set port to 3000 without adding a final newline.",
    "disambiguated-block": "Make second() return 1 while first() continues returning 0.",
    "global-replace": "Rename every old_name call to new_name.",
    "same-file-batch": "Set one to 10 and three to 30, leaving two unchanged.",
    "multi-file-batch": "Enable the feature, set timeout to 20 and retries to 3 in their existing files.",
    "long-line": "Change mode=old to mode=new while preserving the preceding data payload.",
    "insert-after-anchor": "Add import sys immediately after import os.",
    "delete-block": "Remove the obsolete comment and old_setting entry, preserving start and end.",
}


class Worker:
    def __init__(self, command):
        self.process = subprocess.Popen(command, cwd=ROOT, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf8",
            env=execution_environment(), bufsize=1)
        self.messages = queue.Queue()
        self.diagnostics = []
        def read():
            try:
                for line in self.process.stdout:
                    try:
                        self.messages.put(json.loads(line))
                    except json.JSONDecodeError:
                        self.diagnostics.append(line)
            finally:
                self.messages.put(EOFError("native adapter exited"))
        def errors():
            for line in self.process.stderr:
                self.diagnostics.append(line)
        self.reader = threading.Thread(target=read, daemon=True)
        self.errors = threading.Thread(target=errors, daemon=True)
        self.reader.start()
        self.errors.start()

    def request(self, value):
        self.process.stdin.write(dump(value) + "\n")
        self.process.stdin.flush()
        reply = self.messages.get(timeout=180)
        if isinstance(reply, BaseException):
            raise RuntimeError(str(reply) + ": " + "".join(self.diagnostics)[-2000:])
        return reply

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=5)
        self.reader.join(timeout=5)
        self.errors.join(timeout=5)
        for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
            stream.close()


def normalize(value, root):
    if isinstance(value, str):
        return value.replace(str(root), "/workspace").replace(str(root).replace("\\", "/"), "/workspace")
    if isinstance(value, list):
        return [normalize(item, root) for item in value]
    if isinstance(value, dict):
        return {key: normalize(item, root) for key, item in value.items()}
    return value


def grouped_changes(before, after):
    a, b = before.splitlines(), after.splitlines()
    return a, b, list(difflib.SequenceMatcher(a=a, b=b, autojunk=False).get_grouped_opcodes(3))


def patch_payload(files, expected):
    result = ["*** Begin Patch\n"]
    for name in sorted(files):
        if files[name] == expected[name]:
            continue
        a, b, groups = grouped_changes(files[name], expected[name])
        result.append(f"*** Update File: {name}\n")
        for group in groups:
            result.append("@@\n")
            for tag, i, j, k, l in group:
                if tag == "equal": result.extend(" " + line + "\n" for line in a[i:j])
                if tag in ("delete", "replace"): result.extend("-" + line + "\n" for line in a[i:j])
                if tag in ("insert", "replace"): result.extend("+" + line + "\n" for line in b[k:l])
            if group[-1][2] == len(a):
                result.append("*** End of File\n")
    result.append("*** End Patch\n")
    return "".join(result)


def aider_payload(files, expected, format):
    result = []
    for name in sorted(files):
        if files[name] == expected[name]:
            continue
        before, after = files[name].replace("\r\n", "\n"), expected[name].replace("\r\n", "\n")
        if format == "whole":
            result.append(f"{name}\n{FENCE}\n{after}" + ("" if after.endswith("\n") else "\n") + FENCE + "\n")
        elif format == "diff":
            a, b, groups = grouped_changes(before, after)
            for group in groups:
                old = "\n".join(a[group[0][1]:group[-1][2]]) + "\n"
                new = "\n".join(b[group[0][3]:group[-1][4]]) + "\n"
                result.append(f"{name}\n{FENCE}\n<<<<<<< SEARCH\n{old}=======\n{new}>>>>>>> REPLACE\n{FENCE}\n")
        else:
            # Actual aider UnifiedDiffCoder parses these normal unified hunks.
            diff = list(difflib.unified_diff(before.splitlines(), after.splitlines(),
                fromfile=name, tofile=name, n=3, lineterm=""))
            result.append(FENCE + "diff\n" + "\n".join(diff) + "\n" + FENCE + "\n")
    return "".join(result)


def contextual_edits(files, expected):
    """Construct the before/after 3-line context explicitly prescribed by Gemini."""
    result = []
    for name, before in files.items():
        a, b = before.splitlines(keepends=True), expected[name].splitlines(keepends=True)
        for group in difflib.SequenceMatcher(a=a, b=b, autojunk=False).get_grouped_opcodes(3):
            result.append({"file": name, "old": "".join(a[group[0][1]:group[-1][2]]),
                "text": "".join(b[group[0][3]:group[-1][4]])})
    return result


def probes():
    return [
        {"name": "ambiguous-line", "files": {"src/example.txt": "first\nsame\nmiddle\nsame\nlast\n"},
         "edit": {"file": "src/example.txt", "old": "same\n", "text": "changed\n"},
         "observation": "Does an ambiguous target reject, modify the first occurrence, or request model repair?"},
        {"name": "stale-context", "files": {"src/example.txt": "version=2\n"},
         "edit": {"file": "src/example.txt", "old": "version=1\n", "text": "version=3\n"},
         "observation": "The supplied old text is stale; record mutation/rejection without treating different contracts as bugs."},
        {"name": "whitespace-drift", "files": {"src/example.txt": "  value = 1\n"},
         "edit": {"file": "src/example.txt", "old": "\tvalue = 1\n", "text": "\tvalue = 2\n"},
         "observation": "Record whether native fallback matching accepts whitespace drift."},
        {"name": "overlapping-literal", "files": {"src/example.txt": "ababa\n"},
         "edit": {"file": "src/example.txt", "old": "aba", "text": "X"},
         "observation": "Overlapping substring ambiguity is meaningful only for substring edit interfaces."},
        {"name": "aider-accepted-unclosed-whole", "files": {"src/example.txt": "port=8080"},
         "expected": {"src/example.txt": "port=3000"},
         "edit": {"file": "src/example.txt", "old": "port=8080", "text": "port=3000"},
         "observation": "WholeFileCoder accepts an unclosed fence that preserves missing EOF; this is an alternate accepted encoding, not the canonical closed-fence profile."},
    ]


def reset_files(root, files):
    for name, content in files.items():
        dest = root / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(content.encode("utf8"))


def actual_files(root, files):
    # Include unexpected files as well as missing expected paths.
    names = set(files) | {path.relative_to(root).as_posix() for path in root.rglob("*") if path.is_file()}
    return {name: (root / name).read_bytes() if (root / name).is_file() else None for name in sorted(names)}


def observe(actual, expected):
    original = {name: value.encode("utf8") for name, value in expected.items()}
    exact = actual == original
    normalized = set(actual) == set(expected) and all(actual.get(name) is not None and
        actual[name].decode("utf8", errors="replace").replace("\r\n", "\n") == text.replace("\r\n", "\n")
        for name, text in expected.items())
    return {"byte_exact": exact, "lf_normalized_equal": normalized,
            "files": {name: {"sha256": hashlib.sha256(data).hexdigest() if data is not None else None,
                "bytes": len(data) if data is not None else None,
                "utf8": data.decode("utf8", errors="replace") if data is not None else None}
                for name, data in actual.items()}}


class Backend:
    def __init__(self, name, family, mode, metadata, worker=None, format=None, line_endings=None):
        self.name, self.family, self.mode = name, family, mode
        self.metadata, self.worker = metadata, worker
        self.format, self.line_endings = format, line_endings

    def execute(self, root, files, expected, edits, purpose, probe=False):
        calls = []
        if self.mode == "gemini-context" and not probe:
            edits = contextual_edits(files, expected)
        patch = None
        if self.mode in ("codex", "opencode-patch"):
            if probe:
                edit = edits[0]
                patch = ("*** Begin Patch\n*** Update File: " + edit["file"] + "\n@@\n" +
                    "".join("-" + line + "\n" for line in edit["old"].splitlines()) +
                    "".join("+" + line + "\n" for line in edit["text"].splitlines()) + "*** End Patch\n")
            else:
                patch = patch_payload(files, expected)
        if self.mode == "codex":
            reply = codex_apply(root, patch, self.metadata["executable"])
            ok = reply["returncode"] == 0 and not reply["transport_error"]
            calls.append({"tool": "apply_patch", "input": patch, "native_result": reply,
                "returned_text": reply["stdout"] + reply["stderr"], "model_visible_result": None,
                "ok": ok, "elapsed_ms": reply["elapsed_ms"], "timing_scope": "native CLI process including startup"})
        elif self.mode == "aider":
            if probe and self.format == "whole":
                edit = edits[0]
                response = f'{edit["file"]}\n{FENCE}\n{edit["text"]}'
            elif probe:
                edit = edits[0]
                response = f'{edit["file"]}\n{FENCE}\n<<<<<<< SEARCH\n{edit["old"]}=======\n{edit["text"]}>>>>>>> REPLACE\n{FENCE}\n'
            else:
                response = aider_payload(files, expected, self.format)
            reply = self.worker.request({"action": "execute", "format": self.format,
                "root": str(root), "response": response, "chat_files": list(files),
                "line_endings": self.line_endings})
            if reply.get("adapter_error"): raise RuntimeError(dump(reply["adapter_error"]))
            calls.append({"tool": self.format, "input": response, "native_result": reply,
                "returned_text": reply.get("stdout", "") + reply.get("stderr", "") + reply.get("io_output", "") + (reply.get("native_error") or {}).get("message", ""),
                "model_visible_result": None, "ok": reply.get("ok", False),
                "elapsed_ms": reply.get("edit_elapsed_ms"), "timing_scope": "warm native get_edits/apply_edits; setup separate"})
        elif self.mode == "opencode-patch":
            args = {"patchText": patch}
            reply = self.worker.request({"action": "execute", "tool": "apply_patch", "root": str(root), "arguments": args})
            if reply.get("adapter_error") or reply.get("status") == "unavailable":
                raise RuntimeError("adapter failure: " + dump(reply))
            calls.append({"tool": "apply_patch", "input": args, "native_result": reply,
                "returned_text": (reply.get("output") or reply.get("error", "")),
                "model_visible_result": reply.get("output"), "ok": reply.get("ok", False),
                "elapsed_ms": reply.get("elapsed_ms"), "timing_scope": "warm native tool execution"})
        else:
            current = dict(files)
            for edit in edits:
                if self.mode == "xnip":
                    args = {**edit, "file": str(root / edit["file"])}
                    start = time.perf_counter()
                    reply = self.worker.call("xnip_edit", args)
                    elapsed = (time.perf_counter() - start) * 1000
                    ok = not (reply.get("isError") or reply.get("jsonrpc_error"))
                    text = "\n".join(item.get("text", "") for item in reply.get("content", []))
                    if "jsonrpc_error" in reply: text = dump(reply["jsonrpc_error"])
                    visible = None  # MCP host controls whether structuredContent enters model context.
                elif self.mode in ("gemini", "gemini-context"):
                    args = {"file_path": str(root / edit["file"]), "old_string": edit["old"],
                        "new_string": edit["text"], "instruction": purpose}
                    if edit.get("all"): args["expected_replacements"] = current[edit["file"]].count(edit["old"])
                    reply = self.worker.request({"action": "execute", "root": str(root), "arguments": args})
                    elapsed, ok = reply.get("elapsed_ms"), reply.get("ok", False)
                    text = reply.get("model_visible_result") or reply.get("error", "")
                    visible = reply.get("model_visible_result")
                else:
                    args = {"filePath": str(root / edit["file"]), "oldString": edit["old"], "newString": edit["text"]}
                    if edit.get("all"): args["replaceAll"] = True
                    reply = self.worker.request({"action": "execute", "root": str(root), "arguments": args})
                    elapsed, ok = reply.get("elapsed_ms"), reply.get("ok", False)
                    text, visible = (reply.get("output") or reply.get("error", "")), reply.get("output")
                if reply.get("adapter_error") or reply.get("status") == "unavailable":
                    raise RuntimeError("adapter failure: " + dump(reply))
                tool_name = "xnip_edit" if self.mode == "xnip" else "replace" if self.mode.startswith("gemini") else "edit"
                calls.append({"tool": tool_name, "input": args, "native_result": reply,
                    "returned_text": text, "model_visible_result": visible, "ok": ok,
                    "elapsed_ms": elapsed, "timing_scope": "warm tool call; worker initialization excluded"})
                if not ok: break
                current[edit["file"]] = (root / edit["file"]).read_text(encoding="utf8")
        return calls


def record(backend, root, case, counter, repeats=1, probe=False):
    files = case["files"]
    expected = case.get("expected", files)
    edits = case["edits"]
    outcomes, timings, first = [], [], None
    for _ in range(repeats):
        with tempfile.TemporaryDirectory(prefix="case-", dir=root) as temporary:
            workspace = Path(temporary)
            reset_files(workspace, files)
            calls = backend.execute(workspace, files, expected, edits, case["purpose"], probe)
            actual = actual_files(workspace, files)
            result = observe(actual, expected)
            result["native_reported_ok"] = all(call["ok"] for call in calls)
            result["unchanged"] = actual == {name: data.encode("utf8") for name, data in files.items()}
            outcomes.append({**{key: value for key, value in result.items() if key != "files"},
                "file_hashes": {name: item["sha256"] for name, item in result["files"].items()}})
            timings.append(sum(call["elapsed_ms"] or 0 for call in calls))
            if first is None:
                first = {"calls": normalize(calls, workspace), "raw_calls": calls, "outcome": result}
    calls = first["calls"]
    input_cost = counter.total(call["input"] if isinstance(call["input"], str) else dump(call["input"]) for call in calls)
    output_cost = counter.total(call["returned_text"] if isinstance(call["returned_text"], str) else dump(call["returned_text"]) for call in calls)
    repair = [request for call in calls for request in call["native_result"].get("model_requests_blocked", [])]
    return {"backend": backend.name, "family": backend.family, "repeats": repeats,
        "native_reported_ok": all(outcome["native_reported_ok"] for outcome in outcomes),
        "outcomes": outcomes, "stable_outcome": all(item == outcomes[0] for item in outcomes),
        "input_cost": input_cost, "returned_text_cost": output_cost,
        "adapter_return_cost": counter.total(dump(call["native_result"]) for call in calls),
        "model_repair_requests_blocked": len(repair),
        "repair_request_cost": counter.total(dump(item) for item in repair),
        "median_ms": statistics.median(timings), "timings_ms": timings,
        "measurement_scope": "deterministic native editor execution, not an agent/model task",
        "cost_scope": "tool inputs and returned text normalized to /workspace; adapter_return_cost includes instrumentation and is not a model response",
        "repair_request_scope": "one recorded repetition; inference requests are blocked, never billed",
        **first}


def fingerprint():
    paths = sorted([ROOT / "benches/native_tools.py", ROOT / "benches/native_report.py", ROOT / "benches/edit_cost.py",
        *(ROOT / "benches/native").glob("*.py"), *(ROOT / "benches/native").glob("*.mjs"),
        *(ROOT / "benches/native").glob("*.ts"), *(ROOT / "benches/native").glob("*.json")])
    return {str(file.relative_to(ROOT)).replace("\\", "/"): hashlib.sha256(file.read_bytes()).hexdigest() for file in paths}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--xnip", type=Path, default=ROOT / "target/debug" / ("xnip.exe" if os.name == "nt" else "xnip"))
    parser.add_argument("--codex", type=Path)
    parser.add_argument("--aider-python", type=Path, default=ROOT / "target/native-bench/aider-venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python"))
    parser.add_argument("--encoding")
    parser.add_argument("--tokenizer-path", type=Path, default=ROOT / "target/benchmark-python")
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--json", type=Path, default=ROOT / "target/native-bench/results.json")
    parser.add_argument("--markdown", type=Path, default=ROOT / "target/native-bench/results.md")
    parser.add_argument("--report-only", type=Path, metavar="RESULTS_JSON",
        help="regenerate relative statistics, Markdown and SVG charts from an existing native run; no executors or tokenizer required")
    args = parser.parse_args()
    if args.report_only is not None:
        write_report(json.loads(args.report_only.read_text(encoding="utf8")), args.markdown, args.report_only)
        print(f"report regenerated from {args.report_only}; raw results unchanged")
        return 0
    if args.repeats < 1: parser.error("--repeats must be positive")
    if args.tokenizer_path.is_dir(): sys.path.insert(0, str(args.tokenizer_path))
    counter = Counter(args.encoding)
    initial = fingerprint()
    binary_hash = hashlib.sha256(args.xnip.read_bytes()).hexdigest()
    workers, backends, unavailable = [], [], []
    metadata = {}
    try:
        with contextlib.ExitStack() as scope:
            temporary = scope.enter_context(tempfile.TemporaryDirectory(prefix="xnip-native-tool-benchmark-"))
            root = Path(temporary)
            xnip = Mcp(args.xnip.resolve(), root)
            workers.append(xnip)
            scope.callback(xnip.close)
            backends.append(Backend("xnip-edit", "xnip", "xnip", {"version": xnip.initialize["serverInfo"]["version"], "binary_sha256": binary_hash}, xnip))
            codex = codex_discover(args.codex)
            metadata["codex"] = codex
            if codex["status"] == "available": backends.append(Backend("codex-apply-patch", "codex", "codex", codex))
            else: unavailable.append({"name": "Codex apply_patch", "reason": codex.get("reason")})
            for name, command in [
                ("gemini", ["node", str(ROOT / "benches/native/gemini_adapter.mjs")]),
                ("opencode", ["node", str(ROOT / "benches/native/opencode_adapter.mjs")]),
                ("aider", [str(args.aider_python), str(ROOT / "benches/native/aider_adapter.py")]),
            ]:
                try:
                    worker = Worker(command)
                    workers.append(worker)
                    scope.callback(worker.close)
                    info = worker.request({"action": "describe"})
                    metadata[name] = info
                    if info.get("status") != "available": raise RuntimeError(dump(info))
                    if name == "gemini":
                        backends.append(Backend("gemini-edit-context", name, "gemini-context", info, worker))
                        backends.append(Backend("gemini-edit-minimal", name, "gemini", info, worker))
                    if name == "opencode":
                        backends.append(Backend("opencode-edit", name, "opencode", info, worker))
                        backends.append(Backend("opencode-apply-patch", name, "opencode-patch", info, worker))
                    if name == "aider":
                        for format in ("diff", "whole", "udiff"):
                            for endings in ("platform", "lf"):
                                backends.append(Backend(f"aider-{format}-{endings}", name, "aider", info, worker, format, endings))
                except (OSError, RuntimeError, queue.Empty) as error:
                    unavailable.append({"name": name, "reason": str(error)[:1500]})
            unavailable.extend([
                {"name": "Claude Code Edit / Write", "reason": "No independently invocable official native executor was available; not simulated or scored."},
                {"name": "Cursor native edit", "reason": "Installed app has no verified independent native edit entry; not simulated or scored."},
            ])
            cases = []
            for scenario in scenarios():
                case = {"name": scenario.name, "files": scenario.files, "expected": scenario.expected(),
                    "edits": scenario.edits, "purpose": TASKS[scenario.name]}
                print("native case: " + scenario.name, file=sys.stderr, flush=True)
                case["results"] = [record(backend, root, case, counter, args.repeats) for backend in backends]
                cases.append(case)
            observations = []
            for probe in probes():
                case = {**probe, "edits": [probe["edit"]], "purpose": probe["observation"]}
                if probe["name"] == "aider-accepted-unclosed-whole":
                    chosen = [backend for backend in backends if backend.mode == "aider" and backend.format == "whole"]
                else:
                    chosen = [backend for backend in backends if backend.mode != "gemini-context" and not (
                        backend.mode == "aider" and backend.format != "diff") and not (
                        probe["name"] == "overlapping-literal" and backend.mode in ("codex", "opencode-patch", "aider"))]
                case["results"] = [record(backend, root, case, counter, args.repeats, True) for backend in chosen]
                observations.append(case)
            report = {"schema_version": 1, "scope": "paired real native editor execution versus xnip",
                "model_calls_performed": 0, "platform": sys.platform, "python": sys.version, "counter": counter.metadata, "repeats": args.repeats,
                "metadata": metadata, "unavailable": unavailable, "adapter_fingerprints": initial,
                "xnip_binary_sha256": binary_hash,
                "backends": [{"name": backend.name, "family": backend.family,
                    "version": backend.metadata.get("version", "source pin"), "mode": backend.mode,
                    "line_endings": backend.line_endings} for backend in backends],
                "cases": cases, "probes": observations}
    finally:
        for worker in reversed(workers):
            if worker.process.poll() is None: worker.close()
    if initial != fingerprint(): raise RuntimeError("adapter source changed during run; refusing mixed report")
    if binary_hash != hashlib.sha256(args.xnip.read_bytes()).hexdigest(): raise RuntimeError("xnip binary changed during run")
    args.json.parent.mkdir(parents=True, exist_ok=True)
    args.markdown.parent.mkdir(parents=True, exist_ok=True)
    args.json.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf8")
    write_report(report, args.markdown, args.json)
    print(f"native backends: {len(backends)}; tasks: {len(cases)}; repetitions: {args.repeats}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
