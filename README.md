[中文](./README.zh.md) | English

# xnip

> Precise text editing CLI for LLM agents — ≥ 70% token reduction.

xnip compresses the LLM agent "read section → generate new section → replace" loop into a single command: **file path + position + new content**.

- Single static Rust binary, works on all platforms
- 9 commands: `peek` / `find` / `replace` / `insert` / `move` / `indent` / `apply` / `mcp` / `doctor`
- `apply` accepts three formats (native compact / JSON / YAML) for atomic batch edits
- Atomic writes, `--dry-run` preview, symmetric `--revert`
- **`xnip mcp`**: built-in [Model Context Protocol](https://modelcontextprotocol.io/) stdio server for Claude Desktop / Cursor / Cline / Continue / Zed

## Install

### From source

```sh
git clone https://github.com/SeaLoong/xnip
cd xnip
cargo install --path .
```

### Prebuilt binaries

```sh
# macOS / Linux
curl -fsSL https://github.com/SeaLoong/xnip/releases/latest/download/install.sh | sh

# Windows
iwr -useb https://github.com/SeaLoong/xnip/releases/latest/download/install.ps1 | iex

# Any platform with Rust
cargo install xnip
```

## Quick start

```sh
# Show lines 30-45
xnip peek src/Foo.vue --lines 30-45

# Locate matches
xnip find --pattern '^const PORT' src/Foo.vue

# Replace a line (preview first)
xnip replace src/Foo.vue --lines 30 --text "const X = 1;" --dry-run

# Cross-file constant rename
xnip replace --files-from list.txt --pattern OLD_NAME --repl NEW_NAME

# Atomic batch edit
xnip apply edits.txt
```

See [`docs/SKILL.md`](./docs/SKILL.md) and [`docs/examples.md`](./docs/examples.md) for the full reference.

## Integrating with LLM agents

xnip offers two integration paths. Both can be active simultaneously.

### Path A — MCP (structured tool protocol)

Best for agents with native MCP support. The agent calls 8 structured tools directly — no shell quoting, no exit-code handling.

First, verify xnip is on your PATH: `xnip doctor`

Then add it to your agent's config:

**Claude Desktop**

Edit `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows), then restart:

```json
{
  "mcpServers": {
    "xnip": { "command": "xnip", "args": ["mcp"] }
  }
}
```

**Claude Code**

Create `.mcp.json` in your project root:

```json
{
  "mcpServers": {
    "xnip": { "command": "xnip", "args": ["mcp"] }
  }
}
```

**Cursor**

Create `.cursor/mcp.json` in your project (or `~/.cursor/mcp.json` globally). A ready-to-copy file is at [`integrations/cursor/mcp.json`](./integrations/cursor/mcp.json):

```json
{
  "mcpServers": {
    "xnip": { "command": "xnip", "args": ["mcp"] }
  }
}
```

**Cline / Continue**

UI → Settings → MCP → Add server → command: `xnip`, args: `["mcp"]`

**Zed**

Add to `~/.config/zed/settings.json`:

```json
{
  "context_servers": {
    "xnip": { "command": { "path": "xnip", "args": ["mcp"] } }
  }
}
```

After restarting your client, 8 tools appear in the agent panel: `xnip_peek`, `xnip_find`, `xnip_replace`, `xnip_insert`, `xnip_move`, `xnip_indent`, `xnip_apply`, `xnip_doctor`. Full MCP reference: [`docs/mcp.md`](./docs/mcp.md).

### Path B — Skill / prompt injection

Best for agents driven by system prompts or instructions files. Copy the snippet for your agent and drop it in the right place — the agent then autonomously invokes `xnip <subcommand>` for file edits.

| Agent | Source file | Where to put it |
|-------|-------------|-----------------|
| **GitHub Copilot** | [`integrations/copilot/xnip.md`](./integrations/copilot/xnip.md) | Append to `.github/copilot-instructions.md` in your project |
| **Aider** | [`integrations/aider/CONVENTIONS.md`](./integrations/aider/CONVENTIONS.md) | Merge into your project's `CONVENTIONS.md` |
| **Claude Code** | [`integrations/claude-code/SKILL.md`](./integrations/claude-code/SKILL.md) | Copy to `.claude/skills/xnip.md` in your project |
| **AGENTS.md** (Codex, etc.) | [`integrations/agents-md/AGENTS.md`](./integrations/agents-md/AGENTS.md) | Append to your project's `AGENTS.md` |
| **Any other agent** | [`integrations/generic/SKILL.md`](./integrations/generic/SKILL.md) | Paste into the agent's system prompt or custom instructions |

## Design

- **Project-agnostic** — no project config, no language detection, no assumptions about structure
- **Cross-platform** — same binary on macOS / Linux / Windows
- **Machine-friendly** — stderr for humans, stdout for machines; optional `--json` NDJSON
- **Atomic writes** — tmpfile in same dir → validate → atomic rename; `.bak` is opt-in (`--backup`)
- **Symmetric revert** — `--revert` inverts the same args; non-invertible ops error out

Full design spec: [`PLAN.md`](./PLAN.md) · Version history: [`CHANGELOG.md`](./CHANGELOG.md)

## License

MIT — see [`LICENSE`](./LICENSE).
