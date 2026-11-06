"""OpenHands SDK file_editor using xnip's guarded core transactions.

The SDK owns tool schemas, validation, matching, history, rendering, hooks,
confirmation and resource scheduling. This module supplies public file IO and
executor lifecycle methods. The original file_editor registration stays intact.
"""

from __future__ import annotations

from concurrent.futures import Future
from contextvars import ContextVar
from dataclasses import dataclass, field
import io
import os
from pathlib import Path
import threading

from xnip_native_client import BridgeError, NativeClient

from openhands.sdk.tool import register_tool
from openhands.tools.file_editor import FileEditorTool
from openhands.tools.file_editor.editor import FileEditor
from openhands.tools.file_editor.exceptions import ToolError
from openhands.tools.file_editor.impl import FileEditorExecutor
from openhands.tools.file_editor.utils.encoding import with_encoding


FACTORY_NAME = "xnip_file_editor"


@dataclass(eq=False)
class _Call:
    before: bytes | None
    target: Path
    sdk_token: object = None
    cancelled: threading.Event = field(default_factory=threading.Event)
    plan_id: int | None = None
    abort_reply: Future | None = None

    @property
    def is_cancelled(self):
        return self.cancelled.is_set() or (self.sdk_token is not None and self.sdk_token.is_cancelled)


_call = ContextVar("xnip_openhands_call")
_sdk_token = ContextVar("xnip_openhands_sdk_token", default=None)


class XnipFileEditor(FileEditor):
    def __init__(self, executor, **kwargs):
        super().__init__(**kwargs)
        self.storage = executor

    def __call__(self, **kwargs):
        path = Path(kwargs["path"])
        # Capture the storage guard before the host computes its replacement.
        self.validate_path(kwargs["command"], path)
        target = path.resolve() if path.is_symlink() else path
        try:
            before = (self.storage.client.read(target)
                      if kwargs["command"] != "view" and target.is_file() else None)
        except Exception as error:
            raise ToolError(f"Ran into {error} while trying to read {path}") from None
        call = _Call(before, target, _sdk_token.get())
        token = _call.set(call)
        with self.storage.lock:
            self.storage.calls.add(call)
        try:
            return super().__call__(**kwargs)
        finally:
            with self.storage.lock:
                self.storage.calls.remove(call)
            _call.reset(token)

    @with_encoding
    def read_file(self, path, start_line=None, end_line=None, encoding="utf-8"):
        self.validate_file(path)
        try:
            # TextIO's universal-newline rules match the official open() reader.
            with io.StringIO(self.storage.client.read(path).decode(encoding), newline=None) as stream:
                if start_line is not None and end_line is not None:
                    return "".join(line for i, line in enumerate(stream, 1)
                                   if start_line <= i <= end_line)
                if start_line is not None or end_line is not None:
                    raise ValueError("Both start_line and end_line must be provided together")
                return stream.read()
        except Exception as error:
            raise ToolError(f"Ran into {error} while trying to read {path}") from None

    @with_encoding
    def write_file(self, path, file_text, encoding="utf-8"):
        self.validate_file(path)
        call = _call.get()
        try:
            if call.is_cancelled:
                raise BridgeError("File editor operation cancelled", "cancelled")
            # The host transcodes an unrepresentable legacy encoding to UTF-8.
            translated = file_text.replace("\n", os.linesep)
            try:
                content = translated.encode(encoding)
            except UnicodeEncodeError:
                content = translated.encode("utf-8")
            operation = {"op": "write" if call.before is not None else "create",
                         "path": str(call.target), "text": self.storage.client.payload(content)}
            if call.before is not None:
                operation["old"] = self.storage.client.payload(call.before)
            plan = self.storage.client.request("plan", operations=[operation],
                                               **self.storage.client.encoding)
            with self.storage.lock:
                call.plan_id = plan["plan_id"]
            try:
                if call.is_cancelled:
                    with self.storage.lock:
                        if call.abort_reply is None:
                            call.abort_reply = self.storage.client.send("abort", plan_id=call.plan_id)
                    call.abort_reply.result()
                    raise BridgeError("File editor operation cancelled", "cancelled")
                try:
                    self.storage.client.request("commit", plan_id=call.plan_id,
                        policy={"read_only": False, "writable_roots": self.storage.writable_roots})
                except BridgeError as error:
                    abort_reply = call.abort_reply
                    if (call.is_cancelled and error.code == "usage"
                            and str(error) == f"unknown or consumed plan {call.plan_id}"
                            and abort_reply is not None and abort_reply.done()
                            and abort_reply.exception() is None
                            and abort_reply.result().get("aborted") is True):
                        raise BridgeError("File editor operation cancelled", "cancelled") from None
                    raise
            finally:
                with self.storage.lock:
                    call.plan_id = None
        except Exception as error:
            raise ToolError(f"Ran into {error} while trying to write to {path}") from None


class XnipFileEditorExecutor(FileEditorExecutor):
    def __init__(self, workspace_root, executable, allowed_edits_files=None, writable_roots=None):
        super().__init__(workspace_root, allowed_edits_files)
        self.client = NativeClient(executable, workspace_root)
        self.lock = threading.RLock()
        self.calls = set()
        self.writable_roots = writable_roots
        self.editor = XnipFileEditor(self, workspace_root=workspace_root)

    def __call__(self, action, conversation=None):
        cancellation = conversation.cancel_token if conversation is not None else None
        if cancellation is not None and cancellation.is_cancelled:
            return self._cancelled_observation(action)
        token = _sdk_token.set(cancellation)
        try:
            return super().__call__(action, conversation)
        finally:
            _sdk_token.reset(token)

    @staticmethod
    def _cancelled_observation(action):
        from openhands.tools.file_editor.definition import FileEditorObservation
        return FileEditorObservation.from_text(text="File editor operation cancelled",
                                               command=action.command, is_error=True)

    def interrupt(self):
        with self.lock:
            for call in self.calls:
                call.cancelled.set()
                if call.plan_id is not None and call.abort_reply is None:
                    call.abort_reply = self.client.send("abort", plan_id=call.plan_id)

    def close(self):
        self.client.close()


class XnipFileEditorTool(FileEditorTool):
    name = FACTORY_NAME

    @classmethod
    def create(cls, conv_state, executable="xnip", allowed_edits_files=None, writable_roots=None):
        original = FileEditorTool.create(conv_state)[0]
        executor = XnipFileEditorExecutor(conv_state.workspace.working_dir, executable,
                                          allowed_edits_files, writable_roots)
        return [original.set_executor(executor)]


def register_xnip_file_editor():
    """Register a factory alias; models retain OpenHands' file_editor name."""
    register_tool(FACTORY_NAME, XnipFileEditorTool)
