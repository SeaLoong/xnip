# MCP Server (`xnip mcp`)

`xnip mcp` starts a [Model Context Protocol](https://modelcontextprotocol.io/) stdio server. It exposes five everyday tools (edit / apply / peek / find / recover) by default; `--toolset full` exposes all ten. Structured arguments carry multiline old/new text without shell escaping, and structured receipts identify whether a change was committed.

## When to use MCP vs CLI

| Scenario | Recommended |
|---|---|
| Default editing with structured old/new content and an existing MCP connection | MCP |
| Manual terminal use, shell scripts, CI pipelines | CLI |
| Create/delete or exact block replacement with preview/check | `xnip_edit` or CLI `edit` |
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

Use `xnip mcp --toolset full` when a client needs specialized replace/insert/move/indent or doctor tools. All calls arrive as `initialize` / `tools/list` / `tools/call` requests over stdio. Restart an existing server to discover a changed toolset.

## Tool list

The table lists all ten tools; `xnip_replace`, `xnip_insert`, `xnip_move`, `xnip_indent` and `xnip_doctor` require `--toolset full`. Names correspond to CLI subcommands:

| MCP Tool | CLI equivalent | Type |
|---|---|---|
| `xnip_edit` | `xnip edit` | create / exact block replace / delete / rename; optional preview/check |
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

The specialized write tools commit after validation. For nonwriting modes, `xnip_edit` and `xnip_apply` expose `dry_run` and `check`, while `xnip_recover` exposes `dry_run`:

- `--dry-run` — specialized write tools use the CLI for previews.
- `--check` — use `xnip_edit`, `xnip_apply` or CLI check mode when validation must not write.
- `--revert` — no top-level tool argument. An `xnip_apply` manifest may contain supported `revert` modifiers, with the same limitations as CLI apply.
- `--json` — MCP is already a structured JSON protocol; redundant
- `--text-stdin` / `apply --from-stdin` / op-level `@-` — MCP process stdin is occupied by the protocol

MCP **retains**:

- `was` / `was_file` (`xnip_replace`, line-based mode only) — check the original bytes; this is not a filesystem lock
- `backup` (write commands) — opt-in `.bak` copy as a safety escape hatch
- `edits` (`xnip_apply` only) — ordered exact replacements as a JSON array
- `manifest_text` (`xnip_apply` only) — inline manifests for lifecycle, line operations and patches

## Result display

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

Text starts with `written`, `unchanged`, `preview` or `check`. A single successful write returns only `written`; operation, input paths and one-match counts are not repeated. Preview/check explicitly include `written=false`. Batch receipts add a path count, multiple replacements add a match count, and retained recovery journals remain visible. Structured `files` stays complete; a rename has two affected paths. Diagnostic display paths omit Windows extended-length prefixes; structured paths retain them. Non-UTF-8 paths use lossy display text.

Preview payloads default to 12,000 bytes and follow the receipt as plain unified diff text. Truncation is explicit; pass `max_bytes: 0` for the complete preview. The notice is outside the payload budget. Diffs are not duplicated in structured metadata. Read `structuredContent` for machine-readable receipts. Successful writes and checks omit the diff.

`peek` defaults to 200 lines, `find` to 100 total hits; both default to 12,000 payload bytes. Each limit accepts `0` for unlimited output. Set both the item limit and `max_bytes` to zero when complete output is needed. Truncated displays are unsuitable as exact original bytes for an edit.

Unknown MCP arguments are rejected, including nested direct edits. Optional fields can be omitted; schemas no longer spend context advertising null alternatives.

```text
written
written; 3 matches
written; 2 paths
preview; written=false
check; written=false
```

See the [MCP tool specification](https://modelcontextprotocol.io/specification/2025-06-18/server/tools) for the standard metadata. Restart/reconnect an existing MCP server after upgrading its executable so that the client discovers the new titles and schemas.

## Error semantics

Tool call failures return a JSON-RPC `error` object (not `result.isError=true`):

| Scenario | error.code | Meaning |
|---|---|---|
| Missing / conflicting / invalid-type params | `-32602` (`invalid_params`) | User input error; fix the args and retry |
| Locator not found / `was` mismatch / pattern no match | `-32600` (`invalid_request`) | Precondition not met; may need to `xnip_peek` current state first |
| Target file IO failure / batch or whole-file rename commit failure | `-32603` (`internal_error`) | Inspect the system error and any retained recovery journal |

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

### Direct batch replacements

Use `edits` for ordered exact replacements without encoding a manifest as a string:

```json
{"name":"xnip_apply","arguments":{"edits":[
  {"file":"src/a.rs","old":"old_name","text":"new_name"},
  {"file":"src/b.rs","old":"old_name","text":"new_name"}
],"check":true}}
```

The whole plan is validated before any write. Replacements can depend on earlier replacements on the same file. `edits` conflicts with `path`, `manifest_text`, `format` and `patch_root`; an empty array is rejected. Use manifest inputs for file lifecycle, line operations and patches.

### `xnip_peek` — inspect missing context

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
| Client shows a generic tool card | Inspect `structuredContent.mode` and `written`; verify only the missing task outcome, and avoid replaying a successful edit |

## Internal architecture

`xnip mcp` and `xnip <other subcommands>` **share the same binary and the same core**. The MCP tool handlers are a "parallel frontend" to the CLI layer — they call `core::ops::*` / `apply::commit::*` directly; they do **not** spawn a subprocess or scrape CLI stdout. This means:

- A core-layer bug fix is immediately reflected in both paths
- Shared core operations and end-to-end tests cover byte transformations and file writes
- The Tokio runtime is created only for `xnip mcp`; binary size varies with target and build settings

Dependency versions and runtime features are declared in `Cargo.toml`; CI verifies Rust 1.89 compatibility.

## Working directory and trust

Relative paths are resolved from the server process working directory, which is chosen by the client. Configure that directory in the client when supported, or send absolute paths. `xnip_doctor` can check whether the current directory is writable. The server does not confine access to a repository and uses the launching user's permissions.

For batch edits, direct targets resolve against the working directory; manifest-relative payloads and file lists follow the [apply path rules](apply-format.md#paths-and-edit-ordering). A phase-2 error can leave files changed if rollback is incomplete. Inspect its rollback status and recovery journal before retrying; blindly replaying a partially committed manifest can apply an operation twice.
