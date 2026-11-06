"""Observable MCP SDK contracts; the editor remains the real Rust process."""

import asyncio
import importlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from agent_sdks_runtime import fixture

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
from edit_cost import Mcp, scenarios

TOOLS = {"xnip_edit", "xnip_apply", "xnip_peek", "xnip_find", "xnip_recover"}
BINARY = str(Path(os.environ.get("XNIP_TEST_BINARY", ROOT / "target/debug/xnip")).resolve())


def files(root):
    return {p.relative_to(root).as_posix(): p.read_bytes() for p in root.rglob("*") if p.is_file()}


def contract_class(host):
    class Contract(unittest.IsolatedAsyncioTestCase):
        async def asyncSetUp(self):
            self.temp = tempfile.TemporaryDirectory(prefix="xnip-agent-sdk-")
            self.addCleanup(self.temp.cleanup)
            self.cwd = Path(self.temp.name)

        def runtime(self, **options):
            return fixture(host, self.cwd, BINARY, **options)

        async def test_registry_schema_and_agent_configuration(self):
            async with self.runtime() as runtime:
                self.assertEqual(set(runtime.registry), TOOLS)
                with Mcp(Path(BINARY), self.cwd) as direct:
                    raw = {t["name"]: t["inputSchema"] for t in direct.request("tools/list", {})["tools"]}
                for name in TOOLS:
                    self.assertEqual(runtime.schemas[name]["properties"], raw[name]["properties"])
                    self.assertEqual(runtime.schemas[name].get("required", []), raw[name].get("required", []))
                self.assertEqual(runtime.model_calls, 0)
                self.assertIsNotNone(runtime.sdk_agent)

        async def test_preview_check_noop_and_backup(self):
            target = self.cwd / "a.txt"
            target.write_bytes(b"old\r\n")
            async with self.runtime() as runtime:
                for mode in ["dry_run", "check"]:
                    result = await runtime.call("xnip_edit", {"file": "a.txt", "old": "old", "text": "new", mode: True})
                    self.assertFalse(result["is_error"], result)
                    self.assertEqual(target.read_bytes(), b"old\r\n")
                    if result["receipt"] is not None:
                        self.assertFalse(result["receipt"]["written"])
                result = await runtime.call("xnip_edit", {"file": "a.txt", "old": "old", "text": "new", "backup": True})
                self.assertFalse(result["is_error"], result)
                self.assertEqual(target.read_bytes(), b"new\r\n")
                self.assertEqual((self.cwd / "a.txt.bak").read_bytes(), b"old\r\n")
                result = await runtime.call("xnip_edit", {"file": "a.txt", "old": "new", "text": "new"})
                self.assertFalse(result["is_error"], result)
                self.assertEqual(target.read_bytes(), b"new\r\n")

        async def test_create_delete_rename_and_grouped_failure(self):
            async with self.runtime() as runtime:
                operations = [{"op": "create", "file": "nested/a", "text": "one\r\n"},
                              {"op": "rename", "file": "nested/a", "to-file": "nested/b"},
                              {"op": "create", "file": "delete", "text": "temp"},
                              {"op": "delete", "file": "delete"}]
                result = await runtime.call("xnip_apply", {"manifest_text": json.dumps(operations), "format": "json", "sequential": True})
                self.assertFalse(result["is_error"], result)
                self.assertEqual(files(self.cwd), {"nested/b": b"one\r\n"})
                result = await runtime.call("xnip_apply", {"edits": [
                    {"file": "nested/b", "old": "one", "text": "changed"},
                    {"file": "absent", "old": "missing", "text": "wrong"}]})
                self.assertTrue(result["is_error"], result)
                self.assertEqual(files(self.cwd), {"nested/b": b"one\r\n"})

        async def test_missing_ambiguous_and_full_write_guards(self):
            target = self.cwd / "a"
            target.write_text("same same", encoding="utf8")
            async with self.runtime() as runtime:
                for args in [{"file": "a", "old": "absent", "text": "new"},
                             {"file": "a", "old": "same", "text": "new"},
                             {"file": "a", "old": "stale", "text": "new", "operation": "write"}]:
                    result = await runtime.call("xnip_edit", args)
                    self.assertTrue(result["is_error"], result)
                    self.assertEqual(target.read_bytes(), b"same same")

        async def test_reads_search_budgets_and_unicode(self):
            (self.cwd / "a").write_bytes("零\r\nmatch\r\n尾巴".encode())
            async with self.runtime() as runtime:
                peek = await runtime.call("xnip_peek", {"file": "a", "lines": "2-3", "max_bytes": 100})
                self.assertFalse(peek["is_error"], peek)
                self.assertIn("match", json.dumps(peek["projection"], ensure_ascii=False))
                found = await runtime.call("xnip_find", {"files": ["a"], "pattern": "match", "max_matches": 1})
                self.assertFalse(found["is_error"], found)
                self.assertIn("a:2:1", json.dumps(found["projection"], ensure_ascii=False))

        async def test_full_catalog_and_owned_session_teardown(self):
            async with self.runtime(full=True) as runtime:
                self.assertEqual(len(runtime.registry), 10)
                if host == "agents-sdk":
                    server = runtime.sdk_server
                    self.assertIsNotNone(server.session)
                else:
                    workbench = runtime.sdk_workbench
                    self.assertIsNotNone(workbench.initialize_result)
                self.assertEqual(runtime.model_calls, 0)
                self.assertTrue(runtime.processes)
                self.assertTrue(all(process.returncode is None for process in runtime.processes))
            if host == "agents-sdk":
                self.assertIsNone(server.session)
            else:
                self.assertIsNone(workbench.initialize_result)
            self.assertTrue(all(process.returncode is not None for process in runtime.processes))

        async def test_host_tool_selection_and_approval_metadata(self):
            (self.cwd / "a").write_text("original", encoding="utf8")
            if host == "agents-sdk":
                from agents.mcp import create_static_tool_filter
                async with self.runtime(tool_filter=create_static_tool_filter(allowed_tool_names=["xnip_peek"]),
                                        require_approval="always") as runtime:
                    self.assertEqual(set(runtime.registry), {"xnip_peek"})
                    self.assertIs(runtime.registry["xnip_peek"].needs_approval, True)
                    self.assertEqual(runtime.model_calls, 0)
            else:
                from autogen_core.tools import ToolOverride
                async with self.runtime(tool_overrides={"xnip_peek": ToolOverride(name="inspect_file")}) as runtime:
                    self.assertIn("inspect_file", runtime.registry)
                    self.assertNotIn("xnip_peek", runtime.registry)
                    result = await runtime.call("inspect_file", {"file": "a", "all": True})
                    self.assertFalse(result["is_error"], result)
                    self.assertIn("original", json.dumps(result["projection"]))
            self.assertEqual((self.cwd / "a").read_text(), "original")

        async def test_installed_python_package_executes_actual_sdk_tool(self):
            project = "agents-sdk" if host == "agents-sdk" else "autogen"
            module = "xnip_agents" if host == "agents-sdk" else "xnip_autogen"
            stage, installed = self.cwd / "source", self.cwd / "installed"
            shutil.copytree(ROOT / "integrations" / project, stage,
                            ignore=shutil.ignore_patterns("__pycache__", "*.egg-info", "build"))
            result = subprocess.run([sys.executable, "-m", "pip", "install", "--no-deps", "--no-build-isolation",
                                     "--no-index", "--target", str(installed), str(stage)],
                                    capture_output=True, text=True, timeout=60)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            script = '''import asyncio,importlib,sys
from pathlib import Path
sys.path.insert(0,sys.argv[1]);module=importlib.import_module(sys.argv[2])
assert Path(module.__file__).is_relative_to(Path(sys.argv[1]))
sys.path.insert(0,sys.argv[3]);from agent_sdks_runtime import fixture
async def main():
 async with fixture(sys.argv[4],sys.argv[5],sys.argv[6]) as runtime:
  result=await runtime.call('xnip_edit',dict(file='installed.txt',operation='write',text='installed\\r\\n尾巴'))
  assert not result['is_error'],result
  assert runtime.model_calls==0
asyncio.run(main())
'''
            result = subprocess.run([sys.executable, "-c", script, str(installed), module,
                                     str(ROOT / "tests"), host, str(self.cwd), BINARY],
                                    capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual((self.cwd / "installed.txt").read_bytes(), "installed\r\n尾巴".encode())

        async def byte_case(self, case):
            for name, text in case.files.items():
                target = self.cwd / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(text.encode())
            async with self.runtime() as runtime:
                tool = "xnip_edit" if len(case.edits) == 1 else "xnip_apply"
                arguments = case.edits[0] if len(case.edits) == 1 else {"edits": case.edits}
                result = await runtime.call(tool, arguments)
                self.assertFalse(result["is_error"], result)
                self.assertEqual(runtime.model_calls, 0)
            self.assertEqual(files(self.cwd), {name: text.encode() for name, text in case.expected().items()})

    for case in scenarios():
        async def exercise(self, fixture_case=case):
            await self.byte_case(fixture_case)
        setattr(Contract, "test_bytes_" + case.name.replace("-", "_"), exercise)
    return Contract
