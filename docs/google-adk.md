# Google ADK

The local `xnip-google-adk` package supplies Google's public `McpToolset` to
an ADK `LlmAgent`. ADK owns tool discovery, function declarations, filtering,
confirmation, authentication and result projection. xnip runs as the owned
stdio MCP child and applies edits through its existing Rust engine.

Verified versions are **Google ADK 2.13.0 and MCP 2.3.0**, tested on Python
**3.12 and 3.13**. ADK declares Python 3.11 or later; the frozen test environment uses
Python 3.12–3.13. The package selects ADK's `mcp` extra. Its entry point is an MCP
toolset for applications using ADK's public Python SDK.

## Install and attach

```sh
cargo build --release --locked
python -m pip install ./integrations/google-adk
```

```python
from google.adk.agents import LlmAgent
from xnip_google_adk import xnip_toolset

async with xnip_toolset(
    cwd="/absolute/project",
    executable="/absolute/path/to/xnip",
) as tools:
    agent = LlmAgent(
        name="project_agent",
        model=application_model,
        tools=[*application_tools, tools],
    )
    # Keep the application's ADK Runner and sessions inside this context.
```

The application supplies its model, sessions, Runner and other tools. Keep
the toolset context open throughout the agent's tool usage. On Windows use
an absolute `xnip.exe` path. `executable` defaults to `xnip` on `PATH`.

The child receives `xnip mcp --cwd /absolute/project`, its process working
directory is the same project, and `XNIP_AUTO_UPDATE=0` is set. The default
catalog is `xnip_edit`, `xnip_apply`, `xnip_peek`, `xnip_find` and
`xnip_recover`. `full=True` selects the ten-tool catalog.

## ADK options and lifecycle

Extra keyword arguments go directly to the public `McpToolset` constructor.
For example:

```python
async with xnip_toolset(
    cwd="/absolute/project",
    tool_filter=["xnip_peek", "xnip_edit"],
    tool_name_prefix="project",
    require_confirmation=True,
) as tools:
    agent = LlmAgent(name="project_agent", model=application_model, tools=[tools])
```

ADK applies the public prefix through `get_tools_with_prefix()` and its
normal agent registry. Its `ToolContext` requests confirmation before calling
MCP, then dispatches approved calls through `McpTool.run_async()`. The
application handles ADK's confirmation requests through its usual session
and event flow. ADK's public authentication options also remain available.
Operating-system process permissions define filesystem authority; `--cwd`
selects the directory used to resolve relative paths.

The context closes the real ADK toolset, MCP session and subprocess. Restore
the application's earlier `tools` list when constructing its next agent to
remove this integration. The model and application session lifecycle remain
owned by the application. Remove the local package with
`python -m pip uninstall xnip-google-adk`.

## Result projection

Successful ADK calls retain xnip's MCP content and `structuredContent`,
including journal and written-state information. With MCP 2.3.0 the raw
`CallToolResult` additionally includes `resultType`; ADK's public projected
dictionary drops that MCP field. ADK's current error handling projects a
remote MCP exception into an `error` dictionary. Applications should consume
the actual ADK result at this boundary.

The benchmark records both this public projection and the actual underlying
MCP result or typed remote error. A delegated observer calls the original
MCP client method and captures its outcome; every measured call still
crosses the real stdio transport. This makes the two response layers
inspectable without replacing either SDK's implementation.

## Real-tool verification

```sh
python benches/native/google_adk_setup.py
python benches/native/google_adk_setup.py --check
target/native-hosts/google-adk-2.13.0/.venv/bin/python tests/google_adk_contract.py
target/native-hosts/google-adk-2.13.0/.venv/bin/python tests/google_adk_bench_contract.py
```

The setup creates its isolated environment under `target`, installs the
platform-universal hash-locked dependencies, verifies selected package versions, and
compares installed Google ADK/MCP Python sources against their wheel
receipts. `--check` checks the existing environment and receipt. Setup uses
the invoking Python interpreter; use Python 3.12–3.13 for a fresh cache.

The **20 E2E checks** use real `LlmAgent`, `McpToolset`, `ToolContext`,
session-service and MCP process instances. They cover the common 13 byte
fixtures, the actual registry/raw schemas/function declarations, confirmation
request/rejection/approval, public filtering and prefixing, the full catalog,
specific remote errors, preview and file lifecycle, toolset closure/reopening,
and a locally installed package. The actual model generation method counts
calls and raises if reached; all checks keep that count at zero. These checks
exercise tools and pre-model metadata; agent generation and model-driven tool
selection use the application's Runner.

The independent benchmark contract runs both providers on successful edits,
CRLF, creation and four rejection cases. It also alters captured evidence to
verify that a missing file oracle, a missing underlying RPC result, or an
infrastructure failure makes validation fail.

## Benchmark

[The ADK benchmark](benchmarks/google-adk-provider-results.md) compares the
actual public `McpTool.run_async()` path with a direct `ClientSession` using
the same server binary, parameters and file goals. The suite has **26
fixtures and 52 provider/fixture groups**: the original 13 byte fixtures,
BOM/CRLF and mixed line endings, a no-op, missing/ambiguous/stale guards,
preview, creation, a late batch failure, create/rename/delete, and 64 KiB,
1 MiB and 5 MiB files.

```sh
target/native-hosts/google-adk-2.13.0/.venv/bin/python benches/google_adk_provider.py \
  --executable target/release/xnip --repeats 3 --warmups 1
```

The JSON retains actual arguments, projected results, typed RPC errors,
complete file goals and byte hashes, SDK source and binary hashes, setup
costs, per-call timings and payload bytes. Rejected mutations require a
successful read prefix plus their specific remote error code and message.
Unexpected process, protocol and transport failures fail measurement. Both
providers must produce equal raw RPC outcomes and exact final bytes. Timing
includes the complete public call; fixture reset and file checks run outside
it. Each provider keeps one persistent session. The first setup includes
cold SDK imports, so setup timings describe the recorded process order.

Payload byte counts describe local JSON layers. Model calls stay at zero;
token billing and model latency belong to application-level measurements.

Official references: [Google ADK 2.13.0 on PyPI](https://pypi.org/project/google-adk/2.13.0/)
and [Google's ADK Python repository](https://github.com/google/adk-python).
The frozen installed-source receipts identify the exact API used here.
