"""Real pinned Goose editing and GooseClient→xnip contracts; never call a model.

Provision the optional native runtime first:
    python benches/native/goose_adapter.py --setup
Then run:
    python -m unittest discover -s tests -p goose_native_contract.py
Set XNIP_TEST_BINARY to select the xnip build for the real stdio bridge.
"""
from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
ADAPTER = ROOT / "benches/native/goose_adapter.py"
SPEC = importlib.util.spec_from_file_location("xnip_goose_adapter", ADAPTER)
assert SPEC and SPEC.loader
goose = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(goose)


class SourceIntegrityContract(unittest.TestCase):
    def test_missing_or_modified_official_source_is_rejected_before_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            with mock.patch.object(goose, "CACHE", Path(temporary)):
                with self.assertRaisesRegex(RuntimeError, "missing pinned source"):
                    goose.verify_sources(download=False)
                source = goose.source_directory()
                for entry in goose.PINS["files"]:
                    destination = source / entry["path"]
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    destination.write_bytes(b"untrusted substitute")
                with self.assertRaisesRegex(RuntimeError, "untrusted Goose source cache"):
                    goose.verify_sources(download=False)


class NativeFixture(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        try:
            cls.metadata = goose.prepare(check=True)
        except (OSError, RuntimeError, ValueError) as error:
            raise unittest.SkipTest(f"optional pinned Goose runtime unavailable: {error}") from error
        cls.binary = cls.metadata["binary"]

    def invoke(self, requests):
        result = subprocess.run(
            [self.binary], input="".join(json.dumps(request, ensure_ascii=False) + "\n" for request in requests),
            capture_output=True, text=True, encoding="utf-8", timeout=60,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")
        replies = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual(len(replies), len(requests))
        for reply in replies:
            self.assertNotIn("adapter_error", reply, reply)
        return replies

    def edit(self, root, path, before, after):
        return self.invoke([{"action": "execute", "root": str(root), "tool": "edit",
                            "arguments": {"path": path, "before": before, "after": after}}])[0]


class NativeEditorContract(NativeFixture):
    def test_describe_retains_official_schemas_and_annotations(self):
        metadata = self.invoke([{"action": "describe"}])[0]
        self.assertEqual(metadata["commit"], goose.PINS["commit"])
        self.assertEqual(metadata["model_calls"], 0)
        tools = {tool["name"]: tool for tool in metadata["tools"]}
        self.assertEqual(set(tools), {"write", "edit"})
        self.assertEqual(set(tools["edit"]["inputSchema"]["required"]), {"path", "before", "after"})
        self.assertTrue(tools["edit"]["annotations"]["destructiveHint"])
        self.assertEqual(metadata["source_revisions"], goose.PINS["files"])

    def test_unique_substring_preserves_unicode_mixed_endings_and_missing_eof(self):
        with tempfile.TemporaryDirectory(prefix="goose native 中文 ") as temporary:
            root = Path(temporary)
            file = root / "目录 with spaces" / "example.txt"
            file.parent.mkdir()
            original = "标题=编辑器\r\nport=8080\nmessage=你好 🌏"
            file.write_bytes(original.encode())
            result = self.edit(root, "目录 with spaces/example.txt", "8080", "3000")
            self.assertTrue(result["ok"], result)
            self.assertEqual(file.read_bytes(), original.replace("8080", "3000").encode())
            self.assertIsNone(result["model_visible_result"])
            self.assertEqual(result["native_result"]["content"], result["content"])
            self.assertIn("Edited", result["output"])
            self.assertGreaterEqual(result["elapsed_ms"], 0)

    def test_ambiguous_and_stale_blocks_return_native_context_without_writing(self):
        for before, expected_error in [("same", "Found 2 matches"), ("missing", "No match found")]:
            with self.subTest(before=before), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                file = root / "a.txt"
                original = b"same\r\nunique\nsame"
                file.write_bytes(original)
                result = self.edit(root, "a.txt", before, "changed")
                self.assertFalse(result["ok"])
                self.assertTrue(result["native_result"]["isError"])
                self.assertIn(expected_error, result["output"])
                self.assertEqual(file.read_bytes(), original)

    def test_empty_replacement_deletes_and_identical_replacement_keeps_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            file = root / "a.txt"
            file.write_bytes(b"head\r\nobsolete\r\ntail")
            result = self.edit(root, "a.txt", "obsolete\r\n", "")
            self.assertTrue(result["ok"])
            self.assertEqual(file.read_bytes(), b"head\r\ntail")
            result = self.edit(root, "a.txt", "head", "head")
            self.assertTrue(result["ok"])
            self.assertEqual(file.read_bytes(), b"head\r\ntail")

    def test_native_write_creates_parents_and_overwrites_without_normalizing_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            request = {"action": "execute", "root": str(root), "tool": "write",
                       "arguments": {"path": "nested/文件.txt", "content": "α\r\nβ"}}
            created = self.invoke([request])[0]
            self.assertTrue(created["ok"])
            self.assertTrue(created["output"].startswith("Created "))
            self.assertEqual((root / "nested/文件.txt").read_bytes(), "α\r\nβ".encode())
            request["arguments"]["content"] = "overwritten\n"
            overwritten = self.invoke([request])[0]
            self.assertTrue(overwritten["output"].startswith("Wrote "))
            self.assertEqual((root / "nested/文件.txt").read_bytes(), b"overwritten\n")

    def test_relative_paths_are_isolated_by_the_supplied_working_directory(self):
        with tempfile.TemporaryDirectory() as first, tempfile.TemporaryDirectory() as second:
            a, b = Path(first), Path(second)
            (a / "same.txt").write_bytes(b"first")
            (b / "same.txt").write_bytes(b"second")
            requests = [{"action": "execute", "root": str(root), "tool": "edit",
                         "arguments": {"path": "same.txt", "before": old, "after": "changed " + old}}
                        for root, old in [(a, "first"), (b, "second")]]
            self.assertTrue(all(reply["ok"] for reply in self.invoke(requests)))
            self.assertEqual((a / "same.txt").read_bytes(), b"changed first")
            self.assertEqual((b / "same.txt").read_bytes(), b"changed second")


class GooseMcpBridgeContract(NativeFixture):
    @classmethod
    def setUpClass(cls):
        super().setUpClass()
        configured = os.environ.get("XNIP_TEST_BINARY")
        cls.xnip = Path(configured) if configured else ROOT / "target/debug" / ("xnip.exe" if os.name == "nt" else "xnip")
        if not cls.xnip.is_file():
            raise unittest.SkipTest("build xnip or set XNIP_TEST_BINARY for real Goose stdio tests")
        cls.xnip = cls.xnip.resolve()

    def test_official_default_auto_lifecycle_connects_and_uses_relative_xnip_paths(self):
        self.run_bridge_profile(None)

    def test_official_legacy_initialize_profile_still_connects(self):
        self.run_bridge_profile("2025-11-25")

    def run_bridge_profile(self, version):
        with tempfile.TemporaryDirectory(prefix="goose stdio 中文 ") as temporary:
            root = Path(temporary)
            file = root / "a.txt"
            file.write_bytes(b"head\r\nold")
            request = {"action": "bridge", "root": str(root), "command": str(self.xnip), "calls": [
                {"name": "xnip_edit", "arguments": {"file": "a.txt", "old": "old", "text": "new"}},
                {"name": "xnip_edit", "arguments": {"file": "目录/new.txt", "operation": "write", "text": "你好\r\n"}},
                {"name": "xnip_peek", "arguments": {"file": "a.txt", "all": True}},
            ]}
            if version:
                request["protocol_version"] = version
            result = self.invoke([request])[0]
            self.assertTrue(result["ok"], result)
            self.assertEqual(result["model_calls"], 0)
            self.assertEqual(result["protocol_profile"], version or "official-default-auto")
            names = {tool["name"] for tool in result["tools"]["tools"]}
            self.assertTrue({"xnip_edit", "xnip_apply", "xnip_peek"} <= names)
            self.assertTrue(all(call["ok"] for call in result["calls"]), result)
            self.assertNotIn("structuredContent", result["calls"][0]["result"])
            self.assertEqual(file.read_bytes(), b"head\r\nnew")
            self.assertEqual((root / "目录/new.txt").read_bytes(), "你好\r\n".encode())
            self.assertEqual(result["stderr"], "")


if __name__ == "__main__":
    unittest.main()
