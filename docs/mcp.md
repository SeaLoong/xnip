# MCP Server (`xnip mcp`)

`xnip mcp` starts a [Model Context Protocol](https://modelcontextprotocol.io/) **stdio server** that exposes xnip's 8 capabilities (peek / find / replace / insert / move / indent / apply / doctor) as structured tools to LLM agents — eliminating the quote-escaping, exit-code parsing, and stdout/stderr confusion that comes with shell invocations.

## When to use MCP vs CLI

| Scenario | Recommended |
|---|---|
| Agent / LLM integration (Claude Desktop, Cursor, Cline, Continue, Zed, …) | **MCP** |
| Manual terminal use, shell scripts, CI pipelines | CLI |
| Need `--dry-run` / `--check` / `--revert` / `--json` | CLI (not exposed via MCP) |
| Need `--text-stdin` / `apply --from-stdin` / op-level `@-` | CLI (MCP process stdin is occupied by the protocol) |

Both paths **share the same core implementation** and behave identically.

## Starting the server

```sh
xnip mcp
# The process prints nothing. It waits for the client to feed JSON-RPC frames via stdin.
# Exit: Ctrl-D (close stdin) or client disconnect.
```

No sub-arguments. All "calls" arrive as `initialize` / `tools/list` / `tools/call` requests from the MCP client over stdio.

## Tool list

8 tools, named 1:1 after CLI subcommands:

| MCP Tool | CLI equivalent | Type |
|---|---|---|
| `xnip_peek` | `xnip peek` | read-only |
| `xnip_find` | `xnip find` | read-only |
| `xnip_replace` | `xnip replace` | write (atomic) |
| `xnip_insert` | `xnip insert` | write (atomic) |
| `xnip_move` | `xnip move` | write (atomic) |
| `xnip_indent` | `xnip indent` | write (atomic) |
| `xnip_apply` | `xnip apply` | write (two-phase batch) |
| `xnip_doctor` | `xnip doctor` | diagnostic |

Input schema field names match CLI flags 1:1 (strip `--`, convert hyphens to underscores). For example, `--match-line` becomes `match_line`.

### Differences from CLI

MCP does **not** expose these flags:

- `--dry-run` — MCP returns the result text directly; structured reply is easier for LLMs to consume than a unified diff
- `--check` — MCP uses `Err(McpError)` to express validation failure, which is more direct than an exit code
- `--revert` — CLI convenience feature; LLMs can construct the inverse edit directly at negligible cost
- `--json` — MCP is already a structured JSON protocol; redundant
- `--text-stdin` / `apply --from-stdin` / op-level `@-` — MCP process stdin is occupied by the protocol

MCP **retains**:

- `was` / `was_file` (write commands) — files may be modified externally during a long session; concurrency guard is essential
- `backup` (write commands) — opt-in `.bak` copy as a safety escape hatch
- `manifest_text` (`xnip_apply` only) — inline manifest text, ideal for LLMs generating short manifests without writing a file first

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

You should see `serverInfo: {name: "xnip", version: "..."}` followed by a list of 8 tools.

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

### `xnip_replace` — precise edit with concurrency guard

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

### `xnip_apply` — atomic batch commit

```json
{
  "name": "xnip_apply",
  "arguments": {
    "manifest_text": "replace src/a.rs lines 30-32 text=\"...\"\ninsert src/b.rs lines 1 position=before text=\"// header\\n\"\n",
    "format": "native",
    "backup": true
  }
}
```

Phase 1: all ops are validated and staged; any failure → no file is written, error returned.
Phase 2: atomic renames committed sequentially; partial failure rolls back already-committed files (via `.bak` when `backup=true`).

## Troubleshooting

| Symptom | Fix |
|---|---|
| Client reports "Failed to connect" | Run `which xnip`; the `command` in client config must be an absolute path or be on the client's `PATH` |
| `tools/list` returns 0 tools | Upgrade to v0.1.0+; older versions have no `mcp` subcommand |
| `xnip_apply` reports "manifest contains op content `@-`" | MCP does not read process stdin; use inline `text` or `text_file` instead |
| Edit written but LLM sees no change | Have the LLM call `xnip_peek` to re-read; do not let the LLM treat tool reply text as "current file state" |

## Internal architecture

`xnip mcp` and `xnip <other subcommands>` **share the same binary and the same core**. The MCP tool handlers are a "parallel frontend" to the CLI layer — they call `core::ops::*` / `apply::commit::*` directly; they do **not** spawn a subprocess or scrape CLI stdout. This means:

- A core-layer bug fix is immediately reflected in both paths
- Behavioral consistency is guaranteed by shared code (same byte passthrough, same atomic write, same `was` check)
- Binary size is ~9 MB larger than CLI-only (cost of rmcp + tokio), but the tokio runtime is only created when `xnip mcp` actually starts — zero runtime overhead for all other CLI paths

Dependencies: `rmcp 1.7` (official Rust SDK) + `tokio 1` (`rt, macros, io-std`) + `schemars 1.0` (auto-generates tool input JsonSchema). MSRV 1.95 (constrained by rmcp / schemars transitive dependencies and Cargo.lock resolution).
