"""Actual smolagents registry and tool dispatch through the xnip MCP process."""

from contextlib import contextmanager
import importlib.metadata
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "integrations/smolagents"))
sys.path.insert(0, str(ROOT / "benches"))
from edit_cost import scenarios
from xnip_smolagents import xnip_tools
import xnip_smolagents
from smolagents import Model, ToolCallingAgent
from smolagents.utils import AgentToolCallError, AgentToolExecutionError

BINARY = Path(os.environ.get("XNIP_TEST_BINARY", ROOT / "target/debug/xnip")).resolve()


class IdleModel(Model):
    def __init__(self):
        super().__init__()
        self.calls = 0

    def generate(self, *args, **kwargs):
        self.calls += 1
        raise AssertionError("smolagents tool tests stay outside model generation")

    def generate_stream(self, *args, **kwargs):
        self.calls += 1
        raise AssertionError("smolagents tool tests stay outside model streaming")


class Fixture:
    def __init__(self, collection):
        self.model = IdleModel()
        self.collection = collection
        self.agent = ToolCallingAgent(tools=list(collection.tools), model=self.model, verbosity_level=0)

    @property
    def model_calls(self):
        return self.model.calls

    def call(self, name, arguments):
        return self.agent.execute_tool_call(name, arguments)


@contextmanager
def observed_mcp_processes():
    import mcp.client.stdio
    processes = []
    original_spawn = mcp.client.stdio._create_platform_compatible_process

    async def spawn(*args, **kwargs):
        process = await original_spawn(*args, **kwargs)
        processes.append(process)
        return process

    with patch.object(mcp.client.stdio, "_create_platform_compatible_process", spawn):
        yield processes
        assert processes, "Actual SDK stdio must start its MCP subprocess"
        assert all(process.returncode is not None for process in processes), "SDK collection must settle every owned MCP subprocess"


@contextmanager
def fixture(cwd, executable=BINARY, full=False):
    """Benchmark entry: .call(name, arguments), .agent and .model_calls."""
    with observed_mcp_processes() as processes:
        with xnip_tools(executable, cwd, full=full) as collection:
            runtime = Fixture(collection)
            runtime.processes = processes
            try:
                yield runtime
            finally:
                assert runtime.model_calls == 0


def snapshot(root):
    return {path.relative_to(root).as_posix(): path.read_bytes()
            for path in root.rglob("*") if path.is_file()}


class SmolagentsContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="xnip-smolagents-")
        self.root = Path(self.temporary.name)
        self.addCleanup(self.temporary.cleanup)

    def test_actual_registry_schema_and_optional_arguments(self):
        with fixture(self.root) as runtime:
            self.assertEqual(set(runtime.agent.tools),
                {"xnip_edit", "xnip_apply", "xnip_peek", "xnip_find", "xnip_recover", "final_answer"})
            self.assertEqual(importlib.metadata.version("smolagents"), "1.26.0")
            tool = runtime.agent.tools["xnip_edit"]
            self.assertFalse(tool.inputs["file"]["nullable"])
            self.assertTrue(tool.inputs["all"]["nullable"])
            self.assertIsInstance(tool.output_schema, dict)
            (self.root / "a").write_bytes(b"old\r\n")
            result = runtime.call("xnip_edit", {"file": "a", "old": "old", "text": "new"})
            self.assertTrue(result["written"])
            self.assertEqual((self.root / "a").read_bytes(), b"new\r\n")
            self.assertIn("new", runtime.call("xnip_peek", {"file": "a", "all": True}))
            self.assertEqual(runtime.call("xnip_find", {"files": ["a"], "pattern": "new"}), "a:1:1\n")
            self.assertEqual(runtime.model_calls, 0)

    def test_required_and_unknown_tool_validation_uses_actual_dispatcher(self):
        with fixture(self.root) as runtime:
            with self.assertRaises(AgentToolCallError):
                runtime.call("xnip_edit", {"text": "new"})
            with self.assertRaises(AgentToolExecutionError):
                runtime.call("missing_tool", {})
            self.assertEqual(snapshot(self.root), {})

    def test_preview_check_and_noop_receipts(self):
        path = self.root / "a"
        path.write_bytes(b"old\r\n")
        with fixture(self.root) as runtime:
            for flag in ["dry_run", "check"]:
                result = runtime.call("xnip_edit", {"file": "a", "old": "old", "text": "new", flag: True})
                self.assertFalse(result["written"])
                self.assertEqual(path.read_bytes(), b"old\r\n")
            result = runtime.call("xnip_edit", {"file": "a", "old": "old", "text": "old"})
            self.assertFalse(result["written"])
            self.assertFalse(result["has_changes"])

    def test_mcp_tool_errors_reach_sdk_error_pipeline(self):
        (self.root / "a").write_text("same same\n")
        with fixture(self.root) as runtime:
            for old in ["missing", "same"]:
                with self.assertRaises(AgentToolExecutionError):
                    runtime.call("xnip_edit", {"file": "a", "old": old, "text": "new"})
            self.assertEqual((self.root / "a").read_text(), "same same\n")

    def test_create_rename_delete_and_full_catalog(self):
        with fixture(self.root, full=True) as runtime:
            self.assertIn("xnip_doctor", runtime.agent.tools)
            self.assertIn("xnip_replace", runtime.agent.tools)
            self.assertTrue(runtime.call("xnip_doctor", {}))
            runtime.call("xnip_edit", {"file": "a", "operation": "create", "text": "created\r\n尾巴"})
            runtime.call("xnip_edit", {"file": "a", "operation": "rename", "to": "b"})
            self.assertEqual(snapshot(self.root), {"b": "created\r\n尾巴".encode()})
            runtime.call("xnip_edit", {"file": "b", "operation": "delete"})
            self.assertEqual(snapshot(self.root), {})

    def test_multi_file_failure_keeps_initial_bytes(self):
        (self.root / "a").write_text("old\n")
        (self.root / "b").write_text("other\n")
        before = snapshot(self.root)
        with fixture(self.root) as runtime:
            with self.assertRaises(AgentToolExecutionError):
                runtime.call("xnip_apply", {"edits": [
                    {"file": "a", "old": "old", "text": "new"},
                    {"file": "b", "old": "missing", "text": "new"},
                ]})
            self.assertEqual(snapshot(self.root), before)

    def test_context_shutdown_closes_tool_session(self):
        (self.root / "a").write_text("old\n")
        connections = []
        original = xnip_smolagents.MCPAdapt
        def connection(*args, **kwargs):
            value = original(*args, **kwargs)
            connections.append(value)
            return value
        with patch.object(xnip_smolagents, "MCPAdapt", side_effect=connection):
            with fixture(self.root) as runtime:
                runtime.call("xnip_edit", {"file": "a", "old": "old", "text": "new"})
                self.assertTrue(connections[0].thread.is_alive())
        self.assertFalse(connections[0].thread.is_alive())
        self.assertTrue(connections[0].loop.is_closed())
        self.assertEqual((self.root / "a").read_text(), "new\n")

    def test_tool_conversion_failure_closes_actual_mcp_connection(self):
        connections = []
        original = xnip_smolagents.MCPAdapt
        def connection(*args, **kwargs):
            value = original(*args, **kwargs)
            connections.append(value)
            return value
        with patch.object(xnip_smolagents, "MCPAdapt", side_effect=connection), \
             patch.object(xnip_smolagents.XnipToolAdapter, "adapt", side_effect=ValueError("tool metadata conversion")), \
             observed_mcp_processes():
            with self.assertRaisesRegex(ValueError, "tool metadata conversion"):
                with xnip_tools(BINARY, self.root):
                    self.fail("the failing collection was exposed")
        self.assertEqual(len(connections), 1)
        self.assertFalse(connections[0].thread.is_alive())
        self.assertTrue(connections[0].loop.is_closed())

    def test_installed_local_package_loads_sdk_and_executes_tool(self):
        stage = self.root / "stage"
        installed = self.root / "installed"
        shutil.copytree(ROOT / "integrations/smolagents", stage,
                        ignore=shutil.ignore_patterns("__pycache__", "build", "*.egg-info"))
        subprocess.run([sys.executable, "-m", "pip", "install", "--disable-pip-version-check",
                        "--no-deps", "--no-build-isolation", "--target", str(installed), str(stage)],
                       check=True, capture_output=True, text=True)
        code = r'''
import sys
from pathlib import Path
sys.path.insert(0,sys.argv[1])
import xnip_smolagents
from smolagents import Model,ToolCallingAgent
assert Path(xnip_smolagents.__file__).is_relative_to(Path(sys.argv[1]))
class Idle(Model):
 def generate(self,*args,**kwargs): raise AssertionError('model call')
cwd=Path(sys.argv[3]); (cwd/'installed.txt').write_text('old\n')
with xnip_smolagents.xnip_tools(sys.argv[2],cwd) as collection:
 agent=ToolCallingAgent(tools=list(collection.tools),model=Idle(),verbosity_level=0)
 result=agent.execute_tool_call('xnip_edit',{'file':'installed.txt','old':'old','text':'new'})
 assert result['written']
assert (cwd/'installed.txt').read_bytes()==b'new\n'
print('installed-package-tool-executed')
'''
        result = subprocess.run([sys.executable, "-c", code, str(installed), str(BINARY), str(self.root)],
                                check=True, capture_output=True, text=True)
        self.assertIn("installed-package-tool-executed", result.stdout)

    def fixture_case(self, case):
        for name, content in case.files.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content.encode())
        with fixture(self.root) as runtime:
            if len(case.edits) == 1:
                result = runtime.call("xnip_edit", dict(case.edits[0]))
            else:
                result = runtime.call("xnip_apply", {"edits": case.edits})
            self.assertTrue(result["written"])
            self.assertEqual(snapshot(self.root), {name: content.encode() for name, content in case.expected().items()})
            self.assertEqual(runtime.model_calls, 0)


for case in scenarios():
    setattr(SmolagentsContract, "test_bytes_" + case.name.replace("-", "_"),
            lambda self, case=case: self.fixture_case(case))


if __name__ == "__main__":
    unittest.main()
