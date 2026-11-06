# OpenAI Agents SDK

The local `xnip-openai-agents` package supplies the official Agents SDK
`MCPServerStdio`, configured to run xnip in the application's project directory.
The SDK discovers and converts the five xnip tools into its normal
`FunctionTool` definitions. Editing and recovery use the existing xnip Rust
crate through stdio MCP.

Verified versions are **`openai-agents==0.24.0` and `mcp==1.30.0`**, with Python
3.10 or later. This is an SDK MCP integration. The application's existing model,
tools and Runner lifecycle retain their normal ownership.

## Install and attach

Build xnip, or select an installed standalone executable, then install the
local adapter into the SDK application's Python environment:

```sh
cargo build --release --locked
python -m pip install ./integrations/agents-sdk
```

```python
from agents import Agent
from xnip_agents import xnip_server

async with xnip_server(
    cwd="/absolute/project",
    executable="/absolute/path/to/xnip",
    require_approval="always",
) as server:
    agent = Agent(
        name="project_agent",
        model=model,
        instructions=instructions,
        mcp_servers=[*existing_mcp_servers, server],
    )
    # Continue the application's usual Runner lifecycle with this agent.
```

On Windows select an absolute `xnip.exe` path. `executable` defaults to `xnip`
on `PATH`. The child receives the selected working directory and
`XNIP_AUTO_UPDATE=0`. The default catalog is `xnip_edit`, `xnip_apply`,
`xnip_peek`, `xnip_find` and `xnip_recover`; `full=True` selects all ten tools.

Additional server keyword arguments pass through to the official
`MCPServerStdio`: tool filters, approval configuration, input/output guardrails,
tool metadata callbacks and result projection remain SDK features. The default
uses structured xnip receipts and caches the catalog. For example,
`tool_filter=create_static_tool_filter(allowed_tool_names=["xnip_peek", "xnip_find"])`
selects a read-only model tool list. Import that filter from `agents.mcp`.
Filesystem authority follows the process permissions; tool selection configures
the declarations offered by the SDK.

## Results and restoration

The SDK projects successful structured receipts into its normal function-tool
output. Its default failure handler produces SDK error output; applications
that choose `failure_error_function=None` receive the SDK's MCP exceptions.
Approval and guardrails execute in the application's normal Runner lifecycle.

Closing the context closes the official SDK stdio session and its xnip child.
Construct the next agent with the application's earlier MCP server list to
restore the original tools. Other server registrations and function tools stay
under application control. The local package is removed with
`python -m pip uninstall xnip-openai-agents`.

## Real-tool verification and benchmark

Use Python 3.11 or later for the frozen verification environment; its complete
dependency lock selects that minimum. The local adapter's declared Python
minimum remains 3.10, subject to the application's selected dependency closure.

```sh
python benches/native/agent_sdks_setup.py
python benches/native/agent_sdks_setup.py --check
target/native-hosts/agent-sdks-openai-0.24.0-autogen-0.7.5/.venv/bin/python tests/agents_sdk_contract.py
```

The fixed host environment contains both Agents SDK and AutoGen for their
independent contracts. Windows uses `.venv/Scripts/python.exe`. The 21 checks
cover the 13 common byte fixtures, actual Agent tool discovery, schemas,
preview/check, backups, batch failure, file lifecycle, public tool filters and
approval metadata, subprocess cleanup and an installed local Python package.

The fixture enters public `Agent.get_all_tools()` and
`FunctionTool.on_invoke_tool()` with an idle configured Agent. It verifies
actual model-call counters remain zero. These deterministic calls cover tool
conversion and execution; full Runner approval decisions belong to the
application's model-driven lifecycle. The contract checks the SDK's preserved
`needs_approval` metadata separately.

[The SDK bridge benchmark](benchmarks/agents-sdk-provider-results.md) compares
the actual SDK function tool and direct xnip MCP using identical arguments and
an independent complete-file oracle. It records setup, read/edit timings,
actual returned projections and their byte counts. SDK projections and raw MCP
responses are separate measurement boundaries.

```sh
target/native-hosts/agent-sdks-openai-0.24.0-autogen-0.7.5/.venv/bin/python benches/agents_sdk_provider.py --executable target/release/xnip
```
