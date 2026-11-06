"""DeepAgents' filesystem tools with xnip regular-file transactions."""
from __future__ import annotations

import io
from pathlib import Path

from deepagents.backends import FilesystemBackend
from deepagents.backends.protocol import DeleteResult, EditResult, FileUploadResponse, WriteResult
from deepagents.backends.utils import perform_string_replacement
from xnip_native_client import NativeClient


class XnipFilesystemBackend(FilesystemBackend):
    """Select through FilesystemMiddleware/backend or create_deep_agent/backend.

    FilesystemBackend continues to own path resolution, reads, listing, search,
    formatting and directory operations. The official replacement helper owns
    edit matching. xnip commits regular-file writes, edits, uploads and deletes.
    """

    def __init__(self, root_dir=None, *, executable="xnip", virtual_mode=True,
                 max_file_size_mb=10, writable_roots=None):
        super().__init__(root_dir, virtual_mode, max_file_size_mb)
        self.client = NativeClient(executable, self.cwd)
        self.writable_roots = ([str(self.cwd)] if virtual_mode else None) if writable_roots is None else writable_roots

    def _commit(self, operation):
        plan = self.client.request("plan", operations=[operation], **self.client.encoding)
        # Protocol 1 consumes a committed plan on both success and failure.
        return self.client.request("commit", plan_id=plan["plan_id"],
            policy={"read_only": False, "writable_roots": self.writable_roots})

    def write(self, file_path, content):
        try:
            path = self._resolve_path(file_path)
            self._commit({"op": "write", "path": str(path),
                          "text": self.client.payload(content.encode("utf-8"))})
            return WriteResult(path=file_path)
        except (OSError, RuntimeError, UnicodeEncodeError) as error:
            return WriteResult(error=f"Error writing file '{file_path}': {error}")

    def edit(self, file_path, old_string, new_string, replace_all=False):
        try:
            path = self._resolve_path(file_path)
            if not path.is_file():
                return EditResult(error=f"Error: File '{file_path}' not found")
            # Capture bytes before the official helper computes the replacement.
            before = self.client.read(path)
            with io.StringIO(before.decode("utf-8"), newline=None) as stream:
                content = stream.read()
            old_string = old_string.replace("\r\n", "\n").replace("\r", "\n")
            new_string = new_string.replace("\r\n", "\n").replace("\r", "\n")
            result = perform_string_replacement(content, old_string, new_string, replace_all)
            if isinstance(result, str):
                return EditResult(error=result)
            text, occurrences = result
            self._commit({"op": "write", "path": str(path),
                          "old": self.client.payload(before),
                          "text": self.client.payload(text.encode("utf-8"))})
            return EditResult(path=file_path, occurrences=occurrences)
        except (OSError, RuntimeError, UnicodeError) as error:
            return EditResult(error=f"Error editing file '{file_path}': {error}")

    def delete(self, file_path):
        try:
            path = self._resolve_path(file_path)
            if path.is_dir() or path.is_symlink():
                # Directory/link deletion remains the SDK operation, so apply
                # the selected write roots before delegating that mutation.
                entry = path.parent.resolve() / path.name
                if self.writable_roots is not None and not any(
                    entry.is_relative_to((self.cwd / Path(root)).resolve()) for root in self.writable_roots
                ):
                    return DeleteResult(error=f"Error deleting '{file_path}': outside writable roots")
                return super().delete(file_path)
            if not path.exists():
                return DeleteResult(error=f"Error: '{file_path}' not found")
            self._commit({"op": "delete", "path": str(path),
                          "old": self.client.payload(self.client.read(path))})
            return DeleteResult(path=file_path)
        except (OSError, RuntimeError) as error:
            return DeleteResult(error=f"Error deleting '{file_path}': {error}")

    def upload_files(self, files):
        results = []
        for name, content in files:
            try:
                path = self._resolve_path(name)
                self._commit({"op": "write", "path": str(path), "text": self.client.payload(content)})
                results.append(FileUploadResponse(path=name))
            except (OSError, RuntimeError, ValueError) as error:
                results.append(FileUploadResponse(path=name, error=str(error)))
        return results

    def close(self):
        self.client.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
