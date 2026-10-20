"""Real, offline DSH worker contracts; prepare dsh_setup.mjs explicitly first."""
import hashlib
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
from edit_cost import scenarios

CACHE = ROOT / "target/native-bench/dsh-0.2.1-rc.2"
NODE = os.environ.get("XNIP_DSH_NODE") or shutil.which("node")
BINARY = Path(os.environ.get("XNIP_DSH_TEST_BINARY", str(ROOT / "target/debug" / ("xnip.exe" if os.name == "nt" else "xnip"))))
PREPARED = NODE and (CACHE / "metadata.json").is_file()


class Worker:
    def __init__(self):
        self.messages = queue.Queue()
        self.process = subprocess.Popen(
            [NODE, str(ROOT / "benches/native/dsh_adapter.mjs")], cwd=ROOT,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, encoding="utf8", env={**os.environ, "XNIP_AUTO_UPDATE": "0"},
        )
        self.errors = []
        def read():
            for line in self.process.stdout:
                try:
                    self.messages.put(json.loads(line))
                except ValueError:
                    self.messages.put(RuntimeError("worker stdout is not JSONL: " + line))
            self.messages.put(EOFError("DSH worker exited"))
        def diagnostics():
            self.errors.extend(self.process.stderr)
        self.reader = threading.Thread(target=read, daemon=True)
        self.stderr_reader = threading.Thread(target=diagnostics, daemon=True)
        self.reader.start()
        self.stderr_reader.start()

    def request(self, request):
        self.process.stdin.write(json.dumps(request, ensure_ascii=False) + "\n")
        self.process.stdin.flush()
        reply = self.messages.get(timeout=30)
        if isinstance(reply, BaseException):
            raise reply
        if reply.get("adapter_error"):
            raise AssertionError(reply["adapter_error"])
        return reply

    def close(self):
        if self.process.poll() is None:
            try:
                self.request({"action": "close"})
                self.process.stdin.close()
                self.process.wait(timeout=10)
            finally:
                if self.process.poll() is None:
                    self.process.kill()
                    self.process.wait(timeout=5)
        self.reader.join(timeout=5)
        self.stderr_reader.join(timeout=5)
        self.process.stdout.close()
        self.process.stderr.close()


@unittest.skipUnless(PREPARED, "prepare node benches/native/dsh_setup.mjs explicitly; no implicit install")
class DshWorkerContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.worker = Worker()
        cls.directory = tempfile.TemporaryDirectory(prefix="dsh-contract-", dir=ROOT / "target")
        cls.addClassCleanup(cls.directory.cleanup)
        cls.addClassCleanup(cls.worker.close)

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(dir=self.directory.name))

    def call(self, mode, tool, args, **options):
        return self.worker.request({"action": "execute", "mode": mode, "root": str(self.root),
                                    "tool": tool, "arguments": args, "options": options})

    def file(self, text, name="a.txt"):
        file = self.root / name
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_bytes(text.encode("utf8"))
        return file

    def test_describe_uses_the_actual_case_without_authorizing_a_file(self):
        target = self.file("old")
        description = self.worker.request({"action": "describe-context", "mode": "fs-observed", "root": str(self.root)})
        self.assertEqual([tool["name"] for tool in description["tools"]], ["read", "write", "edit"])
        self.assertIn(json.dumps(str(self.root), ensure_ascii=False), description["runtime_context"])
        rejected = self.call("fs-observed", "edit", {"file_path": str(target), "old_string": "old", "new_string": "new"})
        self.assertFalse(rejected["ok"])
        self.assertIn("FS_NOT_OBSERVED", json.dumps(rejected["native_result"]))
        self.assertEqual(target.read_bytes(), b"old")

    def test_observed_reads_are_real_and_mutations_refresh_the_session_version(self):
        target = self.file("old\r\n")
        self.assertFalse(self.call("fs-observed", "write", {"file_path": str(target), "content": "new"})["ok"])
        read = self.call("fs-observed", "read", {"file_path": str(target), "offset": 1, "limit": 1})
        self.assertTrue(read["ok"])
        self.assertIn("old", read["model_visible_result"])
        for before, after in (("old", "new"), ("new", "final")):
            self.assertTrue(self.call("fs-observed", "edit", {"file_path": str(target), "old_string": before, "new_string": after})["ok"])
        self.assertEqual(target.read_bytes(), b"final\r\n")

    def test_external_change_and_reset_do_not_inherit_or_forge_observations(self):
        target = self.file("old")
        self.assertTrue(self.call("fs-observed", "read", {"file_path": str(target)})["ok"])
        target.write_bytes(b"changed")
        rejected = self.call("fs-observed", "edit", {"file_path": str(target), "old_string": "changed", "new_string": "new"})
        self.assertFalse(rejected["ok"])
        self.assertIn("FS_STALE_VERSION", json.dumps(rejected["native_result"]))
        self.worker.request({"action": "reset", "root": str(self.root), "mode": "fs-observed"})
        rejected = self.call("fs-observed", "edit", {"file_path": str(target), "old_string": "changed", "new_string": "new"})
        self.assertIn("FS_NOT_OBSERVED", json.dumps(rejected["native_result"]))
        self.assertEqual(target.read_bytes(), b"changed")

    def test_native_literal_normalization_overlap_all_and_noop_are_not_simulated(self):
        target = self.file("A\r\nold\nB\n")
        result = self.call("fs-bare", "edit", {"file_path": str(target), "old_string": "old", "new_string": "new"})
        self.assertTrue(result["ok"])
        self.assertEqual(target.read_bytes(), b"A\nnew\nB\n")
        target.write_bytes(b"ababa")
        self.assertTrue(self.call("fs-bare", "edit", {"file_path": str(target), "old_string": "aba", "new_string": "X"})["ok"])
        self.assertEqual(target.read_bytes(), b"Xba")
        target.write_bytes(b"old old")
        args = {"file_path": str(target), "old_string": "old", "new_string": "new"}
        self.assertFalse(self.call("fs-bare", "edit", args)["ok"])
        self.assertTrue(self.call("fs-bare", "edit", {**args, "replace_all": True})["ok"])
        self.assertEqual(target.read_bytes(), b"new new")
        self.assertFalse(self.call("fs-bare", "edit", {"file_path": str(target), "old_string": "new", "new_string": "new"})["ok"])

    def test_native_write_keeps_storage_bytes_separate_from_canonical_diff_basis(self):
        target = self.file("old")
        result = self.call("fs-bare", "write", {"file_path": str(target), "content": "你好\r\nno-final-LF"})
        self.assertTrue(result["ok"])
        self.assertEqual(target.read_bytes(), "你好\r\nno-final-LF".encode("utf8"))
        self.assertEqual(result["value"]["after"], "你好\nno-final-LF")
        self.assertNotIn("no-final-LF", result["model_visible_result"])
        self.assertEqual(result["content"], result["native_result"]["content"])

    def test_str_editor_has_absolute_paths_raw_matching_and_its_own_line_insert_semantics(self):
        target = self.file("one\n")
        self.assertFalse(self.call("str-bare", "str_replace_editor", {"command": "view", "path": "a.txt"})["ok"])
        self.assertTrue(self.call("str-bare", "str_replace_editor", {"command": "insert", "path": str(target),
                                                                   "insert_line": 2, "new_str": "three"})["ok"])
        self.assertEqual(target.read_bytes(), b"one\n\nthree")
        target.write_bytes(b"A\r\nold\n")
        self.assertTrue(self.call("str-bare", "str_replace_editor", {"command": "str_replace", "path": str(target),
                                                                   "old_str": "A\r\n", "new_str": "B\r\n"})["ok"])
        self.assertEqual(target.read_bytes(), b"B\r\nold\n")

    def test_real_workspace_sandbox_denies_an_outside_target(self):
        outside = Path(self.directory.name) / "outside.txt"
        outside.write_bytes(b"old")
        self.assertTrue(self.call("fs-observed", "read", {"file_path": str(outside)})["ok"])
        result = self.call("fs-observed", "edit", {"file_path": str(outside), "old_string": "old", "new_string": "new"})
        self.assertFalse(result["ok"])
        self.assertIn("FS_SANDBOX_DENIED", json.dumps(result["native_result"]))
        self.assertEqual(outside.read_bytes(), b"old")

    def test_all_original_fixtures_execute_through_real_default_and_bare_edit_tools(self):
        for observed in (False, True):
            mode = "fs-observed" if observed else "fs-bare"
            for scenario in scenarios():
                with self.subTest(mode=mode, scenario=scenario.name):
                    self.root = Path(tempfile.mkdtemp(dir=self.directory.name))
                    for name, content in scenario.files.items():
                        self.file(content, name)
                    if observed:
                        for name in scenario.files:
                            self.assertTrue(self.call(mode, "read", {"file_path": name})["ok"])
                    for edit in scenario.edits:
                        args = {"file_path": edit["file"], "old_string": edit["old"], "new_string": edit["text"]}
                        if edit.get("all"):
                            args["replace_all"] = True
                        self.assertTrue(self.call(mode, "edit", args)["ok"])
                    actual = {file.relative_to(self.root).as_posix(): file.read_bytes()
                              for file in self.root.rglob("*") if file.is_file()}
                    self.assertEqual(actual, {name: text.encode("utf8") for name, text in scenario.expected().items()})
                    self.worker.request({"action": "reset", "root": str(self.root), "mode": mode})

    def test_str_editor_default_and_bare_cover_original_goals_without_an_invented_global_flag(self):
        for observed in (False, True):
            mode = "str-observed" if observed else "str-bare"
            for scenario in scenarios():
                with self.subTest(mode=mode, scenario=scenario.name):
                    self.root = Path(tempfile.mkdtemp(dir=self.directory.name))
                    for name, content in scenario.files.items():
                        self.file(content, name)
                    if observed:
                        for name in scenario.files:
                            self.assertTrue(self.call(mode, "str_replace_editor", {"command": "view", "path": str(self.root / name)})["ok"])
                    if any(edit.get("all") for edit in scenario.edits):
                        # This real tool has no global flag: a unique complete
                        # supplied old file is a different, explicit candidate.
                        edits = [{"file": name, "old": before, "text": scenario.expected()[name]}
                                 for name, before in scenario.files.items() if before != scenario.expected()[name]]
                    else:
                        edits = scenario.edits
                    for edit in edits:
                        result = self.call(mode, "str_replace_editor", {
                            "command": "str_replace", "path": str(self.root / edit["file"]),
                            "old_str": edit["old"], "new_str": edit["text"],
                        })
                        self.assertTrue(result["ok"], result)
                    actual = {file.relative_to(self.root).as_posix(): file.read_bytes()
                              for file in self.root.rglob("*") if file.is_file()}
                    self.assertEqual(actual, {name: text.encode("utf8") for name, text in scenario.expected().items()})
                    self.worker.request({"action": "reset", "root": str(self.root), "mode": mode})

    @unittest.skipUnless(BINARY.is_file(), "provide XNIP_DSH_TEST_BINARY for a real bridge test")
    def test_official_mcp_projection_is_real_and_does_not_echo_structured_metadata(self):
        target = self.file("old")
        binary_before = hashlib.sha256(BINARY.read_bytes()).hexdigest()
        description = self.worker.request({"action": "describe-context", "mode": "xnip-mcp", "root": str(self.root),
                                           "options": {"xnipExecutable": str(BINARY.resolve()), "receipts": "structured"}})
        self.assertEqual(len(description["tools"]), 5)
        self.assertTrue(all("outputSchema" not in tool for tool in description["tools"]))
        result = self.call("xnip-mcp", "xnip_edit", {"file": str(target), "old": "old", "text": "new"},
                           xnipExecutable=str(BINARY.resolve()), receipts="structured")
        self.assertTrue(result["ok"])
        self.assertEqual(result["tool_name"], "mcp__xnip__xnip_edit")
        self.assertEqual(result["model_visible_result"], "written")
        self.assertTrue(result["value"]["structuredContent"]["written"])
        self.assertEqual(target.read_bytes(), b"new")
        self.assertEqual(hashlib.sha256(BINARY.read_bytes()).hexdigest(), binary_before)


if __name__ == "__main__":
    unittest.main()
