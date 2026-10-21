# MCP Server (`xnip mcp`)

`xnip mcp` starts a [Model Context Protocol](https://modelcontextprotocol.io/) stdio server. It exposes five everyday tools (edit / apply / peek / find / recover) by default; `--toolset full` exposes all ten. Structured arguments carry multiline old/new text without shell escaping, and receipts identify whether a change was committed. Agent clients can select text-only receipts to reduce returned context; structured metadata remains the default.

## When to use MCP vs CLI

| Scenario | Recommended |
|---|---|
| Default editing with structured old/new content and an existing MCP connection | MCP |
| Manual terminal use, shell scripts, CI pipelines | CLI |
| Literal replacement/insertion, whole-file write or lifecycle with preview/check | `xnip_edit` or CLI `edit` |
| Line/range/batch previews, revert or NDJSON | CLI |
| Need `--text-stdin` / `apply --from-stdin` / op-level `@-` | CLI (MCP process stdin is occupied by the protocol) |

Both paths share core editing algorithms. Input fields, error handling and available preview modes differ as described below.

## Starting the server

```sh
xnip mcp --receipts text  # concise agent receipts
# Use xnip mcp for structured metadata (the default).
# The process prints nothing. It waits for the client to feed JSON-RPC frames via stdin.
# Exit: Ctrl-D (close stdin) or client disconnect.
```

For the primary editing workflow use [xnip_edit](edit.md#mcp); it accepts literal old/new blocks and supports preview/check without writes. Reuse context already available rather than issuing a mandatory peek before every edit.

Use `xnip mcp --toolset full` when a client needs specialized replace/insert/move/indent or doctor tools. xnip 0.5.0+ supports modern `server/discover` negotiation and legacy `initialize`, followed by `tools/list` / `tools/call` over stdio. This resolves the pinned Goose client's default negotiation; 0.4 releases use the older SDK. See [DSH](dsh.md), [Pi](pi.md), [Goose](goose.md) and [npm installation](../npm/README.md) for host-specific setup. Restart an existing server to discover a changed toolset.

## Tool list

The table lists all ten tools; `xnip_replace`, `xnip_insert`, `xnip_move`, `xnip_indent` and `xnip_doctor` require `--toolset full`. Names correspond to CLI subcommands:

| MCP Tool | CLI equivalent | Type |
|---|---|---|
| `xnip_edit` | `xnip edit` | exact replacement/insertion / whole-file write / create / delete / rename; optional preview/check |
| `xnip_peek` | `xnip peek` | read-only |
| `xnip_find` | `xnip find` | read-only |
| `xnip_replace` | `xnip replace` | write (atomic) |
| `xnip_insert` | `xnip insert` | write (atomic) |
| `xnip_move` | `xnip move` | write (atomic) |
| `xnip_indent` | `xnip indent` | write (atomic) |
| `xnip_apply` | `xnip apply` | write (two-phase batch) |
| `xnip_recover` | `xnip recover` | conflict-checked rollback; optional dry-run |
| `xnip_doctor` | `xnip doctor` | diagnostic |

Most input fields use CLI names with underscores, for example `--match-line` becomes `match_line`. `xnip_edit` selects file lifecycle actions with `operation` and uses `to` for rename destinations, rather than CLI `--write`/`--create`/`--delete`/`--rename-to` flags. `position: "before"` or `"after"` inserts exact bytes beside a unique nonempty `old` anchor, only with the default replace operation and without `all`. Whole-file `write` uses `old` as an optional complete-content guard and rejects final symlinks. Manifest JSON/YAML fields remain kebab-case.

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
- `files` (`xnip_apply` only) — independent paths mapped to ordered `[old,text]` pairs or complete-file writes
- `edits` (`xnip_apply` only) — ordered exact replacements as a JSON array, including explicit `all:true`
- `manifest_text` (`xnip_apply` only) — inline manifests for lifecycle, line operations and patches

## Result display

Successful tool results omit the optional `isError` field; MCP treats absence as false. All tools advertise a human-readable `title` and read/write annotations. Tool names and arguments remain compatible. Titles are display hints: Codex and other clients choose whether to use them, how to show arguments, and whether to render Markdown. xnip cannot force a native editor card, hide the client's argument JSON or customize its layout through these fields. Write tools remain marked as potentially destructive even when an individual call requests a preview.

By default, `xnip_edit`, `xnip_apply` and `xnip_recover` return a concise text summary plus `structuredContent`, described by their `outputSchema`. Start with `--receipts text` when the agent only needs the text receipt: these three tools omit both structured results and their advertised output schemas. Inputs, validation, errors, previews, retained journals and read results keep the same behavior. Use `--receipts structured` when a client consumes the fields below:

| Field | Meaning |
|---|---|
| `operation` | `create`, `write`, `replace`, `delete`, `rename`, `apply` or `recover` |
| `mode` | `write`, `preview` or `check` |
| `has_changes` | Planned file content/existence differs from the starting state |
| `written` | This successful call committed changes; always false in preview/check |
| `files` | Complete target paths; single edit includes its target even when unchanged, rename includes both endpoints, batches/recovery include changed paths |
| `destination` | Rename destination, when applicable |
| `replacements` | Matched literal blocks for exact replacement, including identical replacements |
| `journal` | Retained recovery snapshot directory, when applicable |

Text starts with `written`, `unchanged`, `preview` or `check`. An ordinary successful write returns only `written`; operation, paths and counts remain in structured metadata. Preview/check explicitly include `written=false`, no-ops return `unchanged`, and retained recovery journals remain visible. Structured `files` stays complete; a rename has two affected paths. Diagnostic display paths omit Windows extended-length prefixes; structured paths retain them. Non-UTF-8 paths use lossy display text.

Preview payloads default to 12,000 bytes and follow the receipt as plain unified diff text. Truncation is explicit; pass `max_bytes: 0` for the complete preview. The notice is outside the payload budget. Diffs are not duplicated in structured metadata. Read `structuredContent` for machine-readable receipts. Successful writes and checks omit the diff.

`peek` defaults to 200 lines, `find` to 100 total hits; both default to 12,000 payload bytes. Each limit accepts `0` for unlimited output. Set both the item limit and `max_bytes` to zero when complete output is needed. Truncated displays are unsuitable as exact original bytes for an edit.

Unknown MCP arguments are rejected, including nested direct edits. Optional fields can be omitted; omitted input flags are false. Schemas omit redundant null alternatives and false-default annotations while retaining validation constraints.

```text
written
unchanged
preview; written=false
check; written=false
```

See the [MCP tool specification](https://modelcontextprotocol.io/specification/2025-06-18/server/tools) for the standard metadata. Restart/reconnect an existing MCP server after upgrading its executable so that the client discovers the new titles and schemas.

## Error semantics

Tool call failures return a JSON-RPC `error` object (not `result.isError=true`). Parameter errors retain their cause chain, including regex and manifest parser diagnostics:

| Scenario | error.code | Meaning |
|---|---|---|
| Missing / conflicting / invalid-type params | `-32602` (`invalid_params`) | User input error; fix the args and retry |
| Locator not found / `was` mismatch / pattern no match / batch phase-1 validation | `-32600` (`invalid_request`) | Precondition not met; may need to `xnip_peek` current state first |
| Standalone tool IO / batch file-list IO / batch, whole-file write or rename commit failure | `-32603` (`internal_error`) | Inspect the system error and any retained recovery journal |

## Client configuration

These examples keep the default structured receipts. With xnip 0.4.0 or newer, use `args: ["mcp", "--receipts", "text"]` when the client only needs text receipts. Restart existing connections after upgrading or changing server options.

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

### Cline

Open the MCP Servers panel, choose Configure, then Configure MCP Servers. Add xnip under `mcpServers` using the same `command`/`args` JSON shape above. See [Cline's configuration reference](https://docs.cline.bot/mcp/mcp-overview).

### Continue

Add an entry to the `mcpServers` list in the active `config.yaml`:

```yaml
mcpServers:
  - name: xnip
    command: xnip
    args: [mcp]
```

Use Continue's agent mode for MCP tools. See the [Continue MCP setup guide](https://docs.continue.dev/customize/deep-dives/mcp).

### Zed

Add a local server in Zed's MCP settings, or edit the settings file opened by `zed: open settings file`. The [Zed MCP reference](https://zed.dev/docs/ai/mcp) uses sibling `command` and `args` fields:

```json
{
  "context_servers": {
    "xnip": { "command": "xnip", "args": ["mcp"] }
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

### Whole-file write and exact insertion

```json
{"name":"xnip_edit","arguments":{"file":"config.txt","operation":"write","old":"PORT=8080","text":"PORT=3000"}}
```

`write` creates or replaces the complete regular file. Its optional `old` guard compares all bytes of an existing file; `old: ""` does not match a missing file. Use `operation:"create"` for no-clobber creation. Preview/check validates without creating files or parent directories.

```json
{"name":"xnip_edit","arguments":{"file":"source.txt","old":"anchor","position":"after","text":"\r\nnext"}}
```

The second call keeps `anchor` and inserts exactly the supplied CRLF and text. No newline is added implicitly. A missing, empty or ambiguous anchor fails, and `position` cannot be used with `all` or another operation.

### Compact file groups

```json
{"name":"xnip_apply","arguments":{"files":{
  "src/a.rs":[["old_name","new_name"],["new_name()","new_name(42)"]],
  "generated.txt":"complete content\n",
  "config.txt":{"text":"PORT=3000","old":"PORT=8080"}
},"check":true}}
```

Pairs are unique exact replacements, ordered within their file against virtual content. Strings explicitly write the entire file; `{text,old?}` adds an optional complete-file guard. File entries are independent and cannot encode cross-file dependencies by map order. Normalized alias targets and empty maps/pair groups are rejected. Clients must send unique JSON keys: the protocol decoder collapses repeated raw keys before tool validation. To check raw JSON for duplicates, use `manifest_text` with `format:"compact"`. Every group validates before the shared transaction commits per-file final states.

Choose exactly one input: `files`, `edits`, `path` or `manifest_text`. `files` cannot be combined with `format` or `patch_root` either; it already identifies the representation. Read [compact groups](apply-format.md#compact-file-groups) for the CLI JSON equivalent and [ordered plans](apply-format.md#file-lifecycle-operations-and-ordered-plans) for dependent lifecycle operations.

### Known line ranges with the default toolset

The existing `xnip_apply` accepts native line operations directly, so a known range does not require switching to the full toolset:

```json
{"name":"xnip_apply","arguments":{"manifest_text":"replace src/example.txt 2-5 \"    return sum(item.price for item in items)\""}}
```

Native format and default modifiers can be omitted. Use this representation when the range is known and its line-based preconditions/newline behavior fit the task; retain an old-content guard when required. For complete known files, [compare line manifests, unique exact pairs and whole writes](edit.md#choose-an-edit) before sending a longer payload.

### Direct batch replacements

Use `edits` for ordered exact replacements without encoding a manifest as a string:

```json
{"name":"xnip_apply","arguments":{"edits":[
  {"file":"src/a.rs","old":"old_name","text":"new_name"},
  {"file":"src/b.rs","old":"old_name","text":"new_name"}
],"check":true}}
```

The whole plan is validated before any write. Replacements can depend on earlier replacements on the same file. `edits` conflicts with `files`, `path`, `manifest_text`, `format` and `patch_root`; an empty array is rejected. Use `files` for independent whole-file writes and manifest inputs for dependent lifecycle, line operations and patches.

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
