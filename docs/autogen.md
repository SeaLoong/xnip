# Microsoft AutoGen AgentChat

The local `xnip-autogen` package creates the official AutoGen `McpWorkbench`
for xnip. An `AssistantAgent` receives the workbench through its normal SDK
constructor. Tool schemas, MCP dispatch, cancellation and result projection
remain owned by AutoGen; all edits use xnip's existing Rust engine.

Verified versions are **AgentChat/Ext 0.7.5 and MCP 1.30.0**, using Python 3.10
or later. The package pins the MCP v1 API used by the released AutoGen actor.
This is an AgentChat SDK workbench integration.

## Install and attach

```sh
cargo build --release --locked
python -m pip install ./integrations/autogen
```

```python
from autogen_agentchat.agents import AssistantAgent
from xnip_autogen import xnip_workbench

async with xnip_workbench(
    cwd="/absolute/project",
    executable="/absolute/path/to/xnip",
) as workbench:
    agent = AssistantAgent(
        "project_agent",
        model_client=model_client,
        workbench=workbench,
    )
    # Continue the application's usual AgentChat lifecycle.
```

The application selects and owns `model_client`. On Windows use an absolute
`xnip.exe` path. `executable` defaults to `xnip` on `PATH`; the child receives
`XNIP_AUTO_UPDATE=0` and the selected project directory. Default discovery
offers `xnip_edit`, `xnip_apply`, `xnip_peek`, `xnip_find` and `xnip_recover`.
`full=True` selects the ten-tool catalog.

Workbench options pass to the official constructor. For instance, AutoGen's
public `ToolOverride` from `autogen_core.tools` can select another public tool
name through `tool_overrides`. Existing workbenches can be retained in the
application's workbench list. The host remains responsible for agent-level
tool selection and approvals; process permissions define filesystem authority.

## Results and restoration

AutoGen converts MCP text/image blocks into `ToolResult.result` and preserves
the `is_error` flag. Its current workbench projects the content blocks rather
than xnip's additional `structuredContent`. This distinction appears in the
recorded SDK benchmark. The MCP server continues to report journal and error
text through those content blocks.

Closing the workbench context stops the SDK's MCP actor and stdio child. Use
the application's previous workbench list in a new agent to restore its
earlier tool configuration. Model-client shutdown remains the application's
responsibility. Remove the local adapter with
`python -m pip uninstall xnip-autogen`.

## Real-tool verification and benchmark

Use Python 3.11 or later for the frozen verification environment; its complete
dependency lock selects that minimum. The local adapter's declared Python
minimum remains 3.10, subject to the application's selected dependency closure.

```sh
python benches/native/agent_sdks_setup.py
python benches/native/agent_sdks_setup.py --check
target/native-hosts/agent-sdks-openai-0.24.0-autogen-0.7.5/.venv/bin/python tests/autogen_contract.py
```

The 21 checks construct real `AssistantAgent` and `McpWorkbench` instances,
discover their schemas and execute actual tools through public
`McpWorkbench.call_tool()`. They cover the common 13 byte fixtures, public
renaming, error flags, bounded reads, preview/check, backups, batch failure,
file lifecycle, actor cleanup and a locally installed Python package. Actual
model `create` and `create_stream` methods are trapped and remain at zero calls.
Full agent loops and model-driven approval choices remain separate from this
deterministic tool-only execution boundary.

[The AutoGen bridge benchmark](benchmarks/autogen-provider-results.md) measures
the real workbench and direct MCP on identical calls and independently checked
files. It retains each returned layer's actual projection and byte count,
setup cost, warm read/edit timings, and zero model calls.

```sh
target/native-hosts/agent-sdks-openai-0.24.0-autogen-0.7.5/.venv/bin/python benches/autogen_provider.py --executable target/release/xnip
```
