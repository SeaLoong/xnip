[中文](./README.zh.md) | English

# xnip

> Precise text editing CLI for LLM agents.

xnip compresses the LLM agent "read section → generate new section → replace" loop into a single command: **file path + position + new content**.

- Standalone Rust binary for supported macOS, Linux and Windows targets
- 9 commands: `peek` / `find` / `replace` / `insert` / `move` / `indent` / `apply` / `mcp` / `doctor`
- `apply` accepts three formats (native compact / JSON / YAML) for validated batch edits with per-file atomic writes
- Atomic writes, `--dry-run` preview, symmetric `--revert`
- **`xnip mcp`**: built-in [Model Context Protocol](https://modelcontextprotocol.io/) stdio server for Claude Desktop / Cursor / Cline / Continue / Zed

## Install

Requires Rust **1.89+**. Source installation is available now.

### From source

```sh
git clone https://github.com/SeaLoong/xnip
cd xnip
cargo install --path . --locked
```

### Prebuilt binaries (after the first release)

The repository currently has no published GitHub release. These commands become available after a version tag is published with the release workflow. The crates.io command additionally requires publishing the crate.

```sh
# macOS / Linux
curl -fsSL https://github.com/SeaLoong/xnip/releases/latest/download/install.sh | sh

# Windows
iwr -useb https://github.com/SeaLoong/xnip/releases/latest/download/install.ps1 | iex

# Any platform with Rust
cargo install xnip --locked
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
printf '%s\n' 'replace --files-from list.txt s/OLD_NAME/NEW_NAME/g' | xnip apply --from-stdin

# Validated batch edit
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
- **Symmetric revert** — `--revert` supports constrained inverse operations; see the limitations below

Full design spec: [`PLAN.md`](./PLAN.md) · Version history: [`CHANGELOG.md`](./CHANGELOG.md)

## License

MIT — see [`LICENSE`](./LICENSE).

## Safety and usage limits

- `apply` validates all operations before writing, but commits files one at a time. A phase-2 failure returns exit 4 with the committed paths. There is **no automatic rollback**; inspect those paths and restore from Git or `.bak` files as needed.
- Backups append `.bak`: `main.rs` becomes `main.rs.bak`. An existing backup is replaced. This changes the older extension-replacement behavior (`main.bak`).
- Existing symlinks are followed and file permissions preserved. Atomic replacement does not preserve hard-link identity, ownership, ACLs or extended attributes. Avoid concurrent writers: `--was` checks the bytes read, but does not lock the file.
- `--revert` is not an undo log. Pattern revert accepts only a nonempty literal pattern and a nonempty replacement without capture expansion; pre-existing replacement text can make even literal reversal ambiguous. Range replacement revert requires the replacement to occupy the same range. Indentation conversion and edits at an unterminated EOF may not round-trip exactly. Use Git/backups for exact recovery.
- Line operations split on LF; existing CR bytes are retained, but inserted text is not automatically converted to CRLF. Supply CRLF payloads when required.
- CLI `--text` receives literal shell arguments. Use `--text-file` or stdin for portable multiline content. `apply --json --check` emits only NDJSON; `--json` and `--dry-run` are mutually exclusive.
- MCP runs with the launching user's filesystem permissions and working directory; it is intended for trusted local clients.

## Development and release

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo +1.89.0 check --workspace --all-targets --locked
cargo deny check
cargo bench --locked
cargo package --locked
cargo run --locked -p xtask -- sync-integrations
```

Use `python3 tests/install_sh.py` on Linux/macOS, or build with `cargo build --locked` and run `./tests/install_ps1.ps1` in PowerShell. Installer tests use local archives and mock HTTP, including checksum-failure recovery.

Before publishing, update the package version and changelog, run CI, and push a matching `vX.Y.Z` tag. Release CI checks the tag against Cargo metadata, builds six targets, and publishes all archives, SHA256 sidecars, installers and integration packages only after all builds succeed. Manual workflow runs build artifacts without publishing. Publishing to crates.io is a separate maintainer action (`cargo publish --locked`). See [architecture and remaining limits](docs/architecture.md).
