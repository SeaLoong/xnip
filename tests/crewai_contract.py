"""Real CrewAI tool registration, file execution and package lifecycle."""
from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from crewai_runtime import ROOT, fixture
from crewai.mcp import create_static_tool_filter
from crewai.tools.mcp_native_tool import MCPNativeTool
from crewai import Agent
from crewai_runtime import IdleLLM

sys.path.insert(0, str(ROOT / "benches"))
from edit_cost import scenarios

BINARY = Path(os.environ.get("XNIP_TEST_BINARY", ROOT / "target/debug/xnip")).resolve()
NAMES = {"xnip_edit", "xnip_apply", "xnip_peek", "xnip_find", "xnip_recover"}


class CrewAIContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="crewai-contract-", dir=ROOT / "target")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def byte_fixture(self, case):
        for name, content in case.files.items():
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content.encode())
        with fixture(self.root, str(BINARY)) as runtime:
            if len(case.edits) == 1:
                result, actual = runtime.call("xnip_edit", case.edits[0])
            else:
                result, actual = runtime.call("xnip_apply", {"edits": case.edits})
            self.assertEqual(result, "written")
            self.assertTrue(actual)
            self.assertEqual(runtime.model.calls, 0)
        files = {p.relative_to(self.root).as_posix(): p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        self.assertEqual(files, {name: value.encode() for name, value in case.expected().items()})

    def test_actual_agent_registry_schema_and_file_tools_cache_policy(self):
        with fixture(self.root, str(BINARY)) as runtime:
            self.assertEqual(set(runtime.tools), NAMES)
            self.assertTrue(all(isinstance(tool, MCPNativeTool) for tool in runtime.agent.tools))
            for tool in runtime.agent.tools:
                self.assertEqual(tool.name, tool.original_tool_name)
                self.assertFalse(tool.cache_function({}, "written"))
                self.assertIs(tool.to_structured_tool().cache_function, tool.cache_function)
            edit = runtime.tools["xnip_edit"].args_schema.model_json_schema()
            self.assertIn("file", edit["required"])
            self.assertIn("operation", edit["properties"])
            self.assertEqual(runtime.model.calls, 0)
        original = Agent(role="Original", goal="Restore tools", backstory="Tool contract", llm=IdleLLM(model="idle"), tools=[])
        self.assertEqual(original.tools, [])

    def test_write_read_find_rename_delete_and_preview(self):
        with fixture(self.root, str(BINARY)) as runtime:
            result, _ = runtime.call("xnip_edit", {"file": "nested/a.txt", "operation": "create", "text": "\ufeffold\r\n尾巴"})
            self.assertEqual(result, "written")
            preview, _ = runtime.call("xnip_edit", {"file": "nested/a.txt", "old": "old", "text": "new", "dry_run": True})
            self.assertIn("written=false", preview)
            self.assertEqual((self.root / "nested/a.txt").read_bytes(), "\ufeffold\r\n尾巴".encode())
            read, trace = runtime.call("xnip_peek", {"file": "nested/a.txt", "all": True})
            self.assertIn("old", read)
            self.assertFalse(trace[-1]["result"].get("isError", False))
            found, _ = runtime.call("xnip_find", {"files": ["nested/a.txt"], "pattern": "old"})
            self.assertIn("nested/a.txt", found)
            runtime.call("xnip_edit", {"file": "nested/a.txt", "operation": "rename", "to": "moved.txt"})
            self.assertEqual((self.root / "moved.txt").read_bytes(), "\ufeffold\r\n尾巴".encode())
            runtime.call("xnip_edit", {"file": "moved.txt", "operation": "delete"})
            self.assertFalse((self.root / "moved.txt").exists())

    def test_real_remote_guard_and_ambiguity_errors_preserve_storage(self):
        target = self.root / "a.txt"
        for before, old, expected in [("newer\r\n", "old", "old text was not found"),
                                      ("old old\n", "old", "old text is ambiguous")]:
            target.write_bytes(before.encode())
            with fixture(self.root, str(BINARY)) as runtime:
                with self.assertRaises(Exception):
                    runtime.call("xnip_edit", {"file": "a.txt", "old": old, "text": "changed"})
                self.assertTrue(runtime.calls)
                for call in runtime.calls:
                    self.assertEqual(call["name"], "xnip_edit")
                    self.assertEqual(call["rpc_error"]["code"], -32600)
                    self.assertIn(expected, call["rpc_error"]["message"])
                self.assertEqual(target.read_bytes(), before.encode())

    def test_official_filter_and_per_invocation_connection_lifecycle(self):
        selected = create_static_tool_filter(allowed_tool_names=["xnip_peek"])
        with fixture(self.root, str(BINARY), selected) as runtime:
            self.assertEqual(set(runtime.tools), {"xnip_peek"})
            target = self.root / "a.txt"
            target.write_text("first")
            first, _ = runtime.call("xnip_peek", {"file": "a.txt", "all": True})
            target.write_text("second")
            second, _ = runtime.call("xnip_peek", {"file": "a.txt", "all": True})
            self.assertIn("first", first)
            self.assertIn("second", second)
            self.assertGreaterEqual(len(runtime.transports), 3)
            self.assertTrue(all(not item.connected for item in runtime.transports))

    def test_installed_python_package_resolves_actual_crewai_tools(self):
        source = self.root / "source"
        shutil.copytree(ROOT / "integrations/crewai", source, ignore=shutil.ignore_patterns("__pycache__"))
        installed = self.root / "installed"
        subprocess.run([sys.executable, "-m", "pip", "install", "--no-deps", "--no-build-isolation", "--target", str(installed), str(source)],
                       check=True, capture_output=True, text=True)
        script = r'''
import sys,io
from pathlib import Path
from contextlib import redirect_stdout
sys.path.insert(0,sys.argv[1])
import xnip_crewai
assert Path(xnip_crewai.__file__).is_relative_to(Path(sys.argv[1]))
with redirect_stdout(io.StringIO()):
 with xnip_crewai.xnip_tools(sys.argv[2],sys.argv[3]) as tools:
  tool=next(t for t in tools if t.original_tool_name=='xnip_edit')
  assert tool.run(file='packaged.txt',operation='create',text='installed\r\n')=='written'
assert (Path(sys.argv[2])/'packaged.txt').read_bytes()==b'installed\r\n'
'''
        subprocess.run([sys.executable, "-c", script, str(installed), str(self.root), str(BINARY)],
                       check=True, capture_output=True, text=True, env=os.environ, timeout=45)


for case in scenarios():
    setattr(CrewAIContract, "test_bytes_" + case.name.replace("-", "_"),
            lambda self, value=case: self.byte_fixture(value))


if __name__ == "__main__":
    unittest.main(verbosity=2)
