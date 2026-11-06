# DeepAgents filesystem backend

The local `XnipFilesystemBackend` implements DeepAgents **0.7.34**'s public
backend seam. `FilesystemMiddleware` keeps its original tool definitions,
permission rules, descriptions, read formatting and result messages. Regular
file writes, edits, binary uploads and deletes use the existing Rust `core`
through `xnip native serve`.

## Use in an application

Use Python 3.11 or later, build xnip, then install both local distributions:

```sh
cargo build --release --locked
python -m pip install ./integrations/native-python './integrations/python-agents[deepagents]'
```

Select the backend when constructing the application's normal DeepAgent:

```python
from deepagents import create_deep_agent
from xnip_python_agents.deepagents import XnipFilesystemBackend

with XnipFilesystemBackend(
    "/absolute/project",
    executable="/absolute/path/to/xnip",
) as backend:
    agent = create_deep_agent(model=model, backend=backend)
    # Use the application's ordinary invoke()/ainvoke() lifecycle.
```

`model` belongs to the application. On Windows, choose an absolute `xnip.exe`
path. The backend owns one persistent subprocess and closes it at context exit.
To restore the original storage, select `deepagents.backends.FilesystemBackend`
when constructing the next agent.

An application that already constructs middleware can pass this backend to
`FilesystemMiddleware(backend=backend, permissions=...)`. The public
`FilesystemPermission` rules continue to authorize the original tool calls.
The SDK's normal parallel-file mutation wrapper and result policy remain in
the agent's middleware chain.

## Tools and storage behavior

The default middleware retains its eight declarations: `ls`, `read_file`,
`write_file`, `edit_file`, `delete`, `glob`, `grep` and `execute`. The selected
`FilesystemBackend` controls the available backend capabilities; this adapter
supplies file persistence. Shell execution belongs to the application's
execution backend.

| Operation | Implementation |
|---|---|
| Text edit matching | Official `perform_string_replacement`, including uniqueness and `replace_all` |
| Writes, edits and binary uploads | Core plans and guarded atomic persistence |
| Regular-file deletion | Core deletion with the raw old-content guard |
| Reads, listing, search and downloads | Original `FilesystemBackend` implementation |
| Directory/link deletion | Original filesystem backend operation within the selected writable roots |
| Path resolution | The pinned `FilesystemBackend` resolver and virtual-root policy |

DeepAgents reads text using Python's universal-newline behavior. An edit
therefore writes LF text for a CRLF/mixed-EOL source, matching its original
backend. UTF-8 BOM and Unicode remain part of that text. The adapter captures
raw bytes before matching and checks them during core planning and commit, so
an external update produces a conflict.

`virtual_mode=True` is the SDK's default and confines resolved paths to
`root_dir`. Core writes also select that root. `writable_roots=[]` permits zero
write targets. `virtual_mode=False` follows the SDK's process-level filesystem
authority; an explicit `writable_roots` list can confine those writes.

Core persistence replaces regular files atomically and preserves ordinary
permissions. It needs read access to existing snapshots and write access to the
containing directory. These are the shared [persistence boundaries](architecture.md#persistence-and-limits).
SDK backend methods have no cancellation signal; already-started IO settles
before `close()` drains the child. Requests are bounded by the native protocol's
32 MiB serialized-line limit.

## Verification and benchmark

```sh
python benches/native/python_agents_setup.py
python benches/native/python_agents_setup.py --check
target/native-hosts/python-agents/.venv/bin/python tests/python_agents_contract.py
target/native-hosts/python-agents/.venv/bin/python tests/python_agents_bench_contract.py
```

On Windows use `.venv/Scripts/python.exe`. The frozen host also contains the
LangGraph and PydanticAI adapters. It installs the two actual local packages,
checks official wheel source hashes and dependency versions, and retains its
receipt under `target/native-hosts/python-agents`.

The contract enters a compiled `StateGraph` and actual `ToolNode` with the
original filesystem middleware wrappers. It checks registry/schema equality,
permissions, byte guards, binary uploads, restoration, and the original 13 byte
fixtures. Actual model-generation methods are trapped and remain at zero calls.
The [Python SDK benchmark](benchmarks/python-agents-provider-results.md) compares
the original and xnip-backed backends through the same official tools; it keeps
the literal byte goal and the SDK's newline policy as separate oracles.
