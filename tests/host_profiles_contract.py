"""Real generated MCP launches, official CLI parsers and Goose's MCP client."""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from host_profiles_runtime import ROOT, ProfileMcp, module, profiles, setup, verify_cli, verify_continue

sys.path.insert(0, str(ROOT / "benches"))
from edit_cost import scenarios

BINARY = Path(os.environ.get("XNIP_TEST_BINARY", ROOT / "target/debug/xnip")).resolve()
TOOL_NAMES = {"xnip_edit", "xnip_apply", "xnip_peek", "xnip_find", "xnip_recover"}


class HostProfilesContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="xnip host profiles 中文 ", dir=ROOT / "target")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.project = self.root / "project with spaces"
        self.project.mkdir()
        self.other = self.root / "host-launch-directory"
        self.other.mkdir()
        self.artifacts = profiles.generate(BINARY, self.project, self.root / "generated")

    def configuration(self, host="cursor"):
        entry = self.artifacts["profiles"][host]
        return profiles.launch(host, Path(entry["file"]))

    def profile(self, host):
        if host == "continue":
            proof = verify_continue(self.artifacts["profiles"][host]["file"])
            self.assertEqual(proof["model_calls"], 0)
        target = self.project / "a.txt"
        target.write_bytes("标题\r\nold\n尾巴".encode())
        (self.other / "a.txt").write_bytes(b"other directory")
        with ProfileMcp(self.configuration(host), self.other) as client:
            self.assertEqual({tool["name"] for tool in client.catalog}, TOOL_NAMES)
            tool = next(tool for tool in client.catalog if tool["name"] == "xnip_edit")
            self.assertIn("file", tool["inputSchema"]["properties"])
            self.assertFalse(client.call("xnip_peek", {"file": "a.txt", "all": True}).get("isError", False))
            result = client.call("xnip_edit", {"file": "a.txt", "old": "old", "text": "new"})
            self.assertFalse(result.get("isError", False), result)
            self.assertEqual(target.read_bytes(), "标题\r\nnew\n尾巴".encode())
            self.assertEqual((self.other / "a.txt").read_bytes(), b"other directory")
        self.assertEqual(client.process.returncode, 0, "MCP EOF drains the configured child")

    def byte_fixture(self, case):
        for name, text in case.files.items():
            file = self.project / name
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes(text.encode())
        with ProfileMcp(self.configuration(), self.other) as client:
            if len(case.edits) == 1:
                result = client.call("xnip_edit", case.edits[0])
            else:
                result = client.call("xnip_apply", {"edits": case.edits})
            self.assertFalse(result.get("isError", False), result)
        actual = {p.relative_to(self.project).as_posix(): p.read_bytes()
                  for p in self.project.rglob("*") if p.is_file()}
        self.assertEqual(actual, {name: text.encode() for name, text in case.expected().items()})

    def test_configuration_generation_preserves_user_settings_and_existing_outputs(self):
        original = '{"mcpServers":{"user-server":{"command":"keep"}}}\n'
        settings = self.project / ".mcp.json"
        settings.write_text(original)
        with self.assertRaises(FileExistsError):
            profiles.generate(BINARY, self.project, self.root / "generated")
        self.assertEqual(settings.read_text(), original)
        self.assertTrue(all(Path(item["file"]).is_file() for item in self.artifacts["profiles"].values()))

    def test_preview_guard_failure_and_complete_file_lifecycle(self):
        target = self.project / "a.txt"
        target.write_bytes(b"old\r\n")
        with ProfileMcp(self.configuration(), self.other) as client:
            preview = client.call("xnip_edit", {"file": "a.txt", "old": "old", "text": "new", "dry_run": True})
            self.assertFalse(preview.get("isError", False))
            self.assertEqual(target.read_bytes(), b"old\r\n")
            with self.assertRaisesRegex(RuntimeError, "precondition|not found"):
                client.call("xnip_apply", {"edits": [
                    {"file": "a.txt", "old": "old", "text": "new"},
                    {"file": "a.txt", "old": "missing", "text": "failure"}]})
            self.assertEqual(target.read_bytes(), b"old\r\n")
            for operation in [
                {"file": "nested/created.txt", "operation": "create", "text": "\ufeffα\r\n尾巴"},
                {"file": "nested/created.txt", "operation": "rename", "to": "renamed.txt"},
            ]:
                self.assertFalse(client.call("xnip_edit", operation).get("isError", False))
            self.assertEqual((self.project / "renamed.txt").read_bytes(), "\ufeffα\r\n尾巴".encode())
            self.assertFalse(client.call("xnip_edit", {"file": "renamed.txt", "operation": "delete"}).get("isError", False))
            self.assertFalse((self.project / "renamed.txt").exists())

    def official_cli(self, host):
        record = verify_cli(host, self.configuration(host), self.root / "cli-reader" / host)
        self.assertEqual(record["version"], setup.VERSIONS[host])
        self.assertEqual(record["model_calls"], 0)

    def test_goose_uses_its_unchanged_official_client_for_discovery_and_real_edits(self):
        goose = module("host_profiles_goose", ROOT / "benches/native/goose_adapter.py")
        metadata = goose.prepare(check=True)
        config = self.configuration("goose")
        target = self.project / "a.txt"
        target.write_bytes(b"old\r\n")
        request = {"action": "bridge", "root": str(self.other), "command": config["command"],
                   "arguments": config["args"], "capture_tool_errors": True, "calls": [
                       {"name": "xnip_edit", "arguments": {"file": "a.txt", "old": "old", "text": "goose"}},
                       {"name": "xnip_edit", "arguments": {"file": "a.txt", "old": "missing", "text": "rejected"}}]}
        result = subprocess.run([metadata["binary"]], input=json.dumps(request) + "\n",
                                capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        reply = json.loads(result.stdout)
        self.assertTrue(reply["ok"], reply)
        self.assertEqual(reply["model_calls"], 0)
        self.assertEqual({tool["name"] for tool in reply["tools"]["tools"]}, TOOL_NAMES)
        self.assertTrue(reply["calls"][0]["ok"])
        self.assertEqual(reply["calls"][1]["rpc_error"]["code"], -32600)
        self.assertIn("old text was not found", reply["calls"][1]["rpc_error"]["message"])
        self.assertEqual(target.read_bytes(), b"goose\r\n")


for host in profiles.HOST_FILES:
    setattr(HostProfilesContract, "test_profile_" + host.replace("-", "_"),
            lambda self, value=host: self.profile(value))
for case in scenarios():
    setattr(HostProfilesContract, "test_bytes_" + case.name.replace("-", "_"),
            lambda self, value=case: self.byte_fixture(value))
for host in setup.VERSIONS:
    setattr(HostProfilesContract, "test_official_cli_" + host.replace("-", "_"),
            lambda self, value=host: self.official_cli(value))


if __name__ == "__main__":
    unittest.main(verbosity=2)
