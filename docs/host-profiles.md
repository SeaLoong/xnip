# Host MCP profiles

The profile generator connects the same xnip MCP tools to Codex, Claude Code,
Cursor, VS Code, GitHub Copilot Chat/CLI, Gemini CLI, Qwen Code, Roo Code,
Cline's IDE extension, Windsurf, Goose, Continue and Zed. It writes isolated
configuration artifacts for review. Each application keeps its server approval,
tool permissions, instructions, builtin tools and removal lifecycle.

The profiles select the five-tool catalog and text receipts. They pass an
absolute executable and `xnip mcp --cwd ABS_PROJECT`, so relative tool paths
resolve in the selected project even when the application launches the server
from another directory. `--cwd` selects path resolution; host permission rules
and the process's OS authority govern file access.

## Generate and inspect

Use Python 3.11 or later and a build containing `mcp --cwd`:

```sh
cargo build --release --locked
python integrations/host-profiles/xnip_profiles.py generate \
  --executable "$PWD/target/release/xnip" --project "$PWD" \
  --output "$PWD/target/xnip-host-profiles"
python integrations/host-profiles/xnip_profiles.py inspect --host cursor \
  --file target/xnip-host-profiles/cursor/.cursor/mcp.json
```

Select a fresh output directory for each render. Repeat `--host ID` to generate
only selected profiles. On Windows use the absolute `xnip.exe` path and the
corresponding PowerShell paths. The generator writes its output directory;
installation consists of merging the `xnip` server entry into the application's
configuration. Preserve the application's existing servers and settings.

The `.yaml` artifacts use JSON syntax, which their YAML loaders accept. `inspect`
reads these generated artifacts and prints the selected launch. The editing
guide and any existing host instructions remain separate optional additions.

## Codex

Generated artifact: `codex/config.toml`. Merge `[mcp_servers.xnip]` into the
active Codex configuration. Its official `cwd` field and the explicit server
argument select the same project. Start a new session to refresh tools.

The contract uses official **Codex CLI 0.167.0** and command-line `-c` overrides
to read only the supplied verification entry through `mcp get --json`. The
installed cloud CLI **0.166.0-alpha.1** was also checked during preparation.
This exercises the actual configuration reader without an agent turn or changes
to `HOME`, `CODEX_HOME` or user configuration. [Codex instructions](codex.md)
continue to support the ordinary-guide workflow.

Remove the `xnip` MCP entry to unregister the server, then start a new session.

## Claude Code

Generated artifact: `claude-code/.mcp.json`. Merge `mcpServers.xnip` into the
project `.mcp.json`, then approve that server through Claude Code's normal
project-server prompt. The existing [Claude skill](../integrations/claude-code/SKILL.md)
can be loaded alongside it.

Official **Claude Code 2.1.318** reads this actual project artifact in the
contract. `claude --setting-sources project mcp get xnip` reports **Pending
approval** before activation. The isolated verification uses
`CLAUDE_CONFIG_DIR` under its temporary test directory. Session
`--mcp-config`/`--strict-mcp-config` options belong to the session startup path;
the management check here reads the project file.

Remove `mcpServers.xnip` or use `claude mcp remove xnip -s project`, then reload
the session. Other servers and the skill remain under their own lifecycle.

## Cursor

Generated artifact: `cursor/.cursor/mcp.json`. Merge its `mcpServers.xnip` entry
into the project's `.cursor/mcp.json`. Enable it in Cursor's MCP settings and
refresh the tool list. Existing [Cursor rules](../integrations/cursor/xnip.mdc)
can continue to guide tool selection.

The tests execute the generated launch, discover the actual xnip schemas and
perform an edit. This is configuration/stdio evidence; Cursor's GUI and model
tool-selection loop have their own execution boundary. Remove the entry and
refresh MCP settings to restore the earlier server list.

## VSCode

Generated artifact: `vscode/.vscode/mcp.json`. Merge `servers.xnip` into the
workspace's `.vscode/mcp.json`. VS Code's MCP view starts the server and owns
trust/approval. The generated structure follows the official `servers`,
`type: "stdio"`, `command`, `args`, `env` and `cwd` schema.

The current official source revision and file hash are recorded in
[configuration evidence](../benches/native/host_profiles_sources.json).
Remove `servers.xnip` and stop/refresh the server through VS Code's MCP view.

## Copilot

Generated artifact: `copilot/.vscode/mcp.json`. GitHub Copilot Chat in VS Code
consumes the same workspace MCP configuration as the VS Code entry above.
These are two consumers of the same configuration interface and shared stdio
measurement lane. Existing [Copilot instructions](../integrations/copilot/xnip.md)
remain available.

Select the xnip tools in the chat tool picker. Remove `servers.xnip` and refresh
the server/tool picker to restore the earlier configuration.

## Copilot CLI

Generated artifact: `copilot-cli/mcp-config.json`. Merge `mcpServers.xnip` into
the CLI user configuration, normally `~/.copilot/mcp-config.json`. Its
`tools: ["*"]` field exposes the server's catalog; Copilot's ordinary tool
permission policy still applies.

Official **Copilot CLI 1.0.106** reads the artifact through `mcp get xnip --json`
in the contract. Its documented `COPILOT_HOME` selects an isolated task-owned
directory during verification. A regular one-session invocation can select the
file through `--additional-mcp-config`; the management-reader evidence uses the
user-config interface.

Remove the `xnip` entry through the MCP configuration or `copilot mcp remove
xnip`, then refresh the session's tools.

## Gemini CLI

Generated artifact: `gemini-cli/.gemini/settings.json`. Merge
`mcpServers.xnip` into the project's `.gemini/settings.json`, then use Gemini's
MCP settings/commands to approve and refresh the server. The profile follows
the official Gemini CLI `mcpServers` setting; its checked source is recorded in
the evidence file.

This is the ordinary CLI MCP route. The [native filesystem service](gemini-native.md)
is a separate local-SDK integration for applications that own `Config` and the
tool execution lifecycle. Remove `mcpServers.xnip` and refresh the CLI to restore
the earlier server list.

## Qwen Code

Generated artifact: `qwen-code/.qwen/settings.json`. Merge `mcpServers.xnip`
into the project's `.qwen/settings.json`, then use the CLI's MCP controls to
approve and refresh it. The official Qwen Code storage/configuration source
revision is recorded in the evidence file. Its public MCP format uses
`command`, `args` and `env` in the server entry.

The contract proves this configured server's real discovery and file tools.
Application-side Qwen execution remains a separate layer from that shared
stdio measurement. Remove the `xnip` entry and refresh the session to restore.

## Roo Code

Generated artifact: `roo-code/.roo/mcp.json`. Merge `mcpServers.xnip` into the
project's `.roo/mcp.json`. The official loader watches that project path. The
profile's empty `alwaysAllow` list retains user tool approval.

The official loader/schema revision is recorded in the evidence file; the
contract also starts its actual configured server and edits bytes. Remove the
entry and let Roo refresh its MCP connections.

## Cline GUI

Generated artifact: `cline-gui/cline_mcp_settings.json`. Open Cline's MCP
configuration editor and merge `mcpServers.xnip` into the selected
`cline_mcp_settings.json`. Cline's current IDE loader supports the generated
flat stdio format, with an empty `autoApprove` list.

Use the configuration editor to select the active file: current Cline releases
share/migrate configuration locations, so the application controls that path.
The [Cline SDK executor](cline-native.md) is a separate `apply_patch` route.
Remove the `xnip` entry in the editor to restore the prior GUI server list.

## Windsurf

Generated artifact: `windsurf/mcp_config.json`. Merge `mcpServers.xnip` through
Cascade's MCP settings, conventionally stored at
`~/.codeium/windsurf/mcp_config.json`. The profile uses the documented stdio
`command`/`args`/`env` format; the explicit xnip working directory supplies
relative-path resolution.

The recorded evidence is configuration plus real stdio discovery/edit. The
Windsurf GUI is outside that measured lane. Remove `mcpServers.xnip` and refresh
Cascade's MCP settings to restore the previous registration.

## Goose

Generated artifact: `goose/config.yaml`. Merge `extensions.xnip` into the Goose
configuration selected by the application. Its stdio extension is enabled and
uses the chosen executable, arguments, environment and working directory.
Keep the Developer extension when its shell/search tools are useful; xnip's
MCP tools have their own extension namespace.

Alongside the configured-server check, the tests and benchmark execute
**Goose 1.55.0's actual source-verified `McpClient`**. This covers connection,
discovery, calls and closure without constructing a model provider or Goose
agent. It retains the raw client DTO boundary, separately from GUI projection.
[Goose integration details](goose.md) describe that separation.

Remove `extensions.xnip` or disable it in Goose, then restart the selected
session. Editing journals and backups retain their separate [recovery](recovery.md)
lifecycle.

## Continue

Generated artifact: `continue/.continue/mcpServers/xnip.yaml`. Add that standalone
MCP block to the project's `.continue/mcpServers/` directory, preserving any
existing block with that name. Continue's public **config-yaml 1.23.0** schema
actually parses the generated artifact in the contract. Continue's agent UI
then owns tool selection and permissions.

Remove the added file, or merge out its `xnip` server entry, and refresh Continue
to restore the earlier toolset.

## Zed

Generated artifact: `zed/settings.json`. Merge `context_servers.xnip` through
**Settings → AI → MCP Servers** or the settings editor. Current Zed's official
format uses a string `command` plus `args` and `env`, as recorded in the pinned
official source documentation.

The configured launch is tested through real MCP. Remove
`context_servers.xnip` and refresh the MCP server in Zed to restore the earlier
configuration.

## Verification and measurements

Prepare the isolated official CLI packages and Goose client, then run:

```sh
python benches/native/host_profiles_setup.py
python benches/native/host_profiles_setup.py --check
python benches/native/goose_adapter.py --setup
python tests/host_profiles_contract.py
python tests/host_profiles_bench_contract.py
python benches/host_profiles.py --executable target/release/xnip --repeats 3 --warmups 1
```

The **33-check** contract covers all 14 configurations, three actual CLI
configuration readers, Continue's published schema, the original 13 byte
fixtures, preview, failed guards, lifecycle, explicit working directory and
the actual Goose client. Tests execute registered tools and retain zero model
calls. The setup uses frozen npm dependencies under
`target/native-hosts/profile-clis` and the existing pinned Goose fixture cache.
It leaves ordinary user configuration in place.

The [benchmark](benchmarks/host-profiles-results.md) measures one shared stdio
lane plus the actual Goose-client lane on the same independently checked files.
GUI profiles bind to the shared lane rather than receiving invented per-GUI
timings. Every sample starts a fresh connection, so connection and tool-call
timings keep distinct fields. Rejected Goose calls retain the actual typed remote
error and each client's elapsed call time. Their preceding reads must succeed;
the requested mutation must produce its expected guard/ambiguity error.
Connection, transport and subprocess failures stop the benchmark.
The raw JSON includes actual calls, file goals, CLI/config evidence and pinned
binary/source metadata. These are local bridge measurements with zero model
generations.
