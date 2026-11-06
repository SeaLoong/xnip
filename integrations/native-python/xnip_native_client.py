"""Shared Python transport for xnip native sessions.

SDK adapters retain their host's tools and use this process client for core IO.
"""
from __future__ import annotations

from concurrent.futures import Future, wait
import json
import logging
import os
import subprocess
import threading

class BridgeError(RuntimeError):
    """A native subprocess error with its original machine-readable category."""

    def __init__(self, message, code="transport"):
        super().__init__(message)
        self.code = code


class NativeClient:
    """One session process; replies remain correlated across SDK worker threads."""

    def __init__(self, executable, cwd):
        self.process = subprocess.Popen(
            [str(executable), "native", "serve"], cwd=cwd,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            env={**os.environ, "XNIP_AUTO_UPDATE": "0"},
        )
        self._lock = threading.RLock()
        self._pending = {}
        self._next_id = 0
        self._closed = False
        self._failure = None
        self.request_bytes = self.response_bytes = 0
        self.actions = {}
        self._reader = threading.Thread(target=self._receive, daemon=True)
        self._reader.start()
        capabilities = self.request("capabilities")
        self.utf8 = "utf8" in capabilities.get("content_encodings", [])

    def _receive(self):
        try:
            for line in self.process.stdout:
                reply = json.loads(line)
                with self._lock:
                    self.response_bytes += len(line)
                    pending = self._pending.pop(reply["id"])
                if "error" in reply:
                    error = reply["error"]
                    pending.set_exception(BridgeError(error["message"], error["code"]))
                else:
                    pending.set_result(reply["result"])
            raise BridgeError("xnip native session reached EOF")
        except Exception as error:
            with self._lock:
                self._failure = error
                pending, self._pending = self._pending, {}
            for future in pending.values():
                future.set_exception(error)

    def send(self, action, **fields):
        with self._lock:
            if self._failure:
                raise self._failure
            if self._closed:
                raise BridgeError("xnip native session closed")
            self._next_id += 1
            request_id = self._next_id
            line = (json.dumps({"id": request_id, "protocol": 1, "action": action,
                                **fields}, ensure_ascii=False, separators=(",", ":")) + "\n").encode()
            if len(line) > 32 * 1024 * 1024:
                raise BridgeError("xnip native request exceeds 32 MiB", "usage")
            future = Future()
            self._pending[request_id] = future
            try:
                self.process.stdin.write(line)
                self.process.stdin.flush()
            except OSError:
                del self._pending[request_id]
                raise
            self.request_bytes += len(line)
            self.actions[action] = self.actions.get(action, 0) + 1
            return future

    def request(self, action, **fields):
        result = self.send(action, **fields).result()
        if action == "commit" and result.get("journal"):
            logging.getLogger(__name__).warning(
                "xnip: committed; recovery journal retained at %s", result["journal"])
        return result

    def drain(self):
        """Wait for already-issued replies, including asynchronous abort receipts."""
        with self._lock:
            pending = list(self._pending.values())
        wait(pending)

    def payload(self, content):
        if self.utf8:
            try:
                return content.decode("utf-8")
            except UnicodeDecodeError:
                pass
        return list(content)

    @property
    def encoding(self):
        return {"content_encoding": "utf8"} if self.utf8 else {}

    def read(self, path):
        result = self.request("read", path=str(path), **self.encoding)
        return result["text"].encode() if "text" in result else bytes(result["bytes"])

    def close(self):
        with self._lock:
            if self._closed:
                return
            self._closed = True
            self.process.stdin.close()
        self.process.wait()
        self._reader.join()
        self.process.stdout.close()
