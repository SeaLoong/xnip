"""Real SDK registries, public dispatch, package loading and independent byte oracles."""
import asyncio
import importlib.metadata
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
from edit_cost import scenarios
from python_agents_runtime import Runtime, model_guard

BINARY = Path(os.environ.get("XNIP_TEST_BINARY", ROOT / "target/debug/xnip")).resolve()
FRAMEWORKS = ("deepagents", "langgraph", "pydantic-ai")


class PythonAgentsContract(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="xnip-python-agent-")
        self.root = Path(self.temporary.name)
        self.addCleanup(self.temporary.cleanup)
        self.guard = model_guard()
        self.model_calls = self.guard.__enter__()
        self.addCleanup(lambda: self.guard.__exit__(None, None, None))

    async def runtime(self, framework, **options):
        runtime = await Runtime(framework, self.root, BINARY, **options).__aenter__()
        self.addAsyncCleanup(runtime.__aexit__, None, None, None)
        return runtime

    def test_packages_are_installed_and_versions_are_fixed(self):
        import xnip_native_client
        import xnip_python_agents.deepagents as adapter
        for module in [xnip_native_client, adapter]:
            self.assertTrue(Path(module.__file__).is_relative_to(Path(sys.prefix)))
        for name, version in {"deepagents": "0.7.34", "langgraph": "1.2.20",
                              "pydantic-ai-slim": "2.66.0", "mcp": "2.3.0"}.items():
            self.assertEqual(importlib.metadata.version(name), version)

    async def test_real_deepagents_registry_schemas_and_original_restoration(self):
        native = await self.runtime("deepagents")
        original = await self.runtime("deepagents", native=False)
        self.assertEqual(native.catalog(), original.catalog())
        self.assertEqual(set(native.catalog()), {"read_file", "write_file", "edit_file", "delete", "ls", "glob", "grep", "execute"})
        self.assertEqual((await native.write("a", "old\r\n"))["status"], "success")
        self.assertEqual((await native.edit("a", "old", "new"))["status"], "success")
        self.assertEqual((self.root / "a").read_bytes(), b"new\n")
        process = native.owner.client.process
        await native.__aexit__(None, None, None)
        self.assertIsNotNone(process.poll())
        self.assertEqual((await original.edit("a", "new", "restored"))["status"], "success")
        self.assertEqual((self.root / "a").read_bytes(), b"restored\n")

    async def test_deepagents_permissions_stop_native_io(self):
        from deepagents.middleware.filesystem import FilesystemPermission
        runtime = await self.runtime("deepagents", permissions=[
            FilesystemPermission(operations=["write"], paths=["/secret/**"], mode="deny")])
        before = dict(runtime.owner.client.actions)
        result = await runtime.write("secret/a", "blocked")
        self.assertEqual(result["status"], "error")
        self.assertIn("permission denied", result["content"])
        self.assertEqual(before, runtime.owner.client.actions)
        self.assertFalse((self.root / "secret").exists())

    async def test_deepagents_core_write_policy_and_virtual_paths(self):
        runtime = await self.runtime("deepagents", writable_roots=[])
        result = await runtime.write("a", "blocked")
        self.assertEqual(result["status"], "error")
        self.assertIn("host policy denies", result["content"])
        self.assertNotIn("unknown or consumed", result["content"])
        self.assertFalse((self.root / "a").exists())
        (self.root / "nested").mkdir()
        (self.root / "nested/file").write_text("retained")
        result = await runtime.call("delete", {"file_path": "/nested"})
        self.assertEqual(result["status"], "error")
        self.assertIn("writable", result["content"])
        self.assertEqual((self.root / "nested/file").read_text(), "retained")
        result = await runtime.write("../outside", "blocked")
        self.assertEqual(result["status"], "error")
        self.assertFalse((self.root.parent / "outside").exists())

    async def test_deepagents_raw_guard_retains_external_change(self):
        runtime = await self.runtime("deepagents")
        target = self.root / "a"
        target.write_bytes(b"old\r\n")
        request = runtime.owner.client.request
        for after_plan in [False, True]:
            target.write_bytes(b"old\r\n")
            def race(action, **kwargs):
                if action == "plan" and not after_plan:
                    target.write_bytes(b"external\r\n")
                result = request(action, **kwargs)
                if action == "plan" and after_plan:
                    target.write_bytes(b"external\r\n")
                return result
            with patch.object(runtime.owner.client, "request", side_effect=race):
                result = await runtime.edit("a", "old", "new")
            self.assertEqual(result["status"], "error")
            self.assertNotIn("unknown or consumed", result["content"])
            self.assertTrue("precondition" in result["content"] or "changed" in result["content"], result)
            self.assertEqual(target.read_bytes(), b"external\r\n")

    async def test_deepagents_binary_upload_delete_directory_and_noop(self):
        runtime = await self.runtime("deepagents")
        payload = bytes([255, 0, 13, 10])
        uploads = await runtime.owner.aupload_files([("/binary", payload)])
        self.assertIsNone(uploads[0].error)
        self.assertEqual((self.root / "binary").read_bytes(), payload)
        self.assertEqual(runtime.owner.download_files(["/binary"])[0].content, payload)
        self.assertIsNone(runtime.owner.delete("/binary").error)
        await runtime.write("nested/file", "same")
        self.assertEqual((await runtime.edit("nested/file", "same", "same"))["status"], "success")
        self.assertIsNone(runtime.owner.delete("/nested").error)
        self.assertFalse((self.root / "nested").exists())

    async def test_langgraph_native_registration_and_writable_scope(self):
        runtime = await self.runtime("langgraph")
        self.assertEqual(set(runtime.catalog()), {"xnip_read", "xnip_edit", "xnip_write"})
        self.assertEqual(set(runtime.graph.nodes), {"__start__", "tools"})
        await runtime.write("a", "old\r\n尾巴")
        await runtime.edit("a", "old", "new")
        self.assertEqual((self.root / "a").read_bytes(), "new\r\n尾巴".encode())
        self.assertEqual((await runtime.read("a"))["content"], "new\r\n尾巴")
        self.assertEqual((await runtime.edit("a", "new", "new"))["content"], "unchanged")
        (self.root / "large").write_text("a" * 11999 + "尾巴", encoding="utf-8")
        result = await runtime.read("large")
        self.assertEqual(result["content"], "a" * 11999 + "\n[truncated; next_offset=11999; total_bytes=12005]")
        result = await runtime.call("xnip_read", {"file_path": "large", "offset": 11999})
        self.assertEqual(result["content"], "尾巴")
        denied = await self.runtime("langgraph", writable_roots=[])
        result = await denied.write("a", "blocked")
        self.assertEqual(result["status"], "error")
        self.assertIn("host policy denies", result["content"])
        self.assertNotIn("unknown or consumed", result["content"])
        self.assertEqual((self.root / "a").read_bytes(), "new\r\n尾巴".encode())

    async def test_langgraph_wrapper_permission_runs_before_persistence(self):
        from langchain_core.messages import ToolMessage
        def deny(request, handler):
            return ToolMessage(content="Denied by application", tool_call_id=request.tool_call["id"],
                               name=request.tool_call["name"], status="error")
        runtime = await self.runtime("langgraph", wrap_tool_call=deny)
        before = dict(runtime.owner.client.actions)
        result = await runtime.write("a", "blocked")
        self.assertEqual(result["status"], "error")
        self.assertEqual(before, runtime.owner.client.actions)
        self.assertFalse((self.root / "a").exists())

    async def test_pydantic_actual_toolset_schema_hooks_and_shutdown(self):
        events = []
        async def process(ctx, call, name, arguments):
            events.append(name)
            return await call(name, arguments, metadata={"source": "xnip-sdk-e2e"})
        runtime = await self.runtime("pydantic-ai", process_tool_call=process)
        self.assertEqual(set(runtime.catalog()), {"xnip_apply", "xnip_edit", "xnip_find", "xnip_peek", "xnip_recover"})
        self.assertIn("file", runtime.catalog()["xnip_edit"]["parameters"]["required"])
        self.assertTrue(runtime.owner.is_running)
        await runtime.write("a", "old\r\n")
        await runtime.edit("a", "old", "new")
        self.assertEqual(events, ["xnip_edit", "xnip_edit"])
        self.assertEqual((self.root / "a").read_bytes(), b"new\r\n")
        await runtime.__aexit__(None, None, None)
        self.assertFalse(runtime.owner.is_running)

    async def test_pydantic_preview_guards_and_lifecycle(self):
        from pydantic_ai.exceptions import ModelRetry
        runtime = await self.runtime("pydantic-ai")
        await runtime.write("a", "old\r\n")
        preview = await runtime.call("xnip_edit", {"file": "a", "old": "old", "text": "preview", "dry_run": True})
        self.assertIn("preview", str(preview["content"]))
        self.assertEqual((self.root / "a").read_bytes(), b"old\r\n")
        with self.assertRaises(ModelRetry):
            await runtime.edit("a", "missing", "new")
        self.assertEqual((self.root / "a").read_bytes(), b"old\r\n")
        await runtime.call("xnip_edit", {"file": "a", "operation": "rename", "to": "b"})
        self.assertFalse((self.root / "a").exists())
        self.assertEqual((self.root / "b").read_bytes(), b"old\r\n")
        await runtime.call("xnip_edit", {"file": "b", "operation": "delete"})
        self.assertFalse((self.root / "b").exists())
        from pydantic_ai import Agent, RunContext
        from pydantic_ai.usage import RunUsage
        from xnip_python_agents.pydantic_ai import xnip_toolset
        structured = xnip_toolset(self.root, executable=BINARY, receipts="structured")
        context = RunContext(deps=None, model=runtime.model, usage=RunUsage(),
                             agent=Agent(model=runtime.model, toolsets=[structured]))
        async with structured:
            tools = await structured.get_tools(context)
            receipt = await structured.call_tool("xnip_edit", {"file": "structured", "operation": "write", "text": "new\r\n"},
                                                 context, tools["xnip_edit"])
            self.assertTrue(receipt["written"])
            self.assertEqual(receipt["mode"], "write")
        self.assertEqual((self.root / "structured").read_bytes(), b"new\r\n")

    async def check_fixture(self, framework, case):
        providers = [False, True] if framework == "deepagents" else [True]
        outcomes = []
        for native in providers:
            for name, content in case.files.items():
                path = self.root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(content.encode())
            runtime = await self.runtime(framework, native=native)
            for name in case.files:
                self.assertEqual((await runtime.read(name))["status"], "success")
            receipts = []
            for edit in case.edits:
                result = await runtime.edit(edit["file"], edit["old"], edit["text"], edit.get("all", False))
                self.assertEqual(result["status"], "success", result)
                receipts.append(result)
            expected = case.expected()
            if framework == "deepagents":
                expected = {name: text.replace("\r\n", "\n").replace("\r", "\n") for name, text in expected.items()}
            actual = {name: (self.root / name).read_bytes() for name in case.files}
            self.assertEqual(actual, {name: text.encode() for name, text in expected.items()})
            outcomes.append((actual, receipts))
        if len(outcomes) == 2:
            self.assertEqual(*outcomes)
        self.assertEqual(self.model_calls(), 0)


for framework in FRAMEWORKS:
    for case in scenarios():
        async def check(self, framework=framework, case=case):
            await self.check_fixture(framework, case)
        setattr(PythonAgentsContract, "test_" + framework.replace("-", "_") + "_bytes_" + case.name.replace("-", "_"), check)


if __name__ == "__main__":
    unittest.main()
