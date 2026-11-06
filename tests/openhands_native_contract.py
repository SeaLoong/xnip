"""Real OpenHands 1.60.1 SDK tools and xnip transactions; model calls stay zero."""

import base64
import importlib.metadata
import json
import logging
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

os.environ["OPENHANDS_SUPPRESS_BANNER"] = "1"
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "integrations/openhands-native"))
sys.path.insert(0, str(ROOT / "integrations/native-python"))
sys.path.insert(0, str(ROOT / "benches"))

from edit_cost import scenarios
from xnip_openhands import FACTORY_NAME, register_xnip_file_editor
from openhands.sdk import Agent, Conversation, LLM
from openhands.sdk.event import ActionEvent, ObservationEvent, UserRejectObservation
from openhands.sdk.hooks import HookConfig
from openhands.sdk.llm import MessageToolCall
from openhands.sdk.security.confirmation_policy import AlwaysConfirm
from openhands.sdk.tool import Tool
from openhands.sdk.tool.registry import registered_tool_class
from openhands.tools.file_editor import FileEditorTool
from openhands.tools.file_editor.definition import FileEditorAction

logging.disable(logging.CRITICAL)
register_xnip_file_editor()
BINARY = Path(os.environ.get("XNIP_TEST_BINARY", ROOT / "target/debug/xnip")).resolve()


def conversation(cwd, native=True, **params):
    hooks = params.pop("hooks", None)
    tool = Tool(name=FACTORY_NAME, params={"executable": str(BINARY), **params}) if native else Tool(name="file_editor")
    agent = Agent(llm=LLM(model="openai/gpt-4o", api_key="tool-only"), tools=[tool])
    return Conversation(agent=agent, workspace=cwd, visualizer=None, hook_config=hooks)


def call(runtime, command, path, **params):
    return runtime.execute_tool("file_editor", FileEditorAction(command=command, path=str(path), **params))


def action_event(command, path, **params):
    action = FileEditorAction(command=command, path=str(path), **params)
    args = action.model_dump_json(exclude_none=True)
    tool_call = MessageToolCall(id="tool-" + os.urandom(6).hex(), name="file_editor", arguments=args, origin="completion")
    return ActionEvent(thought=[], action=action, tool_name="file_editor", tool_call_id=tool_call.id,
                       tool_call=tool_call, llm_response_id="tool-only-e2e")


class OpenHandsNativeContract(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="xnip-openhands-")
        self.root = Path(self.tmp.name)
        self.addCleanup(self.tmp.cleanup)
        self.model = patch.object(LLM, "completion", side_effect=AssertionError("Tool E2E performed model generation"))
        self.model_calls = self.model.start()
        self.addCleanup(self.model.stop)
        self.amodel = patch.object(LLM, "acompletion", side_effect=AssertionError("Tool E2E performed async model generation"))
        self.amodel_calls = self.amodel.start()
        self.addCleanup(self.amodel.stop)
        self.addCleanup(lambda: self.assertEqual(self.model_calls.call_count + self.amodel_calls.call_count, 0))

    def host(self, **params):
        runtime = conversation(self.root, **params)
        self.addCleanup(runtime.close)
        return runtime

    def initialize(self, runtime):
        call(runtime, "view", self.root)
        return runtime.agent.tools_map["file_editor"]

    def test_registry_schema_renderer_and_default_restoration(self):
        self.assertEqual(importlib.metadata.version("openhands-sdk"), "1.60.1")
        self.assertEqual(importlib.metadata.version("openhands-tools"), "1.60.1")
        baseline = self.host(native=False)
        original = self.initialize(baseline)
        native = self.host()
        tool = self.initialize(native)
        self.assertEqual(tool.to_mcp_tool(), original.to_mcp_tool())
        self.assertEqual(tool.description, original.description)
        self.assertEqual(type(tool), FileEditorTool)
        self.assertEqual(set(native.agent.tools_map), {"file_editor", "finish"})
        self.assertEqual(registered_tool_class("file_editor"), FileEditorTool)
        target = self.root / "restored.txt"
        self.assertFalse(call(native, "create", target, file_text="old").is_error)
        process = tool.executor.client.process
        native.close()
        self.assertEqual(process.poll(), 0)
        self.assertFalse(call(baseline, "str_replace", target, old_str="old", new_str="restored").is_error)
        self.assertEqual(target.read_bytes(), b"restored")
        # Install the actual local Python distribution, then load and use it in
        # an independent SDK process whose module path is the installed package.
        with tempfile.TemporaryDirectory(prefix="xnip-openhands-package-") as temporary:
            package_root = Path(temporary)
            source, installed = package_root / "source", package_root / "installed"
            shutil.copytree(ROOT / "integrations/openhands-native", source)
            client_source = package_root / "native-client"
            shutil.copytree(ROOT / "integrations/native-python", client_source)
            packed = subprocess.run([sys.executable, "-m", "pip", "install", "--no-deps", "--no-build-isolation",
                                     "--no-index", "--disable-pip-version-check", "--target", str(installed), str(client_source), str(source)],
                                    capture_output=True, text=True)
            self.assertEqual(packed.returncode, 0, packed.stdout + packed.stderr)
            code = '''
import json,logging,sys
from pathlib import Path
from unittest.mock import patch
logging.disable(logging.CRITICAL)
import xnip_openhands,xnip_native_client
from openhands.sdk import Agent,Conversation,LLM,Tool
from openhands.tools.file_editor.definition import FileEditorAction
assert Path(xnip_openhands.__file__).is_relative_to(Path(sys.argv[3]))
assert Path(xnip_native_client.__file__).is_relative_to(Path(sys.argv[3]))
xnip_openhands.register_xnip_file_editor()
with patch.object(LLM,'completion',side_effect=AssertionError('model call')) as sync, patch.object(LLM,'acompletion',side_effect=AssertionError('model call')) as async_:
 agent=Agent(llm=LLM(model='openai/gpt-4o',api_key='tool-only'),tools=[Tool(name='xnip_file_editor',params={'executable':sys.argv[1]})])
 c=Conversation(agent=agent,workspace=str(Path(sys.argv[2]).parent),visualizer=None)
 try:
  receipt=c.execute_tool('file_editor',FileEditorAction(command='str_replace',path=sys.argv[2],old_str='restored',new_str='packaged'))
  process=agent.tools_map['file_editor'].executor.client.process
  assert not receipt.is_error,receipt
 finally:c.close()
 print(json.dumps(dict(module=xnip_openhands.__file__,model_calls=sync.call_count+async_.call_count,exit_code=process.poll(),tools=list(agent.tools_map))))
'''
            smoke = subprocess.run([sys.executable, "-c", code, str(BINARY), str(target), str(installed)],
                                   cwd=package_root, env={**os.environ, "PYTHONPATH": str(installed)},
                                   capture_output=True, text=True)
            self.assertEqual(smoke.returncode, 0, smoke.stdout + smoke.stderr)
            evidence = json.loads(smoke.stdout)
            self.assertEqual(evidence["model_calls"], 0)
            self.assertEqual(evidence["exit_code"], 0)
            self.assertEqual(set(evidence["tools"]), {"file_editor", "finish"})
            self.assertEqual(target.read_bytes(), b"packaged")

    def test_create_replace_insert_undo_and_observation_parity(self):
        target = self.root / "edit.txt"
        args = [("create", dict(file_text="a\nb\n")),
                ("str_replace", dict(old_str="b", new_str="c")),
                ("insert", dict(insert_line=1, new_str="inserted")),
                ("undo_edit", {}), ("undo_edit", {})]
        outcomes = []
        for native in [False, True]:
            target.unlink(missing_ok=True)
            runtime = self.host(native=native)
            result = []
            for command, kwargs in args:
                receipt = call(runtime, command, target, **kwargs)
                self.assertFalse(receipt.is_error, receipt)
                result.append((receipt.model_dump(mode="json"), target.read_bytes(), receipt.visualize.plain))
            outcomes.append(result)
        self.assertEqual(*outcomes)

    def test_host_allowed_files_and_core_write_authority(self):
        allowed, other = self.root / "allowed", self.root / "other"
        allowed.write_text("old")
        other.write_text("old")
        runtime = self.host(allowed_edits_files=[str(allowed)])
        denied = call(runtime, "str_replace", other, old_str="old", new_str="new")
        self.assertTrue(denied.is_error)
        self.assertEqual(other.read_bytes(), b"old")
        self.assertFalse(call(runtime, "str_replace", allowed, old_str="old", new_str="new").is_error)
        confined = self.host(writable_roots=[])
        denied = call(confined, "str_replace", other, old_str="old", new_str="new")
        self.assertTrue(denied.is_error)
        self.assertIn("writ", denied.content[0].text.lower())
        self.assertEqual(other.read_bytes(), b"old")
        if os.name == "posix":
            readonly = self.root / "readonly"
            readonly.write_text("old")
            readonly.chmod(0o400)
            original = call(self.host(native=False), "str_replace", readonly, old_str="old", new_str="new")
            self.assertFalse(original.is_error, original)
            readonly.chmod(0o600)
            readonly.write_text("old")
            readonly.chmod(0o400)
            native = call(self.host(), "str_replace", readonly, old_str="old", new_str="new")
            self.assertTrue(native.is_error)
            self.assertEqual(readonly.read_bytes(), b"old")

    def test_bom_legacy_encoding_fallback_and_mixed_newlines(self):
        target = self.root / "encoding.txt"
        for initial, old, new in [(b"\xef\xbb\xbfhello\r\nend", "hello", "new"),
                                  ("café\n".encode("cp1252"), "café", "café → 尾"),
                                  (b"one\r\ntwo\nthree\r\n", "two", "changed")]:
            outcomes = []
            for native in [False, True]:
                target.write_bytes(initial)
                result = call(self.host(native=native), "str_replace", target, old_str=old, new_str=new)
                self.assertFalse(result.is_error, result)
                outcomes.append((result.model_dump(mode="json"), target.read_bytes()))
            self.assertEqual(*outcomes)

    def test_missing_ambiguous_and_noop_error_parity(self):
        target = self.root / "error.txt"
        target.write_text("old\nold\n")
        for old, new in [("missing", "new"), ("old", "new"), ("old", "old")]:
            original = call(self.host(native=False), "str_replace", target, old_str=old, new_str=new)
            native = call(self.host(), "str_replace", target, old_str=old, new_str=new)
            self.assertTrue(native.is_error)
            self.assertEqual(native.model_dump(mode="json"), original.model_dump(mode="json"))
            self.assertEqual(target.read_bytes(), b"old\nold\n")

    def test_read_pagination_directory_and_image_parity(self):
        target = self.root / "read.txt"
        target.write_bytes(b"one\r\ntwo\r\nthree")
        image = self.root / "image.png"
        image.write_bytes(base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+y2FEAAAAASUVORK5CYII="))
        for path, kwargs in [(target, {"view_range": [2, 2]}), (self.root, {}), (image, {})]:
            original = call(self.host(native=False), "view", path, **kwargs)
            native = call(self.host(), "view", path, **kwargs)
            self.assertEqual(native.model_dump(mode="json"), original.model_dump(mode="json"))

    def test_live_symlink_and_file_modes(self):
        target, link = self.root / "target", self.root / "link"
        target.write_text("old")
        target.chmod(0o754)
        link.symlink_to(target)
        result = call(self.host(), "str_replace", link, old_str="old", new_str="new")
        self.assertFalse(result.is_error, result)
        self.assertTrue(link.is_symlink())
        self.assertEqual(target.read_bytes(), b"new")
        self.assertEqual(target.stat().st_mode & 0o777, 0o754)

    def test_drift_between_matching_and_storage_is_guarded(self):
        target = self.root / "drift.txt"
        target.write_text("old")
        runtime = self.host()
        editor = self.initialize(runtime).executor.editor
        writer = editor.write_file
        def concurrent_write(*args, **kwargs):
            target.write_text("external")
            return writer(*args, **kwargs)
        with patch.object(editor, "write_file", side_effect=concurrent_write):
            result = call(runtime, "str_replace", target, old_str="old", new_str="new")
        self.assertTrue(result.is_error)
        self.assertEqual(target.read_bytes(), b"external")

    def test_sdk_confirmation_rejection_and_later_execution(self):
        target = self.root / "confirm.txt"
        target.write_text("old")
        runtime = self.host()
        self.initialize(runtime)
        runtime.set_confirmation_policy(AlwaysConfirm())
        event = action_event("str_replace", target, old_str="old", new_str="new")
        runtime._on_event(event)
        self.assertTrue(runtime.agent._requires_user_confirmation(runtime.state, [event], runtime._on_event))
        self.assertEqual(target.read_bytes(), b"old")
        runtime.reject_pending_actions("User retained the file")
        self.assertEqual(target.read_bytes(), b"old")
        event = action_event("str_replace", target, old_str="old", new_str="new")
        runtime._on_event(event)
        runtime.agent.step(runtime, runtime._on_event)
        self.assertEqual(target.read_bytes(), b"new")
        self.assertTrue(any(isinstance(e, UserRejectObservation) and e.rejection_source == "user" for e in runtime.state.events))

    def test_real_command_hook_blocks_then_parallel_dispatch_edits(self):
        target = self.root / "hook.txt"
        target.write_text("old")
        script = self.root / "policy.py"
        script.write_text('import sys\nsys.stdin.read()\nprint("host policy retained the file",file=sys.stderr)\nsys.exit(2)\n')
        hook = HookConfig.model_validate({"PreToolUse": [{"matcher": "file_editor", "hooks": [
            {"type": "command", "command": shlex.join([sys.executable, str(script)])}]}]})
        runtime = self.host(hooks=hook)
        self.initialize(runtime)
        runtime._on_event(action_event("str_replace", target, old_str="old", new_str="new"))
        runtime.agent.step(runtime, runtime._on_event)
        self.assertEqual(target.read_bytes(), b"old")
        self.assertTrue(any(isinstance(e, UserRejectObservation) and e.rejection_source == "hook" for e in runtime.state.events))
        post = self.root / "post.py"
        post_log = self.root / "post.jsonl"
        post.write_text('import sys,pathlib\np=pathlib.Path(' + repr(str(post_log)) + ')\nwith p.open("a") as f:f.write(sys.stdin.read()+"\\n")\n')
        post_hooks = HookConfig.model_validate({"PostToolUse": [{"matcher": "file_editor", "hooks": [
            {"type": "command", "command": shlex.join([sys.executable, str(post)])}]}]})
        plain = self.host(hooks=post_hooks)
        self.initialize(plain)
        second = self.root / "second.txt"
        second.write_text("old")
        for file in [target, second]:
            plain._on_event(action_event("str_replace", file, old_str="old", new_str="new"))
        plain.agent.step(plain, plain._on_event)
        self.assertEqual([target.read_bytes(), second.read_bytes()], [b"new", b"new"])
        self.assertEqual(len(post_log.read_text().splitlines()), 2)

    def test_executor_interrupt_aborts_actual_planned_write(self):
        target = self.root / "cancel.txt"
        # Exercise cancellation before the plan ID is adopted and while the
        # executor already owns an active plan ready to commit.
        for phase in ["plan", "commit"]:
            target.write_text("old")
            runtime = self.host()
            executor = self.initialize(runtime).executor
            planned, resume = threading.Event(), threading.Event()
            request = executor.client.request
            def gated_request(action, **kwargs):
                if action == "commit" and phase == "commit":
                    planned.set()
                    self.assertTrue(resume.wait(10))
                result = request(action, **kwargs)
                if action == "plan" and phase == "plan":
                    planned.set()
                    self.assertTrue(resume.wait(10))
                return result
            receipts = []
            with patch.object(executor.client, "request", side_effect=gated_request):
                thread = threading.Thread(target=lambda: receipts.append(call(runtime, "str_replace", target, old_str="old", new_str="new")))
                thread.start()
                self.assertTrue(planned.wait(10))
                with executor.lock:
                    self.assertEqual(len(executor.calls), 1)
                    active_call = next(iter(executor.calls))
                executor.interrupt()
                resume.set()
                thread.join(10)
                self.assertFalse(thread.is_alive())
            self.assertTrue(receipts[0].is_error)
            message = receipts[0].content[0].text
            self.assertIn("File editor operation cancelled", message)
            self.assertNotIn("unknown or consumed plan", message)
            self.assertEqual(target.read_bytes(), b"old")
            self.assertIsNotNone(active_call.abort_reply)
            aborted = active_call.abort_reply.result(timeout=10)
            self.assertIs(aborted["aborted"], True)
            self.assertIs(aborted["cancellation_requested"], True)
            self.assertEqual(executor.client.actions["abort"], 1)
            resumed = call(runtime, "str_replace", target, old_str="old", new_str="resumed")
            self.assertFalse(resumed.is_error, resumed)
            self.assertEqual(target.read_bytes(), b"resumed")

    def fixture(self, case):
        outcomes = []
        for native in [False, True]:
            for file in list(self.root.rglob("*")):
                if file.is_file():
                    file.unlink()
            for name, text in case.files.items():
                target = self.root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(text.encode())
            runtime = self.host(native=native)
            receipts = []
            for edit in case.edits:
                target = self.root / edit["file"]
                old, new = edit["old"].replace("\r\n", "\n"), edit["text"].replace("\r\n", "\n")
                if edit.get("all"):
                    source = target.read_text(encoding="utf-8")
                    old, new = source, source.replace(old, new)
                result = call(runtime, "str_replace", target, old_str=old, new_str=new)
                self.assertFalse(result.is_error, result)
                receipts.append(result.model_dump(mode="json"))
            files = {name: (self.root / name).read_bytes() for name in case.files}
            expected = {name: text.encode() for name, text in case.expected().items()}
            # The host's universal-newline policy is retained in both providers.
            if case.name == "crlf":
                expected = {name: data.replace(b"\r\n", b"\n") for name, data in expected.items()}
            self.assertEqual(files, expected)
            outcomes.append((receipts, files))
        self.assertEqual(*outcomes)


for fixture in scenarios():
    setattr(OpenHandsNativeContract, "test_bytes_" + fixture.name.replace("-", "_"),
            lambda self, case=fixture: self.fixture(case))

if __name__ == "__main__":
    unittest.main()
