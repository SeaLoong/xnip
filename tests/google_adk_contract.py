"""Actual Google ADK 2.13 MCP toolset contracts; model calls stay zero."""
from __future__ import annotations

import asyncio
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from google_adk_runtime import ROOT, fixture

sys.path.insert(0, str(ROOT / "benches"))
from edit_cost import scenarios

BINARY = Path(os.environ.get("XNIP_TEST_BINARY", ROOT / "target/debug/xnip")).resolve()
NAMES = {"xnip_apply", "xnip_edit", "xnip_find", "xnip_peek", "xnip_recover"}


class GoogleAdkContract(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="xnip ADK 中文 ", dir=ROOT / "target")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    async def case(self, case):
        for name, content in case.files.items():
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content.encode())
        async with fixture(self.root, BINARY) as runtime:
            tool, arguments = ("xnip_edit", case.edits[0]) if len(case.edits) == 1 else ("xnip_apply", {"edits": case.edits})
            outcome = await runtime.call(tool, arguments)
            self.assertEqual(len(outcome["rpc"]), 1)
            self.assertNotIn("rpc_error", outcome["rpc"][0])
            self.assertFalse(outcome["projection"].get("isError", False))
            self.assertEqual(runtime.model_calls, 0)
        actual = {p.relative_to(self.root).as_posix(): p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        self.assertEqual(actual, {name: content.encode() for name, content in case.expected().items()})

    async def test_actual_agent_registry_raw_schema_and_typed_projection(self):
        async with fixture(self.root, BINARY) as runtime:
            self.assertEqual(set(runtime.registry), NAMES)
            self.assertEqual(runtime.sdk_agent.tools, [runtime.sdk_toolset])
            declared = {item["name"] for group in runtime.declarations["tools"] for item in group.get("functionDeclarations", [])}
            self.assertEqual(declared, NAMES)
            async with fixture(self.root, BINARY, host="direct-mcp") as direct:
                self.assertEqual(runtime.schemas, direct.schemas)
            (self.root / "a.txt").write_bytes(b"old\r\n")
            result = await runtime.call("xnip_edit", {"file": "a.txt", "old": "old", "text": "new"})
            self.assertTrue(result["projection"]["structuredContent"]["written"])
            self.assertEqual(result["rpc"][0]["name"], "xnip_edit")
            self.assertEqual((self.root / "a.txt").read_bytes(), b"new\r\n")
            self.assertEqual(runtime.model_calls, 0)

    async def test_confirmation_request_rejection_and_approval_use_actual_adk_context(self):
        target = self.root / "a.txt"
        target.write_bytes(b"old\n")
        arguments = {"file": "a.txt", "old": "old", "text": "new"}
        async with fixture(self.root, BINARY, require_confirmation=True) as runtime:
            pending = await runtime.call("xnip_edit", arguments)
            self.assertIn("confirmation", pending["projection"]["error"])
            self.assertEqual(pending["rpc"], [])
            self.assertTrue(pending["actions"]["requestedToolConfirmations"])
            self.assertEqual(target.read_bytes(), b"old\n")
            rejected = await runtime.call("xnip_edit", arguments, confirmation=False)
            self.assertIn("rejected", rejected["projection"]["error"])
            self.assertEqual(rejected["rpc"], [])
            self.assertEqual(target.read_bytes(), b"old\n")
            approved = await runtime.call("xnip_edit", arguments, confirmation=True)
            self.assertEqual(len(approved["rpc"]), 1)
            self.assertEqual(target.read_bytes(), b"new\n")
            self.assertEqual(runtime.model_calls, 0)

    async def test_public_filter_prefix_and_full_catalog(self):
        async with fixture(self.root, BINARY, full=True) as runtime:
            self.assertEqual(len(runtime.registry), 10)
        (self.root / "a").write_text("old")
        async with fixture(self.root, BINARY, tool_filter=["xnip_edit"], tool_name_prefix="files") as runtime:
            self.assertEqual(len(runtime.registry), 1)
            name = next(iter(runtime.registry))
            self.assertTrue(name.startswith("files"))
            result = await runtime.call(name, {"file": "a", "old": "old", "text": "new"})
            self.assertEqual(result["rpc"][0]["name"], "xnip_edit")
            self.assertEqual((self.root / "a").read_text(), "new")

    async def test_specific_remote_guards_and_late_failure_keep_original_bytes(self):
        target = self.root / "a"
        target.write_bytes(b"old old\r\n")
        async with fixture(self.root, BINARY) as runtime:
            for old, expected in [("missing", "old text was not found"), ("old", "old text is ambiguous")]:
                result = await runtime.call("xnip_edit", {"file": "a", "old": old, "text": "new"})
                error = result["rpc"][0]["rpc_error"]
                self.assertEqual(error["code"], -32600)
                self.assertIn(expected, error["message"])
                self.assertEqual(target.read_bytes(), b"old old\r\n")
            result = await runtime.call("xnip_apply", {"edits": [
                {"file": "a", "old": "old old", "text": "new"},
                {"file": "a", "old": "absent", "text": "failure"}]})
            self.assertEqual(result["rpc"][0]["rpc_error"]["code"], -32600)
            self.assertEqual(target.read_bytes(), b"old old\r\n")

    async def test_preview_create_rename_delete_and_read_use_actual_tools(self):
        (self.root / "a").write_bytes(b"old\r\n")
        async with fixture(self.root, BINARY) as runtime:
            result = await runtime.call("xnip_edit", {"file": "a", "old": "old", "text": "new", "dry_run": True})
            self.assertFalse(result["projection"]["structuredContent"]["written"])
            self.assertEqual((self.root / "a").read_bytes(), b"old\r\n")
            for arguments in [
                {"file": "nested/new", "operation": "create", "text": "\ufeffα\r\n尾巴"},
                {"file": "nested/new", "operation": "rename", "to": "moved"},
            ]:
                result = await runtime.call("xnip_edit", arguments)
                self.assertNotIn("rpc_error", result["rpc"][0])
            self.assertEqual((self.root / "moved").read_bytes(), "\ufeffα\r\n尾巴".encode())
            await runtime.call("xnip_peek", {"file": "moved", "all": True})
            await runtime.call("xnip_find", {"file": "moved", "pattern": "尾巴"})
            await runtime.call("xnip_edit", {"file": "moved", "operation": "delete"})
            self.assertFalse((self.root / "moved").exists())

    async def test_closing_and_reopening_toolsets_preserves_application_tools(self):
        marker = lambda value: value
        async with fixture(self.root, BINARY) as runtime:
            runtime.sdk_agent.tools.append(marker)
            self.assertIs(runtime.sdk_agent.tools[-1], marker)
        self.assertTrue(all(process.returncode == 0 for process in runtime.processes))
        (self.root / "a").write_text("old")
        async with fixture(self.root, BINARY) as reopened:
            await reopened.call("xnip_edit", {"file": "a", "old": "old", "text": "restored"})
            self.assertEqual((self.root / "a").read_text(), "restored")

    async def test_local_package_install_import_and_actual_tool_execution(self):
        source = self.root / "source"
        shutil.copytree(ROOT / "integrations/google-adk", source, ignore=shutil.ignore_patterns("__pycache__"))
        installed = self.root / "installed"
        subprocess.run([sys.executable, "-m", "pip", "install", "--no-deps", "--no-build-isolation", "--no-compile",
                        "--disable-pip-version-check", "--target", str(installed), str(source)],
                       check=True, capture_output=True, text=True, timeout=60)
        script = r'''
import asyncio,os
from pathlib import Path
import xnip_google_adk
from google.adk.agents import LlmAgent
from google.adk.agents.invocation_context import InvocationContext
from google.adk.models.base_llm import BaseLlm
from google.adk.sessions import InMemorySessionService
from google.adk.tools.tool_context import ToolContext
assert Path(xnip_google_adk.__file__).is_relative_to(Path(os.environ['XNIP_INSTALLED']))
class IdleModel(BaseLlm):
 calls: int = 0
 async def generate_content_async(self,*args,**kwargs):
  self.calls+=1
  raise AssertionError('Installed-package tool execution reached model generation')
  yield
async def main():
 root=Path(os.environ['XNIP_PROJECT']);(root/'installed.txt').write_text('old')
 model=IdleModel(model='tool-only')
 async with xnip_google_adk.xnip_toolset(cwd=root,executable=os.environ['XNIP_TEST_BINARY']) as toolset:
  agent=LlmAgent(name='installed_tools',model=model,tools=[toolset])
  sessions=InMemorySessionService();session=await sessions.create_session(app_name='test',user_id='test')
  context=ToolContext(InvocationContext(session_service=sessions,invocation_id='test',agent=agent,session=session),function_call_id='one')
  tools={tool.name:tool for tool in await toolset.get_tools()}
  result=await tools['xnip_edit'].run_async(args={'file':'installed.txt','old':'old','text':'new'},tool_context=context)
  assert not result.get('isError',False)
 assert (root/'installed.txt').read_text()=='new'
 assert model.calls==0
asyncio.run(main())
'''
        environment = {**os.environ, "PYTHONPATH": str(installed), "XNIP_INSTALLED": str(installed),
                       "XNIP_PROJECT": str(self.root), "XNIP_TEST_BINARY": str(BINARY), "XNIP_AUTO_UPDATE": "0"}
        result = subprocess.run([sys.executable, "-c", script], cwd=self.root, env=environment,
                                capture_output=True, text=True, timeout=40)
        self.assertEqual(result.returncode, 0, result.stderr)


for case in scenarios():
    async def byte_fixture(self, case=case):
        await self.case(case)
    setattr(GoogleAdkContract, "test_bytes_" + case.name.replace("-", "_"),
            byte_fixture)


if __name__ == "__main__":
    unittest.main(verbosity=2)
