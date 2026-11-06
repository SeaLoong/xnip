# Supported integrations

The [integration catalog](../integrations/catalog.json) is the current list of
capability entries. `xnip doctor --native` uses the same catalog. Each
entry identifies its setup, tool surface, contracts and benchmark evidence;
several entries share a package or MCP transport.

The catalog contains 29 entries: nine host backends or registered-tool routes,
six official MCP SDK toolsets and fourteen client profiles.

Choose the route supported by your application:

| Route | Tool surface | Activation |
|---|---|---|
| Host provider or IO backend | Existing host tools or edit formats | Select the provider, extension or SDK backend |
| Plugin or framework tools | Additional `xnip_*` tools | Register the plugin or tool collection |
| Official MCP SDK toolset | The xnip MCP tools through the SDK's adapter | Open the SDK toolset/session |
| Client MCP profile | Client-namespaced xnip tools | Apply the client's reviewed server configuration |

## Host backends and registered tools

The linked guides describe verified versions, approval paths, storage policy
and restoration. Host matching and newline policies remain part of each route.

| Integration | Surface | Verification and measurements |
|---|---|---|
| [DeepSeek Harness](native.md) | Filesystem provider for existing `read`, `write`, `edit` tools | [Contracts](../tests/dsh_native_contract.mjs) · [Provider samples](benchmarks/native-provider-results.json) |
| [Pi](pi-native.md) | Existing `read`, `edit`, `write` through official IO extension points | [Contracts](../tests/pi_native_contract.mjs) · [Provider samples](benchmarks/pi-provider-results.json) |
| [Gemini SDK](gemini-native.md) | Local `Config` service for `read_file`, `replace`, `write_file` | [Contracts](../tests/gemini_native_contract.mjs) · [Provider samples](benchmarks/gemini-provider-results.json) |
| [Cline SDK](cline-native.md) | Official `apply_patch` executor; original `read_files` | [Contracts](../tests/cline_native_contract.mjs) · [Provider samples](benchmarks/cline-provider-results.json) |
| [OpenHands SDK](openhands-native.md) | Original `file_editor` definition and editor history | [Contracts](../tests/openhands_native_contract.py) · [Provider samples](benchmarks/openhands-provider-results.json) |
| [OpenCode](opencode.md) | Plugin tools `xnip_edit`, `xnip_write`, `xnip_apply`, `xnip_read` | [Contracts](../tests/opencode_native_contract.mjs) · [CLI tool samples](benchmarks/opencode-provider-results.json) |
| [aider](aider-native.md) | Original Coder edit formats through its IO adapter | [Contracts](../tests/aider_native_contract.py) · [Provider samples](benchmarks/aider-provider-results.json) |
| [DeepAgents](deepagents.md) | FilesystemMiddleware backend; core handles file mutations | [Contracts](../tests/python_agents_contract.py) · [Framework samples](benchmarks/python-agents-provider-results.json) |
| [LangGraph](langgraph.md) | StructuredTools `xnip_read`, `xnip_edit`, `xnip_write` through ToolNode | [Contracts](../tests/python_agents_contract.py) · [Framework samples](benchmarks/python-agents-provider-results.json) |

OpenCode's builtin editing tools remain available beside its xnip plugin tools.
LangGraph registers the selected tools in the application's graph. These
routes with registered tools and existing-tool backends have distinct
schemas and execution contracts.

## Official MCP SDK toolsets

The default server advertises `xnip_edit`, `xnip_apply`, `xnip_peek`,
`xnip_find` and `xnip_recover`; `--toolset full` selects ten tools. The SDKs
retain their own discovery, invocation, metadata and session lifecycle.
Client and SDK namespacing or renaming is recorded in each guide.

| Integration | SDK route | Verification and measurements |
|---|---|---|
| [PydanticAI](pydantic-ai.md) | Official MCPToolset and StdioTransport | [Contracts](../tests/python_agents_contract.py) · [Framework samples](benchmarks/python-agents-provider-results.json) |
| [OpenAI Agents SDK](agents-sdk.md) | SDK discovery, FunctionTool conversion and invocation | [Contracts](../tests/agents_sdk_contract.py) · [SDK samples](benchmarks/agents-sdk-provider-results.json) |
| [AutoGen AgentChat](autogen.md) | Official MCP workbench and tool invocation | [Contracts](../tests/autogen_contract.py) · [SDK samples](benchmarks/autogen-provider-results.json) |
| [smolagents](smolagents.md) | Official MCP tool adaptation and ToolCallingAgent execution | [Contracts](../tests/smolagents_contract.py) · [SDK samples](benchmarks/smolagents-provider-results.json) |
| [Google ADK](google-adk.md) | Official McpToolset, ToolContext and owned stdio sessions | [Contracts](../tests/google_adk_contract.py) · [SDK samples](benchmarks/google-adk-provider-results.json) |
| [CrewAI](crewai.md) | Official MCP resolver and BaseTools with fresh file-tool calls | [Contracts](../tests/crewai_contract.py) · [SDK samples](benchmarks/crewai-provider-results.json) |

Framework tool and MCP cohorts measure their actual registered execution
pipeline. Reports identify original-editor comparison lanes where the host
provides one, and record SDK/toolset exchange costs for the other routes.

## Client MCP profiles

The [profile guide](host-profiles.md) covers generated configuration, actual CLI
checks, stdio calls, client permissions and removal. Each profile launches
`xnip mcp --cwd ABS_PROJECT --receipts text`. The server owns the project
directory selection, including clients whose server schema exposes command and
arguments as the launch interface.

| Client | Configuration surface |
|---|---|
| [Codex](host-profiles.md#codex) | TOML `mcp_servers` |
| [Claude Code](host-profiles.md#claude-code) | Project `.mcp.json` |
| [Cursor](host-profiles.md#cursor) | `.cursor/mcp.json` |
| [VS Code](host-profiles.md#vscode) | `.vscode/mcp.json` `servers` |
| [GitHub Copilot](host-profiles.md#copilot) | VS Code MCP server configuration |
| [GitHub Copilot CLI](host-profiles.md#copilot-cli) | CLI MCP server configuration |
| [Roo Code](host-profiles.md#roo-code) | `.roo/mcp.json` |
| [Cline GUI](host-profiles.md#cline-gui) | Cline MCP settings |
| [Windsurf](host-profiles.md#windsurf) | MCP settings |
| [Goose](host-profiles.md#goose) | Stdio extension configuration |
| [Continue](host-profiles.md#continue) | Project MCP server YAML |
| [Zed](host-profiles.md#zed) | `context_servers` |
| [Gemini CLI](host-profiles.md#gemini-cli) | `.gemini/settings.json` |
| [Qwen Code](host-profiles.md#qwen-code) | `.qwen/settings.json` |

[Profile contracts](../tests/host_profiles_contract.py) and
[profile measurements](benchmarks/host-profiles-results.json) identify
configuration parsing, real CLI inspection and shared stdio tool exchanges.
Codex, Claude Code and Copilot CLI inspect their isolated server entries.
Goose's official 1.55 client bridge is recorded separately from shared stdio.

Generate a fresh review directory with the actual executable and project paths:

```sh
python3 integrations/host-profiles/xnip_profiles.py generate \
  --executable /absolute/path/to/xnip --project /absolute/project \
  --output target/xnip-client-profiles
```

The client guide describes where each reviewed configuration belongs. The
application retains ownership of server permissions, approval and removal.

## Runtime and installation

Node integrations use the prepared xnip executable. Python adapters use
isolated environments for their locked SDK dependencies:

| Python group | Environment boundary |
|---|---|
| OpenHands | Python 3.12+ and its own SDK/tools environment |
| aider | Python 3.13 and its own Coder dependency environment |
| DeepAgents / LangGraph / PydanticAI | Shared `python-agents` environment with MCP 2.3 |
| OpenAI Agents / AutoGen | Separate `agent-sdks` environment with MCP 1.30 |
| smolagents | Python 3.12+ and its own environment with MCP 1.30 |
| Google ADK | Its own SDK environment with MCP 2.3 |
| CrewAI | Its own SDK environment with MCP 1.30 |

Python adapters for native IO install the local
[`native-python`](../integrations/native-python) client together with the
selected adapter. Official MCP SDK packages use their framework's stdio client.
Run each guide's local installation command with that virtual environment's
Python interpreter. Follow the
[test setup](testing.md#native-host-e2e) for the complete matrix.

[Project instructions and skills](SKILL.md) add editing guidance to these
execution routes. The [benchmark index](benchmarks/README.md) separates current
integration measurements from the historical editing-executor archive.
