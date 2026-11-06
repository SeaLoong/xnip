# OpenCode

The xnip plugin adds `xnip_edit`, `xnip_write`, `xnip_apply` and `xnip_read`
through OpenCode's official plugin API. It uses the existing Rust `core` through
one plugin-owned `xnip native serve` process. OpenCode supplies permission
requests, tool registration, normal session hooks, cancellation and plugin
shutdown.

The plugin is verified with **OpenCode 1.19.3** and
**`@opencode-ai/plugin@1.19.3`**, official commit
[`e3c4eb242585dcd77be1a867391b7c25fd47ce52`](https://github.com/anomalyco/opencode/tree/e3c4eb242585dcd77be1a867391b7c25fd47ce52).
This is an additional native plugin tool surface. Existing OpenCode editing
tools retain their fuzzy matching, formatters, LSP updates and builtin UI.
The `xnip_*` tools use xnip's literal matching and transaction contracts.

## Activate the native plugin

Install `xnip-cli` in the project:

```sh
npm install xnip-cli
```

Add its absolute plugin entry to the existing OpenCode config's `plugin` list:

```json
{
  "plugin": [
    "file:///absolute/project/node_modules/xnip-cli/integrations/opencode/index.mjs"
  ]
}
```

On Windows use a file URL such as
`file:///C:/project/node_modules/xnip-cli/integrations/opencode/index.mjs`.
OpenCode provides its matching plugin SDK dependency during plugin loading.
The npm package export `xnip-cli/opencode` names the same entry for code that
resolves exports programmatically.

The package's default native resolver selects its checksum-verified executable
cache. Installing the npm package leaves plugin activation with the application.
Restart the session after changing its plugin list.

For a source checkout, build xnip with `cargo build --release --locked` and
create a local OpenCode plugin file:

```js
import { tool } from '@opencode-ai/plugin';
import { createXnipPlugin } from '/work/xnip/integrations/opencode/plugin.mjs';

export default createXnipPlugin({
  loadTool: async () => ({ tool }),
  resolveExecutable: () => '/work/xnip/target/release/xnip',
});
```

Select this file in the `plugin` list. Windows uses an absolute `xnip.exe` path.

## Tools and governance

| Tool | Arguments and behavior |
|---|---|
| `xnip_edit` | `filePath`, `oldString`, `newString`, optional `replaceAll`; unique exact byte replacement, or all non-overlapping matches |
| `xnip_write` | `filePath`, `content`, optional `oldContent`; complete UTF-8 upsert with an optional existing-content guard |
| `xnip_apply` | `patch`; unified diff or `*** Begin Patch` input under the session directory, planned as one transaction |
| `xnip_read` | `filePath`, optional byte `offset` and `maxBytes`; 12,000-byte default, visible continuation offset when truncated |

Relative paths use OpenCode's supplied session directory. Permission patterns
are relative to its actual worktree. Files, existing parent directories and
workspace roots are resolved before authority checks, including directory
symlinks and new targets. Every planned change is authorized before commit.
External targets use `external_directory`
requests; reads use `read`; planned mutations use `edit` with paths and the
core preview diff. Commit follows the host's decision and rechecks the plan's
raw before bytes. A rejected permission or cancelled signal releases its plan.
Read metadata calls the file-range flag `file_truncated`, keeping OpenCode's
reserved output-rendering `truncated` flag separate.

The usual OpenCode permission configuration applies:

```json
{
  "permission": {
    "edit": "ask",
    "xnip_apply": "ask"
  }
}
```

A tool-name rule controls availability. The `read`, `edit` and
`external_directory` requests govern the corresponding paths. Ordinary sessions
supply OpenCode's interaction and before/after hooks around execution.

The child runs with `XNIP_AUTO_UPDATE=0` and the session process's OS authority.
Its commit policy follows the host approval; xnip's ordinary atomic persistence,
file permissions, final-symlink and hard-link boundaries remain the
[persistence contract](architecture.md#persistence-and-limits). Request lines
have a 32 MiB serialized limit. The default xnip tool receipts are short
`written` or `unchanged` outputs; the host owns custom-tool presentation.

## MCP option and restoration

[The local MCP example](../integrations/opencode/opencode.json) uses OpenCode's
`mcp.xnip` format with `type: "local"` and a command array. Merge that entry into
existing config when selecting MCP tools. Native and MCP entries have their own
tool names and can be selected explicitly.

Remove the native plugin entry and restart the session to restore the previous
tool list. OpenCode calls `dispose()` on the plugin; it closes child input and
waits for pending transactions to settle. The original builtin tools stay
registered. Removing a selected `mcp.xnip` entry restores its previous MCP list.
The native profile installer applies to DSH; OpenCode uses its plugin lifecycle.

## Real tool verification and benchmark

```sh
node benches/native/opencode_native_setup.mjs
node benches/native/opencode_native_setup.mjs --check
node --test-reporter=tap tests/opencode_native_contract.mjs
node --test-reporter=tap tests/opencode_native_bytes.mjs
node --test-reporter=tap tests/opencode_native_packaged.mjs
node --test-reporter=tap tests/opencode_provider_bench_contract.mjs
```

Preparation freezes official npm inputs and verifies the CLI and SDK source
bytes under `target/native-hosts/opencode-1.19.3`. The fixture's executable is the
published Linux x64 payload, run directly with package install scripts disabled.
The plugin uses OpenCode's public API on its supported platforms; these retained
CLI test results use Linux x64.

The suite starts the real executable, loads an actual plugin file through its
loader, inspects the registry, and executes registered tools with
`opencode debug agent build --tool ... --params ...`. It checks native process
shutdown, path/tool permission denial, atomic patch failure, drift, malformed
input, binary reads, all 13 shared byte fixtures, the installed npm entry and
builtin restoration. The restoration scenario also starts the documented MCP
server through the actual OpenCode connection manager and removes its entry.
A configured local provider endpoint counts actual model
requests and stays at zero. A separate public-plugin callback contract verifies
cancellation of a real pending native plan.

The official debug command applies real **deny** rules; it has a deterministic
tool-only entry. Interactive ASK behavior and session before/after hooks belong
to the ordinary session pipeline. The pinned
[debug handler](https://github.com/anomalyco/opencode/blob/e3c4eb242585dcd77be1a867391b7c25fd47ce52/packages/opencode/src/cli/cmd/debug/agent.handler.ts)
and [plugin registry](https://github.com/anomalyco/opencode/blob/e3c4eb242585dcd77be1a867391b7c25fd47ce52/packages/opencode/src/tool/registry.ts)
record these boundaries. The custom tools use explicit names, so this path
selects the intended plugin implementation.

[Provider benchmark](benchmarks/opencode-provider-results.md) compares the
original `edit`/`write` tools and xnip plugin on identical byte goals. It records
full CLI invocation and stdout receipt capture per call, actual arguments/results, permission errors,
file hashes and zero model requests. One `xnip_apply` multi-file transaction is
compared with the corresponding original edit calls; call counts remain visible.
CLI receipts are captured through a regular-file stdout descriptor so the host's
immediate exit retains complete long-line JSON output. Process startup, signals,
timeouts and invalid JSON raise infrastructure failures; guard and permission
cases require their specific public error reasons. Reports retain actual tool
arguments and complete result payloads, with lossless gzip encoding for larger
receipts, plus recomputable hashes and the fixture-derived call sequence.
Both providers must meet the independent byte goal.

This is a tool-only CLI measurement. Warm in-session latency and model/billing
usage have their own measurement boundaries.
