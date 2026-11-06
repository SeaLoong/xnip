"""Offline byte contracts for CLI, MCP and the native-host JSONL frontend.

Run after `cargo build --locked`: python3 tests/native_byte_contract.py
XNIP_TEST_BINARY optionally selects an already-built executable. These are real
local transport executions, not model/agent sessions or simulated host tools.
The unchanged thirteen benchmark scenarios and their independent expected-byte
oracle come from benches/edit_cost.py.
"""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
from edit_cost import Mcp, dump, execution_environment, scenarios


def snapshot(root):
    """Include unexpected files and directories, not just requested targets."""
    files, directories = {}, set()
    for path in root.rglob("*"):
        relative = path.relative_to(root).as_posix()
        if path.is_dir():
            directories.add(relative)
        else:
            files[relative] = path.read_bytes()
    return files, directories


def native_backend_requests(binary, root, actions):
    requests = [dict(action, id=index, protocol=1)
                for index, action in enumerate(actions, 1)]
    result = subprocess.run(
        [str(binary), "native", "serve"], cwd=root,
        input="".join(dump(request) + "\n" for request in requests).encode("utf8"),
        capture_output=True, timeout=30, env=execution_environment(),
    )
    if result.returncode:
        raise AssertionError(result.stderr.decode("utf8", errors="replace"))
    replies = [json.loads(line) for line in result.stdout.splitlines()]
    if len(replies) != len(requests):
        raise AssertionError(f"expected {len(requests)} native_backend replies, got {replies!r}")
    for request, reply in zip(requests, replies):
        if reply.get("id") != request["id"] or reply.get("protocol") != 1:
            raise AssertionError(f"invalid native_backend response envelope: {reply!r}")
    return replies


class NativeByteContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        filename = "xnip.exe" if os.name == "nt" else "xnip"
        cls.binary = Path(os.environ.get("XNIP_TEST_BINARY", ROOT / "target/debug" / filename)).resolve()
        if not cls.binary.is_file():
            raise RuntimeError(f"build the actual CLI with cargo build --locked first: {cls.binary}")

    def exercise_cli(self, root, case, before):
        if len(case.edits) == 1:
            edit = case.edits[0]
            command = [str(self.binary), "edit", edit["file"], "--old", edit["old"],
                       "--text", edit["text"], *(["--all"] if edit.get("all") else [])]
            manifest = None
        else:
            command = [str(self.binary), "apply", "--from-stdin", "--format", "json"]
            manifest = dump([dict(edit, op="edit") for edit in case.edits]).encode("utf8")
        for mode in ["--dry-run", "--check", None]:
            result = subprocess.run(
                [*command, "--output", "json", *([mode] if mode else [])],
                input=manifest, cwd=root, capture_output=True, timeout=30,
                env=execution_environment(),
            )
            self.assertEqual(result.returncode, 0, result.stderr.decode("utf8", errors="replace"))
            receipt = json.loads(result.stdout)
            self.assertEqual(receipt["written"], mode is None)
            self.assertTrue(receipt["has_changes"])
            if mode:
                self.assertEqual(snapshot(root), before, f"CLI {mode} changed filesystem state")

    def exercise_mcp(self, root, case, before):
        tool = "xnip_edit" if len(case.edits) == 1 else "xnip_apply"
        arguments = dict(case.edits[0]) if len(case.edits) == 1 else {"edits": case.edits}
        with Mcp(self.binary, root, receipts="structured") as client:
            for mode in ["dry_run", "check", None]:
                request = dict(arguments, **({mode: True} if mode else {}))
                reply = client.call(tool, request)
                self.assertNotIn("jsonrpc_error", reply, reply)
                self.assertFalse(reply.get("isError", False), reply)
                self.assertEqual(reply["structuredContent"]["written"], mode is None)
                self.assertTrue(reply["structuredContent"]["has_changes"])
                if mode:
                    self.assertEqual(snapshot(root), before, f"MCP {mode} changed filesystem state")

    def assert_native_backend_changes(self, root, case, changes):
        # Storage bytes in receipts are also consumed by host diff/undo/audit.
        # Test them against the fixture oracle rather than rebuilding the edit.
        by_path = {Path(change["path"]).relative_to(root).as_posix(): change
                   for change in changes}
        self.assertEqual(set(by_path), {edit["file"] for edit in case.edits})
        expected = case.expected()
        for name, change in by_path.items():
            self.assertEqual(bytes(change["before"]), case.files[name].encode("utf8"))
            self.assertEqual(bytes(change["after"]), expected[name].encode("utf8"))

    def exercise_native_backend(self, root, case, before):
        operations = [{"op": "exact_replace", "path": edit["file"],
                       "old": list(edit["old"].encode("utf8")),
                       "text": list(edit["text"].encode("utf8")),
                       "all": edit.get("all", False)} for edit in case.edits]
        plan = {"action": "plan", "operations": operations}

        # JSONL exposes a nonwriting plan/preview, then explicit commit or abort.
        replies = native_backend_requests(self.binary, root, [plan, {"action": "abort", "plan_id": 1}])
        for reply in replies:
            self.assertNotIn("error", reply, reply)
        self.assert_native_backend_changes(root, case, replies[0]["result"]["changes"])
        self.assertTrue(replies[1]["result"]["aborted"])
        self.assertEqual(snapshot(root), before, "native_backend plan/abort changed filesystem state")

        # A valid plan must remain nonwriting when host policy denies commit.
        # This tests the host check boundary in addition to preview-only abort.
        denied = {"action": "commit", "plan_id": 1,
                  "policy": {"read_only": True, "writable_roots": [str(root)]}}
        replies = native_backend_requests(self.binary, root, [plan, denied])
        self.assertNotIn("error", replies[0], replies[0])
        self.assertEqual(replies[1]["error"]["code"], "governance", replies[1])
        self.assertEqual(snapshot(root), before, "native_backend policy check changed filesystem state")

        allowed = {"action": "commit", "plan_id": 1,
                   "policy": {"read_only": False, "writable_roots": [str(root)]}}
        replies = native_backend_requests(self.binary, root, [plan, allowed])
        for reply in replies:
            self.assertNotIn("error", reply, reply)
        self.assertTrue(replies[1]["result"]["written"])
        self.assert_native_backend_changes(root, case, replies[1]["result"]["changes"])

    def exercise(self, case):
        expected_files = {name: text.encode("utf8") for name, text in case.expected().items()}
        for transport in ["cli", "mcp", "native_backend"]:
            with self.subTest(transport=transport), tempfile.TemporaryDirectory(prefix="xnip-native_backend-bytes-") as temporary:
                root = Path(temporary).resolve()
                for name, text in case.files.items():
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(text.encode("utf8"))
                before = snapshot(root)
                getattr(self, "exercise_" + transport)(root, case, before)
                self.assertEqual(snapshot(root), (expected_files, before[1]),
                                 f"{transport} differs from fixture's expected raw bytes")


def fixture_test(case):
    def test(self):
        self.exercise(case)
    return test


for fixture in scenarios():
    setattr(NativeByteContract, "test_" + fixture.name.replace("-", "_"), fixture_test(fixture))


if __name__ == "__main__":
    unittest.main()
