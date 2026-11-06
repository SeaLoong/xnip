# smolagents MCP tools

The local `xnip-smolagents` package connects smolagents applications to the real
`xnip mcp` process. The SDK registers `xnip_edit`, `xnip_apply`, `xnip_peek`,
`xnip_find` and `xnip_recover`; the existing Rust core performs the edits.
The application retains its existing tools and model configuration.

Support is verified with **smolagents 1.26.0**, **mcpadapt 0.1.20** and the
**MCP Python client 1.30.0**. This is an SDK MCP integration. Applications select
the tool collection when constructing a `ToolCallingAgent` or `CodeAgent`.

## Install and load

Use Python 3.12 or later and an installed xnip executable. The pinned MCP adapter
`mcpadapt==0.1.20` selects that Python minimum. From a source checkout:

```sh
cargo build --release --locked
python -m pip install ./integrations/smolagents
```

Load the tools inside the application's existing agent lifecycle:

```python
from smolagents import ToolCallingAgent
from xnip_smolagents import xnip_tools

with xnip_tools(
    executable="/absolute/path/to/xnip",
    cwd="/absolute/project",
) as collection:
    agent = ToolCallingAgent(
        tools=[*existing_tools, *collection.tools],
        model=model,
    )
    # Continue the application's usual agent.run(...) lifecycle here.
```

On Windows, select an absolute `xnip.exe` path. `executable` defaults to `xnip`
on `PATH`. `full=True` selects all ten xnip tools, including line editing and
diagnostics. The normal five-tool collection is the default.

The factory uses the public `MCPAdapt` and `SmolAgentsAdapter` extension points
behind smolagents' MCP collection. Its schema translation preserves JSON Schema
`required` as smolagents' optional/`nullable` argument metadata and represents
zero-argument tools with empty properties. SDK argument validation then accepts
the same omitted flags as xnip. Advertised structured mutation receipts remain
objects; read/search output remains text. MCP errors reach the SDK's normal tool
error path. The selected MCP v1 client matches the adapter's public transport API.

## Session and authority

The context owns its MCP session and subprocess. Exit the context after the
agent has finished using the tools. To restore the prior tool list, create the
next agent from the application's original `existing_tools` list. The adapter
adds tools for that session; it leaves the host's default editing tools and
saved configuration available.

The child starts in the supplied project directory with `XNIP_AUTO_UPDATE=0`.
It uses the application's normal OS filesystem permissions. Applications apply
their tool-selection and approval policy before dispatch. The core's
[persistence boundaries](architecture.md#persistence-and-limits), preview/check
modes and recovery journals apply to these calls.

## Verification

Prepare the frozen official packages and run the real SDK suite:

```sh
python benches/native/smolagents_setup.py
python benches/native/smolagents_setup.py --check
target/native-hosts/smolagents-1.26.0/.venv/bin/python tests/smolagents_contract.py
```

Windows uses `.venv/Scripts/python.exe`. Preparation installs a universal hashed
dependency lock, verifies official SDK/client source bytes against wheel RECORD
hashes and records a host receipt under `target/native-hosts/smolagents-1.26.0`.

The suite executes the official `ToolCallingAgent.execute_tool_call()` dispatcher,
its actual registry, argument validation and the selected MCP tools. It covers
all thirteen common byte fixtures, structured receipts, read/search, required
and omitted arguments, preview/check/no-op, full-tool discovery, zero-argument
tools, file lifecycle, rejected batches, context shutdown and an installed local
Python package executing a real edit. A model subclass counts the SDK's actual
`generate` and `generate_stream` entry points; every tool test keeps both at zero.
The fixture is a tool-only SDK session with the same execution path available to
the application's agent.
