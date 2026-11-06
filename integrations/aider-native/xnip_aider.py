"""Aider's scripting IO seam with xnip-owned guarded persistence.

Coder classes keep parsing, matching, approval, prompts, history and git policy.
The application owns its normal InputOutput instance and a native session.
"""
from __future__ import annotations

from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import dataclass, field
import os
from pathlib import Path
import threading

from aider.coders import Coder
from aider.io import InputOutput
from xnip_native_client import NativeClient

SUPPORTED_FORMATS = ("diff", "diff-fenced", "editor-diff", "editor-diff-fenced",
                     "whole", "editor-whole", "udiff", "udiff-simple")


@dataclass(eq=False)
class _Operation:
    snapshots: dict = field(default_factory=dict)
    cancelled: threading.Event = field(default_factory=threading.Event)
    plan_id: int | None = None


class XnipInputOutput:
    """Delegate Aider presentation and approval; bind raw reads to core writes."""

    def __init__(self, original, executable="xnip", cwd=None, writable_roots=None,
                 create_client=NativeClient):
        self.original = original
        self.executable = executable
        self.cwd = str(Path(cwd or original.root).resolve())
        self.writable_roots = writable_roots
        self.enabled = True
        self._create_client = create_client
        self._client = None
        self._lock = threading.RLock()
        self._active = set()
        self._operation = ContextVar("xnip_aider_operation")

    def __getattr__(self, name):
        return getattr(self.original, name)

    def __setattr__(self, name, value):
        original = self.__dict__.get("original")
        if original is not None and name not in self.__dict__ and hasattr(original, name):
            setattr(original, name, value)
        else:
            object.__setattr__(self, name, value)

    @property
    def client(self):
        with self._lock:
            if self._client is None:
                self._client = self._create_client(self.executable, self.cwd)
            return self._client

    @contextmanager
    def operation(self):
        current = _Operation()
        token = self._operation.set(current)
        with self._lock:
            self._active.add(current)
        try:
            yield current
        finally:
            with self._lock:
                self._active.remove(current)
            self._operation.reset(token)

    def read_text(self, filename, silent=False):
        current = self._operation.get(None)
        if self.enabled and current is not None:
            path = Path(filename).resolve()
            try:
                raw = path.read_bytes()
            except OSError:
                # The original reader supplies Aider's usual read diagnostics.
                pass
            else:
                current.snapshots.setdefault(path, raw)
        return self.original.read_text(filename, silent=silent)

    def write_text(self, filename, content, max_retries=5, initial_delay=0.1):
        if not self.enabled:
            return self.original.write_text(filename, content, max_retries=max_retries,
                                            initial_delay=initial_delay)
        if self.original.dry_run:
            return
        current = self._operation.get(None)
        if current is None:
            raise RuntimeError("Run Aider edits through the native session or io.operation()")
        if current.cancelled.is_set():
            raise InterruptedError("Aider edit cancelled")
        path = Path(filename).resolve()
        if path in current.snapshots:
            before = current.snapshots[path]
        else:
            try:
                before = path.read_bytes()
            except FileNotFoundError:
                before = None
        newline = self.original.newline
        translated = content.replace("\n", os.linesep if newline is None else newline)
        after = translated.encode(self.original.encoding)
        client = self.client
        operation = {"op": "create" if before is None else "write", "path": str(path),
                     "text": client.payload(after)}
        if before is not None:
            operation["old"] = client.payload(before)
        plan = client.request("plan", operations=[operation], **client.encoding)
        with self._lock:
            current.plan_id = plan["plan_id"]
        try:
            if current.cancelled.is_set():
                raise InterruptedError("Aider edit cancelled")
            client.request("commit", plan_id=current.plan_id,
                           policy={"read_only": False, "writable_roots": self.writable_roots})
        except BaseException:
            try:
                client.request("abort", plan_id=current.plan_id)
            except Exception:
                pass
            raise
        finally:
            with self._lock:
                current.plan_id = None
        current.snapshots[path] = after

    def cancel(self):
        with self._lock:
            for current in self._active:
                current.cancelled.set()
                if current.plan_id is not None:
                    self.client.send("abort", plan_id=current.plan_id)

    def restore(self):
        self.enabled = False

    def close(self):
        self.restore()
        if self._client is not None:
            self._client.close()


class NativeAiderSession:
    """An actual Coder and its scoped storage lifecycle."""

    def __init__(self, coder, io):
        self.coder, self.io = coder, io

    def run(self, message, **kwargs):
        if not message:
            raise ValueError("Provide one message per Aider scripting call")
        with self.io.operation():
            return self.coder.run(with_message=message, **kwargs)

    def run_stream(self, message):
        with self.io.operation():
            yield from self.coder.run_stream(message)

    def apply_response(self, response):
        """Apply an already generated Aider-format reply through its real pipeline."""
        self.coder.partial_response_content = response
        self.coder.multi_response_content = ""
        self.coder.reflected_message = ""
        with self.io.operation():
            return self.coder.apply_updates()

    def cancel(self):
        self.io.cancel()

    def restore(self):
        self.io.restore()

    def close(self):
        self.io.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def create_xnip_coder(*, executable="xnip", io=None, io_options=None,
                     writable_roots=None, create_client=NativeClient, **coder_options):
    """Use Coder.create's scripting IO parameter without changing its registry."""
    original = io if io is not None else InputOutput(**(io_options or {}))
    native = XnipInputOutput(original, executable, writable_roots=writable_roots,
                             create_client=create_client)
    coder = Coder.create(io=native, **coder_options)
    if coder.edit_format not in SUPPORTED_FORMATS:
        raise ValueError("Select an IO-backed Aider format: " + ", ".join(SUPPORTED_FORMATS))
    native.cwd = coder.root
    return NativeAiderSession(coder, native)
