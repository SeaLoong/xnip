# OpenHands SDK native file editor

The local OpenHands adapter keeps the official `file_editor` tool and sends its
text reads and guarded writes to `xnip native serve`. The existing Rust `core`
prepares and commits each transaction. OpenHands owns matching, encoding
detection, history, undo, action/observation schemas, snippets, diff rendering,
tool hooks, confirmation, secret masking and per-file resource scheduling.

Support is verified with **`openhands-sdk==1.60.1` and
`openhands-tools==1.60.1`**, using Python 3.12 or later. This is an **L2′ local SDK
session integration**: a registered factory returns the original
`FileEditorTool` with a custom executor through the public `set_executor()` API.
The executor subclasses the official editor's public `read_file()` and
`write_file()` methods. The SDK factory alias is `xnip_file_editor`; the actual
model-visible tool remains `file_editor`.

## Load in an SDK application

Build xnip and install the local Python adapter into the application's Python
environment:

```sh
cargo build --release --locked
python -m pip install ./integrations/native-python ./integrations/openhands-native
```

Use the existing application's `llm`, workspace and other tool specifications.
Select the native factory in place of its `file_editor` specification:

```python
from openhands.sdk import Agent, Conversation, Tool
from xnip_openhands import register_xnip_file_editor

register_xnip_file_editor()
agent = Agent(
    llm=llm,
    tools=[
        *other_tools,
        Tool(name="xnip_file_editor", params={
            "executable": "/absolute/path/to/xnip/target/release/xnip",
        }),
    ],
)
conversation = Conversation(agent=agent, workspace="/absolute/project")
try:
    # Continue the application's usual send_message()/run() lifecycle.
    ...
finally:
    conversation.close()
```

On Windows use an absolute `xnip.exe` path. `executable` defaults to `xnip` on
`PATH`. The child uses the session workspace and `XNIP_AUTO_UPDATE=0`.

`allowed_edits_files` optionally retains the official executor's per-file
restriction. `writable_roots` optionally confines core commits to a list of
absolute directories; `[]` permits zero write targets, while the default `None`
uses the session process's normal filesystem authority. OpenHands' ordinary
confirmation and hooks remain around the selected executor in the agent loop.

## Restore and session scope

Close the native conversation, then select `Tool(name="file_editor")` in a new
conversation. The official default factory remains registered throughout; the
adapter adds a separate factory alias. Conversation shutdown calls the
executor's public `close()` method, closes child input and waits for its pending
transactions and process to finish. Removing the local Python package removes
the adapter from the application's environment.

An SDK application controls its own tool list. OpenHands GUI sessions and remote
agent servers require that application's corresponding adapter installation and
tool configuration. The DSH profile installer operates through DSH's own
lifecycle; OpenHands activation belongs to SDK conversation construction.

## Host behavior and storage

The tool exposes the original `view`, `create`, `str_replace`, `insert` and
`undo_edit` commands. Its `FileEditorAction`, `FileEditorObservation`, tool
description, annotations, resource declarations and renderers come from the
official tool factory. Matching retains OpenHands' unique-match and whitespace
fallback rules. Undo history stays with the official editor instance.

OpenHands reads text with Python's universal-newline handling. Consequently, an
edit to a CRLF or mixed-newline file writes its normalized LF text on Linux;
the native adapter retains that host policy. BOM and supported legacy encodings
follow the host's encoding detection and UTF-8 fallback. Directory listings,
image views, validation and the insert algorithm also retain their original
host implementation. The byte E2E suite compares both providers and records the
CRLF fixture against the host's explicit normalized-byte expectation.

Before matching, the adapter captures original storage bytes. A native write is
guarded against those bytes, so an external change between matching and commit
produces an error and retains the external content. Creation uses core's
create-only operation. Existing live symlinks resolve to their regular targets;
atomic commits preserve ordinary permission bits. Core's transaction and
durability boundaries are described in [architecture](architecture.md#persistence-and-limits).

Core also enforces a target's read-only permission bits. On Linux, a mode `0400`
file in a writable parent directory can be atomically replaced by the original
OpenHands writer. The native writer instead returns the core read-only error
and retains that file's existing bytes. The SDK permission E2E records both
outcomes explicitly.

OpenHands' file-size and content validation still applies. Serialized native
requests have a 32 MiB bound; UTF-8 strings are negotiated for text, and binary
encoding guards use byte arrays. The child and its correlated reply reader are
shared by the SDK executor's worker threads. `interrupt()` cancels active plans;
the core commit receipt determines whether a racing completed write persisted.

## Reproduce actual SDK tool E2E

```sh
python benches/native/openhands_setup.py
python benches/native/openhands_setup.py --check
target/native-hosts/openhands-1.60.1/.venv/bin/python tests/openhands_native_contract.py
```

Windows uses `.venv/Scripts/python.exe`. Setup creates an isolated Python
environment, installs the complete dependency lock with `--require-hashes`, and
records package versions and official Python-source hashes in `native-host.json`.
`--destination` selects another prepared host directory. `XNIP_TEST_BINARY`
selects the executable used by the E2E suite.

The suite runs 24 checks: all 13 shared byte fixtures plus actual registry,
schema, result/rendering parity, default restoration, child exit, undo, encoding,
permissions, symlink, stale storage, hook, confirmation, parallel-tool and
interrupt cases. Both synchronous and asynchronous LLM completion entry points
are trapped and record zero calls.

`Conversation.execute_tool()` is the official tool-only API and skips agent-loop
confirmation/security analysis. The governance checks separately feed real SDK
`ActionEvent` objects through the actual hook processor and agent dispatch;
confirmation is evaluated by the SDK policy before the queued edit proceeds.
This exercises the SDK tool pipeline with an idle model.
