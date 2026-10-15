# MCP Server (`xnip mcp`)

`xnip mcp` starts a [Model Context Protocol](https://modelcontextprotocol.io/) **stdio server** that exposes xnip's 10 capabilities (edit / peek / find / replace / insert / move / indent / apply / recover / doctor) as structured tools to LLM agents — eliminating the quote-escaping, exit-code parsing, and stdout/stderr confusion that comes with shell invocations.

## When to use MCP vs CLI

| Scenario | Recommended |
|---|---|
| Default editing with structured old/new content and an existing MCP connection | MCP |
| Manual terminal use, shell scripts, CI pipelines | CLI |
| Create/delete or exact block replacement with preview/check | `xnip_edit` or CLI `edit` on v0.2.0+ |
| Line/range/batch previews, revert or NDJSON | CLI |
| Need `--text-stdin` / `apply --from-stdin` / op-level `@-` | CLI (MCP process stdin is occupied by the protocol) |

Both paths share core editing algorithms. Input fields, error handling and available preview modes differ as described below.

## Starting the server

```sh
xnip mcp
# The process prints nothing. It waits for the client to feed JSON-RPC frames via stdin.
# Exit: Ctrl-D (close stdin) or client disconnect.
```

For the primary editing workflow use [xnip_edit](edit.md#mcp); it accepts literal old/new blocks and supports preview/check without writes. Reuse context already available rather than issuing a mandatory peek before every edit.

No sub-arguments. All "calls" arrive as `initialize` / `tools/list` / `tools/call` requests from the MCP client over stdio.

## Tool list

v0.2.0 has 10 tools; released v0.1.1 has 8 and does not include `xnip_edit` or `xnip_recover`. Names correspond to CLI subcommands:

| MCP Tool | CLI equivalent | Type |
|---|---|---|
| `xnip_edit` | `xnip edit` | create / exact block replace / delete; optional preview/check |
| `xnip_peek` | `xnip peek` | read-only |
| `xnip_find` | `xnip find` | read-only |
| `xnip_replace` | `xnip replace` | write (atomic) |
| `xnip_insert` | `xnip insert` | write (atomic) |
| `xnip_move` | `xnip move` | write (atomic) |
| `xnip_indent` | `xnip indent` | write (atomic) |
| `xnip_apply` | `xnip apply` | write (two-phase batch) |
| `xnip_recover` | `xnip recover` | conflict-checked rollback; optional dry-run |
| `xnip_doctor` | `xnip doctor` | diagnostic |

Most input fields use CLI names with underscores, for example `--match-line` becomes `match_line`. `xnip_edit` selects file lifecycle actions with `operation` and uses `to` for rename destinations, rather than CLI `--create`/`--delete`/`--rename-to` flags. Manifest JSON/YAML fields remain kebab-case.

### Differences from CLI

The older tools do **not** expose the following top-level flags; `xnip_edit` and `xnip_apply` are the exceptions for `dry_run` and `check`, and `xnip_recover` exposes `dry_run`:

- `--dry-run` — not available on older tools; the older MCP write tools commit immediately after validation. Use the CLI for previews.
- `--check` — not available; use CLI check mode when validation must not write.
- `--revert` — no top-level tool argument. An `xnip_apply` manifest may contain supported `revert` modifiers, with the same limitations as CLI apply.
- `--json` — MCP is already a structured JSON protocol; redundant
- `--text-stdin` / `apply --from-stdin` / op-level `@-` — MCP process stdin is occupied by the protocol

MCP **retains**:

- `was` / `was_file` (`xnip_replace`, line-based mode only) — check the original bytes; this is not a filesystem lock
- `backup` (write commands) — opt-in `.bak` copy as a safety escape hatch
- `manifest_text` (`xnip_apply` only) — inline manifest text, ideal for LLMs generating short manifests without writing a file first

## Result display

Available in current source, after the v0.2.0 release.

All tools advertise a human-readable `title` and read/write annotations. Tool names and arguments remain compatible. Titles are display hints: Codex and other clients choose whether to use them, how to show arguments, and whether to render Markdown. xnip cannot force a native editor card, hide the client's argument JSON or customize its layout through these fields. Write tools remain marked as potentially destructive even when an individual call requests a preview.

`xnip_edit`, `xnip_apply` and `xnip_recover` return a concise text summary plus `structuredContent`, described by their `outputSchema`:

| Field | Meaning |
|---|---|
| `operation` | `create`, `replace`, `delete`, `rename`, `apply` or `recover` |
| `mode` | `write`, `preview` or `check` |
| `has_changes` | Planned file content/existence differs from the starting state |
| `written` | This successful call committed changes; always false in preview/check |
| `files` | Complete target paths; single edit includes its target even when unchanged, rename includes both endpoints, batches/recovery include changed paths |
| `destination` | Rename destination, when applicable |
| `replacements` | Matched literal blocks for exact replacement, including identical replacements |
| `journal` | Retained recovery snapshot directory, when applicable |

Text starts with `Created`, `Updated`, `Deleted`, `Renamed`, `Restored`, `Unchanged`, `Preview (no files written)` or `Check passed (no files written)`. The human batch path list shows at most 20 entries with an explicit omitted count; `files` remains complete. A rename counts as two affected paths, not two independent edits. Display paths omit Windows extended-length prefixes; structured paths retain them. Non-UTF-8 filesystem paths use lossy text for display/metadata, as in other xnip reports.

Preview diffs remain complete and appear in a `diff` code block after the summary, with a fence long enough to contain source-code backticks. Diffs are not duplicated in structured metadata. Text is deliberately human-readable rather than a second serialized JSON copy; clients requiring machine-readable results should read `structuredContent`. Clients that ignore it still receive the summary and complete preview. Successful writes and checks do not include a diff. Read/search output and existing JSON-RPC error semantics remain unchanged.

```text
Renamed: src/old.rs -> src/new.rs

Preview (no files written): config.toml
  Operation: replace
  Matched blocks: 1
```

See the [MCP tool specification](https://modelcontextprotocol.io/specification/2025-06-18/server/tools) for the standard metadata. Restart/reconnect an existing MCP server after upgrading its executable so that the client discovers the new titles and schemas.

## Error semantics

Tool call failures return a JSON-RPC `error` object (not `result.isError=true`):

| Scenario | error.code | Meaning |
|---|---|---|
| Missing / conflicting / invalid-type params | `-32602` (`invalid_params`) | User input error; fix the args and retry |
| Locator not found / `was` mismatch / pattern no match | `-32600` (`invalid_request`) | Precondition not met; may need to `xnip_peek` current state first |
| File IO failure / `apply` phase-2 partial commit | `-32603` (`internal_error`) | System-level error; human intervention needed |

## Client configuration

### Claude Desktop / Claude Code

`~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or project-root `.mcp.json` (Claude Code):

```json
{
  "mcpServers": {
    "xnip": {
      "command": "xnip",
      "args": ["mcp"]
    }
  }
}
```

### Cursor

`.cursor/mcp.json` (project) or `~/.cursor/mcp.json` (global):

```json
{
  "mcpServers": {
    "xnip": { "command": "xnip", "args": ["mcp"] }
  }
}
```

### Cline / Continue

UI → Settings → MCP → Add server: command=`xnip`, args=`["mcp"]`.

### Zed

`~/.config/zed/settings.json`:

```json
{
  "context_servers": {
    "xnip": { "command": { "path": "xnip", "args": ["mcp"] } }
  }
}
```

### Manual debug (shell only, no client)

```sh
printf '%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | xnip mcp
```

You should see `serverInfo: {name: "xnip", version: "..."}` followed by a list of the tools supported by that build.

## Tool call examples

### `xnip_peek` — read before editing

```json
{
  "method": "tools/call",
  "params": {
    "name": "xnip_peek",
    "arguments": { "file": "src/main.rs", "match_line": "^fn main", "context": 2 }
  }
}
```

### `xnip_replace` — precise edit with expected content

```json
{
  "name": "xnip_replace",
  "arguments": {
    "file": "src/lib.rs",
    "lines": "12-14",
    "text": "// new implementation\nfn foo() { todo!() }\n",
    "was": "// old impl\nfn foo() { 1 + 1 }\n// trailing\n"
  }
}
```

If `was` does not match lines 12-14 literally → returns `invalid_request` error; file is not modified.

### `xnip_apply` — validated batch editing

```json
{
  "name": "xnip_apply",
  "arguments": {
    "manifest_text": "replace src/a.rs 30-32 \"...\"\ninsert src/b.rs 1 before \"// header\\n\"\n",
    "format": "native",
    "backup": true
  }
}
```

Phase 1: all ops are validated and staged; any failure → no file is written, error returned.
Phase 2: persist snapshots, commit targets individually, and attempt rollback on failure. Errors include a recovery journal; [recovery](recovery.md) checks snapshot integrity and external conflicts before restoration.

## Troubleshooting

| Symptom | Fix |
|---|---|
| Client reports "Failed to connect" | Run `command -v xnip` (POSIX) or `Get-Command xnip` (PowerShell); the `command` in client config must be an absolute path or be on the client's `PATH` |
| `tools/list` returns 0 tools | Run `xnip --version`, check the configured executable path, and restart the client |
| `xnip_apply` reports "manifest contains op content `@-`" | MCP does not read process stdin; use inline `text` or `text_file` instead |
| Edit written but LLM sees no change | Have the LLM call `xnip_peek` to re-read; do not let the LLM treat tool reply text as "current file state" |

## Internal architecture

`xnip mcp` and `xnip <other subcommands>` **share the same binary and the same core**. The MCP tool handlers are a "parallel frontend" to the CLI layer — they call `core::ops::*` / `apply::commit::*` directly; they do **not** spawn a subprocess or scrape CLI stdout. This means:

- A core-layer bug fix is immediately reflected in both paths
- Shared core operations and end-to-end tests cover byte transformations and file writes
- The Tokio runtime is created only for `xnip mcp`; binary size varies with target and build settings

Dependencies: `rmcp 1.7` (official Rust SDK) + `tokio 1` (`rt, macros, io-std`) + `schemars 1.2` (auto-generates tool input JsonSchema). MSRV 1.89 (required by the YAML parser).

## Working directory and trust

Relative paths are resolved from the server process working directory, which is chosen by the client. Configure that directory in the client when supported, or send absolute paths. `xnip_doctor` can check whether the current directory is writable. The server does not confine access to a repository and uses the launching user's permissions.

For batch edits, direct targets resolve against the working directory; manifest-relative payloads and file lists follow the [apply path rules](apply-format.md#paths-and-edit-ordering). A phase-2 error can leave files changed if rollback is incomplete. Inspect its rollback status and recovery journal before retrying; blindly replaying a partially committed manifest can apply an operation twice.
