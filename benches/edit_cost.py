#!/usr/bin/env python3
"""Horizontal edit representation and real xnip execution benchmark; no model calls.

Standard library suffices. Optional --encoding uses installed tiktoken.
Without it, exact bytes/characters are reported and token counts are null.
"""
from __future__ import annotations

import argparse
from collections import OrderedDict
import dataclasses
import difflib
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import platform
import queue
import statistics
import subprocess
import sys
import tempfile
from report_privacy import fixture_session, public_report
import threading
import time
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
FENCE = chr(96) * 3
PROMPT_PATHS = (
    "docs/SKILL.md",
    "integrations/generic/SKILL.md",
    "integrations/claude-code/SKILL.md",
    "integrations/codex/AGENTS.md",
    "integrations/codex/guide.md",
    "integrations/agents-md/AGENTS.md",
    "integrations/aider/CONVENTIONS.md",
    "integrations/copilot/xnip.md",
    "integrations/cursor/xnip.mdc",
)


def execution_environment():
    """Keep measurements isolated from background network and binary replacement."""
    return {**os.environ, "XNIP_AUTO_UPDATE": "0"}


def dump(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


class Counter:
    def __init__(self, encoding: str | None, *, cache_entries: int = 2048,
                 cache_text_bytes: int = 8 * 1024 * 1024):
        if cache_entries < 0 or cache_text_bytes < 0:
            raise ValueError("counter cache limits must be nonnegative")
        self._cache = OrderedDict()
        self._cache_entries, self._cache_text_bytes = cache_entries, cache_text_bytes
        self._cached_text_bytes = 0
        self.encoding = None
        self.metadata = {"unit": "utf8_bytes", "tokenizer": None, "encoding": None}
        if encoding:
            try:
                import tiktoken
            except ImportError as error:
                raise SystemExit("--encoding requires optional tiktoken; omit for bytes-only") from error
            self.encoding = tiktoken.get_encoding(encoding)
            self.metadata = {
                "unit": "tokens", "tokenizer": "tiktoken",
                "version": importlib.metadata.version("tiktoken"), "encoding": encoding,
            }

    def measure(self, text: str) -> dict:
        # Each Counter owns one encoding. Cache exact strings, never joined calls
        # or normalized Unicode; return fresh dictionaries so callers cannot
        # corrupt later results. Text storage and entry count are both bounded.
        cached = self._cache.get(text)
        if cached is not None:
            counts, _ = cached
            self._cache.move_to_end(text)
        else:
            counts = (len(text.encode("utf-8")), len(text),
                      len(self.encoding.encode(text, disallowed_special=())) if self.encoding else None)
            size = sys.getsizeof(text)
            if self._cache_entries and size <= self._cache_text_bytes:
                while self._cache and (len(self._cache) >= self._cache_entries
                                       or self._cached_text_bytes + size > self._cache_text_bytes):
                    _, (_, removed_size) = self._cache.popitem(last=False)
                    self._cached_text_bytes -= removed_size
                self._cache[text] = (counts, size)
                self._cached_text_bytes += size
        return {"utf8_bytes": counts[0], "characters": counts[1], "tokens": counts[2]}

    def total(self, texts) -> dict:
        parts = [self.measure(text) for text in texts]
        return {key: sum(part[key] for part in parts) if key != "tokens" or self.encoding else None
                for key in ("utf8_bytes", "characters", "tokens")}


class Mcp:
    """Line-delimited JSON-RPC with response timeout and bounded teardown."""

    def __init__(self, binary: Path, cwd: Path, full: bool = False, receipts: str = "structured"):
        self.binary = binary
        self.diagnostics = tempfile.TemporaryFile()
        self.process = subprocess.Popen(
            [str(binary), "mcp", *(["--toolset", "full"] if full else []),
             *(["--receipts", receipts] if receipts != "structured" else [])],
            cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=self.diagnostics, text=True, encoding="utf-8", bufsize=1,
            env=execution_environment(),
        )
        self.messages: queue.Queue = queue.Queue()
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()
        self.next_id = 0
        try:
            self.initialize = self.request("initialize", {
                "protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "xnip-edit-cost", "version": "1"},
            })
            self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        except BaseException as error:
            self.close()
            raise RuntimeError(f"MCP startup failed for {self.binary}: {self.stderr_text or error}") from error

    def _read(self):
        try:
            for line in self.process.stdout:
                self.messages.put(json.loads(line))
        except BaseException as error:
            self.messages.put(error)
        finally:
            self.messages.put(EOFError("MCP server stopped before responding"))

    def _send(self, value):
        self.last_request = value
        self.process.stdin.write(dump(value) + "\n")
        self.process.stdin.flush()

    def request(self, method, params):
        self.next_id += 1
        current = self.next_id
        self._send({"jsonrpc": "2.0", "id": current, "method": method, "params": params})
        deadline = time.monotonic() + 15
        while True:
            response = self.messages.get(timeout=max(0.001, deadline - time.monotonic()))
            if isinstance(response, BaseException):
                raise response
            if response.get("id") == current:
                self.last_response = response
                return {"jsonrpc_error": response["error"]} if "error" in response else response["result"]

    def call(self, name, arguments):
        return self.request("tools/call", {"name": name, "arguments": arguments})

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=5)
        self.reader.join(timeout=5)
        self.diagnostics.seek(0)
        self.stderr_text = self.diagnostics.read().decode("utf-8", errors="replace")
        self.diagnostics.close()
        for stream in (self.process.stdin, self.process.stdout):
            if stream:
                stream.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


@dataclasses.dataclass(frozen=True)
class Scenario:
    name: str
    files: dict[str, str]
    edits: list[dict]
    purpose: str

    def expected(self):
        output = dict(self.files)
        for edit in self.edits:
            old = edit["old"]
            count = output[edit["file"]].count(old)
            if count != 1 and not edit.get("all"):
                raise ValueError(f"fixture {self.name}: old block is not unique")
            output[edit["file"]] = output[edit["file"]].replace(
                old, edit["text"], -1 if edit.get("all") else 1)
        return output


def scenarios():
    def one(name, before, old, new, purpose, all_matches=False):
        edit = {"file": "src/example.txt", "old": old, "text": new}
        if all_matches:
            edit["all"] = True
        return Scenario(name, {"src/example.txt": before}, [edit], purpose)

    large = "".join(f"const item_{i:04} = {i};\n" for i in range(2000))
    return [
        one("tiny-config", "host=localhost\nport=8080\nsecure=false\n",
            "port=8080", "port=3000", "small config, known text"),
        one("large-local-change", large, "const item_1000 = 1000;",
            "const item_1000 = 42;", "small change in a 2,000-line file"),
        one("multiline-function", "def total(items):\n    result = 0\n    for item in items:\n        result += item.price\n    return result\n\nprint(total([]))\n",
            "    result = 0\n    for item in items:\n        result += item.price\n    return result",
            "    return sum(item.price for item in items)", "multi-line deletion and replacement"),
        one("unicode", "标题 = '编辑器'\nmessage = '你好，世界 🌏'\n",
            "你好，世界 🌏", "高效编辑 🚀", "multibyte source and replacement"),
        one("crlf", "host=localhost\r\nport=8080\r\nsecure=false\r\n",
            "port=8080", "port=3000", "preserve CRLF byte-for-byte"),
        one("no-final-newline", "host=localhost\nport=8080", "port=8080",
            "port=3000", "preserve missing final newline"),
        one("disambiguated-block", "def first():\n    return 0\n\ndef second():\n    return 0\n",
            "def second():\n    return 0", "def second():\n    return 1",
            "surrounding context selects one repeated line"),
        one("global-replace", "old_name()\n" * 20, "old_name", "new_name",
            "intentional repeated literal replacement", True),
        Scenario("same-file-batch", {"src/example.txt": "one=1\ntwo=2\nthree=3\n"},
            [{"file": "src/example.txt", "old": "one=1", "text": "one=10"},
             {"file": "src/example.txt", "old": "three=3", "text": "three=30"}],
            "two disjoint blocks in one call"),
        Scenario("multi-file-batch", {"src/a.txt": "enabled=false\n", "src/b.txt": "timeout=10\n", "src/c.txt": "retries=1\n"},
            [{"file": "src/a.txt", "old": "false", "text": "true"},
             {"file": "src/b.txt", "old": "10", "text": "20"},
             {"file": "src/c.txt", "old": "1", "text": "3"}],
            "three files in one call"),
        one("long-line", "data=" + "x" * 40000 + ";mode=old\n", "mode=old",
            "mode=new", "small substring in a long line"),
        one("insert-after-anchor", "import os\n\nprint(os.name)\n", "import os\n",
            "import os\nimport sys\n", "insert using a known literal anchor"),
        one("delete-block", "start\n# obsolete\nold_setting=true\nend\n",
            "# obsolete\nold_setting=true\n", "", "delete a known block"),
    ]


def changed_lines(before, after, path):
    """Original-file coordinates, descending to avoid shifting later edits."""
    a, b = before.splitlines(keepends=True), after.splitlines(keepends=True)
    edits = []
    for tag, i, j, k, l in difflib.SequenceMatcher(a=a, b=b, autojunk=False).get_opcodes():
        if tag == "equal":
            continue
        if i == j:
            edits.append({"op": "insert", "file": path, "lines": str(max(1, i)),
                          "where": "after" if i else "before", "text": "".join(b[k:l])})
        else:
            edits.append({"op": "replace", "file": path,
                          "lines": str(i + 1) if j == i + 1 else f"{i + 1}-{j}",
                          "was": "".join(a[i:j]), "text": "".join(b[k:l])})
    return list(reversed(edits))


def representations(scenario):
    expected = scenario.expected()
    full, exact, aider, unified, patch, lines = [], [], [], [], ["*** Begin Patch\n"], []
    for path in sorted(scenario.files):
        before, after = scenario.files[path], expected[path]
        if before == after:
            continue
        full.append({"path": path, "content": after})
        hunks = list(difflib.unified_diff(before.splitlines(keepends=True),
                     after.splitlines(keepends=True), fromfile="a/" + path,
                     tofile="b/" + path, n=3))
        unified.extend(hunks)
        patch.append(f"*** Update File: {path}\n")
        for line in hunks[2:]:
            patch.append("@@\n" if line.startswith("@@") else line)
        lines.extend(changed_lines(before, after, path))
        # SEARCH/REPLACE markers require line-aligned blocks. Charge actual
        # surrounding lines rather than pretending arbitrary substrings are lines.
        a, b = before.splitlines(keepends=True), after.splitlines(keepends=True)
        groups = difflib.SequenceMatcher(a=a, b=b, autojunk=False).get_grouped_opcodes(3)
        for group in groups:
            old_block = "".join(a[group[0][1]:group[-1][2]])
            new_block = "".join(b[group[0][3]:group[-1][4]])
            aider.append(f"{path}\n{FENCE}\n<<<<<<< SEARCH\n{old_block}=======\n{new_block}>>>>>>> REPLACE\n{FENCE}\n")
    patch.append("*** End Patch\n")
    for edit in scenario.edits:
        item = {"file_path": edit["file"], "old_string": edit["old"], "new_string": edit["text"]}
        if edit.get("all"):
            item["replace_all"] = True
        exact.append(item)
    guarded_files = {}
    for name in scenario.files:
        edits = [edit for edit in scenario.edits if edit["file"] == name]
        guarded_files[name] = ({"old": scenario.files[name], "text": expected[name]}
                               if any(edit.get("all") for edit in edits) else
                               [[edit["old"], edit["text"]] for edit in edits])
    return {
        "whole-file-json": dump(full), "exact-replacement-json": dump(exact),
        "aider-search-replace": "".join(aider), "unified-diff": "".join(unified),
        "apply-patch": "".join(patch), "line-edits-with-preconditions": dump(lines),
        "xnip-single-edits": "\n".join(dump(edit) for edit in scenario.edits),
        "xnip-direct-edits": dump({"edits": scenario.edits}),
        "xnip-native-manifest": "".join(f'edit {dump(e["file"])} {dump(e["old"])} {dump(e["text"])}' + (" all" if e.get("all") else "") + "\n" for e in scenario.edits),
        "xnip-files-guarded": dump({"files": guarded_files}),
        "xnip-files-snapshot": dump({"files": expected}),
        "xnip-write-snapshot": "\n".join(dump({"file": name, "operation": "write", "text": text}) for name, text in expected.items()),
        "xnip-compact-guarded": dump(guarded_files),
        "xnip-compact-snapshot": dump(expected),
    }


def put_files(directory, files):
    for name, content in files.items():
        path = directory / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content.encode("utf-8"))


def assert_files(directory, expected):
    for name, content in expected.items():
        if (directory / name).read_bytes() != content.encode("utf-8"):
            raise AssertionError(f"byte mismatch: {name}")


def is_error(response):
    return bool(response.get("isError") or response.get("jsonrpc_error"))


def normalize(value, directory):
    return dump(value).replace(str(directory).replace("\\", "\\\\"), "/workspace").replace(str(directory), "/workspace")


def measure_scenario(scenario, binary, counter, repeats):
    payloads, expected = representations(scenario), scenario.expected()
    independent_calls = {
        "xnip-single-edits": scenario.edits,
        "xnip-write-snapshot": [{"file": name, "operation": "write", "text": text} for name, text in expected.items()],
    }
    patch_supported = all("\r" not in text and text.endswith("\n")
                          for text in [*scenario.files.values(), *expected.values()])
    serializations = [{
        "format": name, "payload": payload,
        "cost": counter.total(dump(edit) for edit in independent_calls[name]) if name in independent_calls else counter.measure(payload),
        "contract_cohort": "snapshot" if "snapshot" in name or name == "whole-file-json" else "guarded",
        "supported": patch_supported if name in ("unified-diff", "apply-patch", "aider-search-replace") else True,
        "execution": "xnip-parser" if name in ("unified-diff", "apply-patch", "line-edits-with-preconditions", "xnip-native-manifest", "xnip-compact-guarded", "xnip-compact-snapshot") else (
            "xnip-mcp" if name.startswith("xnip-") else "serialization-only"),
    } for name, payload in payloads.items()]
    executions = []
    formats = ["xnip-single-edits", "xnip-direct-edits", "line-edits-with-preconditions", "xnip-native-manifest",
               "xnip-files-guarded", "xnip-files-snapshot", "xnip-write-snapshot",
               "xnip-compact-guarded", "xnip-compact-snapshot"]
    if patch_supported:
        formats += ["unified-diff", "apply-patch"]
    with fixture_session("xnip-cost-") as temp:
        directory = Path(temp)
        with Mcp(binary, directory) as mcp:
            for name in formats:
                elapsed, response = [], None
                for _ in range(repeats):
                    put_files(directory, scenario.files)
                    start = time.perf_counter()
                    if name in independent_calls:
                        replies = [mcp.call("xnip_edit", edit) for edit in independent_calls[name]]
                        response = {"calls": replies, "isError": any(is_error(item) for item in replies)}
                    elif name == "xnip-direct-edits":
                        response = mcp.call("xnip_apply", {"edits": scenario.edits})
                    elif name in ("xnip-files-guarded", "xnip-files-snapshot"):
                        response = mcp.call("xnip_apply", json.loads(payloads[name]))
                    else:
                        fmt = "compact" if name.startswith("xnip-compact-") else ("native" if name == "xnip-native-manifest" else ("json" if name == "line-edits-with-preconditions" else "patch"))
                        command = [str(binary), "apply", "--from-stdin", "--format", fmt]
                        if fmt == "patch":
                            command += ["--patch-root", str(directory)]
                        result = subprocess.run(command, input=payloads[name].encode("utf-8"),
                                                cwd=directory, capture_output=True, timeout=15,
                                                env=execution_environment())
                        response = {"exit": result.returncode, "stdout": result.stdout.decode("utf-8"),
                                    "stderr": result.stderr.decode("utf-8")}
                    elapsed.append((time.perf_counter() - start) * 1000)
                    if is_error(response) or response.get("exit", 0) != 0:
                        raise AssertionError(f"{scenario.name}/{name}: {response}")
                    assert_files(directory, expected)
                executions.append({
                    "format": name, "status": "expected-bytes-verified", "repeats": repeats,
                    "median_ms": round(statistics.median(elapsed), 3),
                    "response": json.loads(normalize(response, directory)),
                    "response_cost": counter.total(normalize(item, directory) for item in response["calls"]) if name in independent_calls else counter.measure(normalize(response, directory)),
                    "payload_cost": counter.total(dump(edit) for edit in independent_calls[name]) if name in independent_calls else counter.measure(payloads[name]),
                    "contract_cohort": "snapshot" if "snapshot" in name else "guarded",
                    "timing_scope": "persistent MCP call(s)" if name in ("xnip-single-edits", "xnip-direct-edits", "xnip-files-guarded", "xnip-files-snapshot", "xnip-write-snapshot") else "CLI process including startup",
                })
    known_context = [e["old"] for e in scenario.edits]
    full_context = [scenario.files[name] for name in sorted(scenario.files)]
    return {"name": scenario.name, "purpose": scenario.purpose,
            "files": scenario.files, "expected": expected,
            "context": {"known_blocks": counter.total(known_context),
                        "full_files": counter.total(full_context),
                        "assumption": "sum of independently counted content blocks, charged once; discovery and framing excluded"},
            "serializations": serializations, "executions": executions}


def rejection_and_retry(binary, counter):
    """Injected bad calls, not probabilistic model failure rates."""
    trials = [
        ("ambiguous-old", "a=1\nb=1\n", {"old": "=1", "text": "=2"},
         {"old": "b=1", "text": "b=2"}, "a=1\nb=2\n"),
        ("stale-old", "version=2\n", {"old": "version=1", "text": "version=3"},
         {"old": "version=2", "text": "version=3"}, "version=3\n"),
        ("overlapping-old", "ababa\n", {"old": "aba", "text": "X"},
         {"old": "ababa", "text": "Xba"}, "Xba\n"),
    ]
    results = []
    with fixture_session("xnip-retry-") as temp:
        directory = Path(temp)
        with Mcp(binary, directory) as mcp:
            for name, original, bad, good, expected in trials:
                put_files(directory, {"src/example.txt": original})
                failed = mcp.call("xnip_edit", {"file": "src/example.txt", **bad})
                if not is_error(failed):
                    raise AssertionError(f"{name}: expected a rejected call")
                assert_files(directory, {"src/example.txt": original})
                observed = mcp.call("xnip_peek", {"file": "src/example.txt", "all": True})
                if is_error(observed):
                    raise AssertionError(f"{name}: retry context read failed: {observed}")
                fixed = mcp.call("xnip_edit", {"file": "src/example.txt", **good})
                if is_error(fixed):
                    raise AssertionError(f"{name}: retry failed: {fixed}")
                assert_files(directory, {"src/example.txt": expected})
                transcript = [
                    {"tool": "xnip_edit", "arguments": {"file": "src/example.txt", **bad}, "result": failed},
                    {"tool": "xnip_peek", "arguments": {"file": "src/example.txt", "all": True}, "result": observed},
                    {"tool": "xnip_edit", "arguments": {"file": "src/example.txt", **good}, "result": fixed},
                ]
                results.append({"name": name, "injected_failures": 1, "retries": 1,
                                "unchanged_after_failure": True, "expected_bytes_verified": True,
                                "transcript": transcript, "total_cost": counter.measure(normalize(transcript, directory))})
    return results


def lifecycle(binary, counter):
    with fixture_session("xnip-lifecycle-") as temp:
        directory = Path(temp)
        with Mcp(binary, directory) as mcp:
            transcript = []
            for arguments in [
                {"file": "a.txt", "operation": "create", "text": "你好\r\n"},
                {"file": "a.txt", "operation": "rename", "to": "nested/b.txt"},
                {"file": "nested/b.txt", "old": "你好", "text": "完成"},
                {"file": "nested/b.txt", "operation": "delete", "old": "完成\r\n"},
            ]:
                result = mcp.call("xnip_edit", arguments)
                if is_error(result):
                    raise AssertionError(f"lifecycle failed: {result}")
                transcript.append({"arguments": arguments, "result": result})
                if arguments.get("operation") == "create":
                    assert_files(directory, {"a.txt": "你好\r\n"})
                if arguments.get("operation") == "rename":
                    if (directory / "a.txt").exists():
                        raise AssertionError("rename left the original path")
                    assert_files(directory, {"nested/b.txt": "你好\r\n"})
                if "old" in arguments and arguments.get("operation") != "delete":
                    assert_files(directory, {"nested/b.txt": "完成\r\n"})
            if any(path.is_file() for path in directory.rglob("*")):
                raise AssertionError("delete left an unexpected file")
    return {"name": "create-rename-edit-delete", "expected_bytes_verified": True,
            "transcript": transcript, "cost": counter.measure(normalize(transcript, directory))}


def output_probes(binary, counter):
    """Actual bounded and explicitly unbounded output, without changing files."""
    rows = []
    with fixture_session("xnip-output-") as temp:
        directory = Path(temp)
        source = "".join(f"needle-{i:04} " + "read context " * 5 + "\n" for i in range(400))
        long_line = "data=" + "x" * 20000 + ";mode=old\n"
        put_files(directory, {"read.txt": source, "preview.txt": long_line})
        with Mcp(binary, directory) as mcp:
            probes = [
                ("peek-default", "xnip_peek", {"file": "read.txt", "all": True}),
                ("peek-unlimited", "xnip_peek", {"file": "read.txt", "all": True, "max_lines": 0, "max_bytes": 0}),
                ("find-default", "xnip_find", {"files": ["read.txt"], "pattern": "needle"}),
                ("find-unlimited", "xnip_find", {"files": ["read.txt"], "pattern": "needle", "max_matches": 0, "max_bytes": 0}),
                ("preview-default", "xnip_edit", {"file": "preview.txt", "old": "mode=old", "text": "mode=new", "dry_run": True}),
                ("preview-unlimited", "xnip_edit", {"file": "preview.txt", "old": "mode=old", "text": "mode=new", "dry_run": True, "max_bytes": 0}),
            ]
            for name, tool, arguments in probes:
                result = mcp.call(tool, arguments)
                if is_error(result):
                    raise AssertionError(f"output probe {name}: {result}")
                assert_files(directory, {"read.txt": source, "preview.txt": long_line})
                rows.append({"name": name, "tool": tool, "arguments": arguments,
                             "result": json.loads(normalize(result, directory)),
                             "cost": counter.measure(normalize(result, directory))})
        for mode in ("compact", "human", "json"):
            result = subprocess.run(
                [str(binary), "--output", mode, "edit", "preview.txt",
                 "--old", "mode=old", "--text", "mode=new", "--dry-run"],
                cwd=directory, capture_output=True, timeout=15,
                env=execution_environment())
            if result.returncode:
                raise AssertionError(f"CLI output {mode}: {result.stderr}")
            assert_files(directory, {"preview.txt": long_line})
            response = {"exit": result.returncode, "stdout": result.stdout.decode("utf-8"),
                        "stderr": result.stderr.decode("utf-8")}
            rows.append({"name": "cli-preview-" + mode,
                         "result": json.loads(normalize(response, directory)),
                         "cost": counter.measure(normalize(response, directory))})
    return rows


def schema_cost(binary, counter):
    result = []
    for full, receipts in ((False, "structured"), (True, "structured"), (False, "text")):
        with Mcp(binary, ROOT, full=full, receipts=receipts) as mcp:
            listing = mcp.request("tools/list", {})
            if "tools" not in listing:
                raise AssertionError(f"tools/list failed: {listing}")
            payload = {"instructions": mcp.initialize.get("instructions", ""), "tools": listing["tools"]}
            result.append({"toolset": "full" if full else "compact", "receipts": receipts,
                           "tools": [tool["name"] for tool in listing["tools"]],
                           "cost": counter.measure(dump(payload)), "payload": payload})
    return result


def prompt_costs(counter):
    rows = []
    for name in PROMPT_PATHS:
        raw = (ROOT / name).read_bytes()
        text = raw.decode("utf-8")
        rows.append({"path": name, "sha256": hashlib.sha256(raw).hexdigest(),
                     "text": text, "cost": counter.measure(text)})
    return rows


def render(report):
    unit = report["counter"]["unit"]
    value = lambda cost: cost[unit]
    output = [
        "# Supplementary representation cost benchmark", "",
        f"Counter: {dump(report['counter'])}. Revision: {report['git_commit']}; modified source: {report['modified_source']}.",
        f"Working-tree binary SHA-256: {report['binary_sha256']} (unchanged across run).",
        f"Source tree SHA-256: {report['source_tree']['sha256']}; includes tracked and untracked build/instruction inputs. Individual file hashes are in JSON.",
        "Comparison scope: current xnip options and harness editing representations; no historical versions.",
        "Background auto-update is disabled for every xnip subprocess with XNIP_AUTO_UPDATE=0.",
        "All xnip execution rows check final bytes. Other harness formats are serialization examples; no external agent/model was run.",
        "Token counts, when enabled, are exact for the named tokenizer and serialized text. They exclude provider billing, reasoning, cached input, and hidden harness prompts.", "",
        "| MCP catalog | Tools | Cost |", "|---|---:|---:|",
    ]
    for catalog in report["catalogs"]:
        output.append(f"| {catalog['toolset']} / {catalog.get('receipts', 'structured')} | {len(catalog['tools'])} | {value(catalog['cost'])} |")
    output += ["", "| Current instruction entry point | Cost |", "|---|---:|"]
    for prompt in report["prompts"]:
        output.append(f"| {prompt['path']} | {value(prompt['cost'])} |")
    output += ["", "Instruction entry points have different installation roles; references loaded on demand are excluded."]
    output += ["", "Payload costs exclude context and responses; JSON records all three separately.",
               "", "| Scenario | Whole file | Exact JSON | Aider blocks | Unified | apply_patch | Lines | xnip single | xnip batch | Native |",
               "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|"]
    for scenario in report["scenarios"]:
        legacy_formats = {"whole-file-json", "exact-replacement-json", "aider-search-replace", "unified-diff", "apply-patch",
                          "line-edits-with-preconditions", "xnip-single-edits", "xnip-direct-edits", "xnip-native-manifest"}
        cells = [str(value(item["cost"])) if item["supported"] else "unsupported"
                 for item in scenario["serializations"] if item["format"] in legacy_formats]
        output.append("| " + scenario["name"] + " | " + " | ".join(cells) + " |")
    output += ["", "| Compact interface | MCP files guarded | MCP files snapshot | MCP write snapshot | CLI compact guarded | CLI compact snapshot |",
               "|---|---:|---:|---:|---:|---:|"]
    compact_names = ("xnip-files-guarded", "xnip-files-snapshot", "xnip-write-snapshot", "xnip-compact-guarded", "xnip-compact-snapshot")
    for scenario in report["scenarios"]:
        costs = {item["format"]: item["cost"] for item in scenario["serializations"]}
        output.append("| " + scenario["name"] + " | " + " | ".join(str(value(costs[name])) for name in compact_names) + " |")
    output += ["", "Guarded maps retain old-content checks. Snapshot variants explicitly allow whole-file writes; these rows have different precondition contracts.", ""]
    output += ["", "| Current output mode / budget | Cost |", "|---|---:|"]
    for probe in report["output_probes"]:
        output.append(f"| {probe['name']} | {value(probe['cost'])} |")
    output += ["", "| Injected failure + read + retry | Total cost |", "|---|---:|"]
    for retry in report["failure_retries"]:
        output.append(f"| {retry['name']} | {value(retry['total_cost'])} |")
    output += ["", "| xnip batch accounting | Known context | Full context | Payload | Result |",
               "|---|---:|---:|---:|---:|"]
    for scenario in report["scenarios"]:
        payload = next(x["cost"] for x in scenario["serializations"] if x["format"] == "xnip-direct-edits")
        response = next(x["response_cost"] for x in scenario["executions"] if x["format"] == "xnip-direct-edits")
        output.append(f"| {scenario['name']} | {value(scenario['context']['known_blocks'])} | {value(scenario['context']['full_files'])} | {value(payload)} | {value(response)} |")
    verified = sum(len(item["executions"]) for item in report["scenarios"])
    output += ["", f"{verified} xnip execution cases passed per repetition; repeats={report['repeats']}.",
               f"{len(report['failure_retries'])} injected rejection/read/retry cases and lifecycle bytes passed.",
               "", "Timing includes local scheduling and storage; CLI startup and persistent MCP have different scopes.",
               "The fixtures include cases where xnip loses on payload size. Format size does not predict model reliability.",
               "JSON includes fixture bytes, generated payloads, actual responses, schema snapshots and retry transcripts.",
        "Sessions use repository target/native-bench/sessions. Public display metadata replaces machine HOME prefixes with `<HOME>` after measurement; the disclosure_redaction ledger retains original-string hashes. Fixture bytes, serialized payloads and tool inputs are unchanged, and all costs use pre-redaction strings.", ""]
    return "\n".join(output)


def source_fingerprint():
    """Fingerprint current build and instruction inputs, including untracked files."""
    paths = set(ROOT.glob("src/**/*.rs")) | set(ROOT.glob("xtask/src/**/*.rs"))
    paths.update(ROOT / name for name in (
        "Cargo.toml", "Cargo.lock", "build.rs", "xtask/Cargo.toml",
        "rust-toolchain.toml", "benches/edit_cost.py", "benches/report_privacy.py", *PROMPT_PATHS,
    ))
    files, aggregate = [], hashlib.sha256()
    for path in sorted(paths, key=lambda value: value.relative_to(ROOT).as_posix()):
        name = path.relative_to(ROOT).as_posix()
        body, encoded_name = path.read_bytes(), name.encode("utf-8")
        aggregate.update(len(encoded_name).to_bytes(8, "big"))
        aggregate.update(encoded_name)
        aggregate.update(len(body).to_bytes(8, "big"))
        aggregate.update(body)
        files.append({"path": name, "bytes": len(body),
                      "sha256": hashlib.sha256(body).hexdigest()})
    return {"algorithm": "sha256(sorted length-prefixed UTF-8 path and raw bytes)",
            "sha256": aggregate.hexdigest(), "files": files}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--xnip", type=Path, default=ROOT / "target" / "debug" / ("xnip.exe" if os.name == "nt" else "xnip"))
    parser.add_argument("--encoding", help="optional tiktoken encoding, e.g. o200k_base")
    parser.add_argument("--tokenizer-path", type=Path, default=ROOT / "target/benchmark-python", help="optional private Python dependency directory")
    parser.add_argument("--repeats", type=int, default=1)
    parser.add_argument("--json", type=Path, help="JSON artifact path")
    parser.add_argument("--markdown", type=Path, help="human report path")
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be >= 1")
    binary = args.xnip.resolve()
    if not binary.is_file():
        parser.error("xnip binary missing; run cargo build first")
    if args.tokenizer_path.is_dir():
        sys.path.insert(0, str(args.tokenizer_path))
    counter = Counter(args.encoding)
    binary_before = hashlib.sha256(binary.read_bytes()).hexdigest()
    source_before = source_fingerprint()
    catalogs = schema_cost(binary, counter)
    report = {
        "schema_version": 2, "counter": counter.metadata,
        "comparison": "current-options-and-harness-representations",
        "auto_update": {"environment": {"XNIP_AUTO_UPDATE": "0"}, "scope": "all xnip subprocesses"},
        "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "git_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "modified_source": bool(subprocess.check_output(["git", "diff", "--", "src"], cwd=ROOT)),
        "source_diff_sha256": hashlib.sha256(subprocess.check_output(["git", "diff", "--", "src"], cwd=ROOT)).hexdigest(),
        "tracked_worktree_diff_sha256": hashlib.sha256(subprocess.check_output(["git", "diff", "--"], cwd=ROOT)).hexdigest(),
        "platform": platform.platform(), "python": platform.python_version(),
        "binary_sha256": binary_before, "repeats": args.repeats,
        "source_tree": source_before,
        "accounting": "Exact serialized text only; no chat template, caching, provider billing, model generation or undisclosed prompts. Context, payload, response and retries separate; known/full context are alternative assumptions.",
        "catalogs": catalogs,
        "prompts": prompt_costs(counter),
        "scenarios": [measure_scenario(s, binary, counter, args.repeats) for s in scenarios()],
        "failure_retries": rejection_and_retry(binary, counter), "lifecycle": lifecycle(binary, counter),
        "output_probes": output_probes(binary, counter),
    }
    report["binary_sha256_verified"] = hashlib.sha256(binary.read_bytes()).hexdigest()
    if report["binary_sha256_verified"] != binary_before:
        raise RuntimeError("xnip binary changed during the benchmark; rerun after the build finishes")
    if source_fingerprint() != source_before:
        raise RuntimeError("source inputs changed during the benchmark; rerun after edits finish")
    report = public_report(report)
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    markdown = render(report)
    if args.markdown:
        args.markdown.parent.mkdir(parents=True, exist_ok=True)
        args.markdown.write_text(markdown, encoding="utf-8")
    print(markdown)
    return 0


if __name__ == "__main__":
    sys.exit(main())
