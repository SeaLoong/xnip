"""Register core-backed tools through LangGraph's ordinary ToolNode interface."""
from __future__ import annotations

from pathlib import Path
import codecs

from langchain_core.tools import tool
from xnip_native_client import NativeClient


class XnipLangGraphTools:
    """Own one persistent native subprocess and three LangChain StructuredTools."""

    def __init__(self, cwd, *, executable="xnip", writable_roots=None):
        self.cwd = Path(cwd).resolve()
        self.client = NativeClient(executable, self.cwd)
        self.writable_roots = [str(self.cwd)] if writable_roots is None else writable_roots

        def path(name):
            return str((self.cwd / name).resolve())

        def commit(operation):
            plan = self.client.request("plan", operations=[operation], **self.client.encoding)
            receipt = self.client.request("commit", plan_id=plan["plan_id"],
                policy={"read_only": False, "writable_roots": self.writable_roots})
            return "written" if receipt["written"] else "unchanged"

        @tool
        def xnip_read(file_path: str, max_bytes: int = 12000, offset: int = 0) -> str:
            """Read UTF-8 text from a byte offset; truncation reports the next offset. Zero max_bytes selects all bytes."""
            result = self.client.request("read", path=path(file_path), offset=offset,
                max_bytes=None if max_bytes == 0 else max_bytes, **self.client.encoding)
            raw = result["text"].encode() if "text" in result else bytes(result["bytes"])
            decoder = codecs.getincrementaldecoder("utf-8")("replace")
            text = decoder.decode(raw, final=not result["truncated"])
            next_offset = offset + len(raw) - len(decoder.getstate()[0])
            return text + (f"\n[truncated; next_offset={next_offset}; total_bytes={result['total_bytes']}]"
                           if result["truncated"] else "")

        @tool
        def xnip_edit(file_path: str, old: str, text: str, replace_all: bool = False) -> str:
            """Replace a known exact block; ordinary edits require a unique match."""
            return commit({"op": "exact_replace", "path": path(file_path),
                "old": self.client.payload(old.encode()), "text": self.client.payload(text.encode()),
                "all": replace_all})

        @tool
        def xnip_write(file_path: str, text: str) -> str:
            """Create or replace a complete UTF-8 text file."""
            return commit({"op": "write", "path": path(file_path), "text": self.client.payload(text.encode())})

        self.tools = [xnip_read, xnip_edit, xnip_write]

    def close(self):
        self.client.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
