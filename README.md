[中文](./README.zh.md) | English

# xnip

> Precise text editing CLI for LLM agents.

xnip compresses the LLM agent "read section → generate new section → replace" loop into a single command: **file path + position + new content**.

- Standalone Rust binary for supported macOS, Linux and Windows targets
- 10 commands on the development branch: `edit` / `peek` / `find` / `replace` / `insert` / `move` / `indent` / `apply` / `mcp` / `doctor`
- `apply` accepts three formats (native compact / JSON / YAML) for validated batch edits with per-file atomic writes
- Atomic writes, `--dry-run` preview, constrained `--revert`
- **`xnip mcp`**: built-in [Model Context Protocol](https://modelcontextprotocol.io/) stdio server for Claude Desktop / Cursor / Cline / Continue / Zed

## Install

Prebuilt binaries do not require Rust. Building from source requires **Rust 1.89+**.

### From source

```sh
git clone https://github.com/SeaLoong/xnip
cd xnip
cargo install --path . --locked
```

### Prebuilt binaries

Download from [GitHub Releases](https://github.com/SeaLoong/xnip/releases), or use the installers below. Both verify the archive SHA256 before installation.

```sh
# macOS / Linux
curl -fsSL https://github.com/SeaLoong/xnip/releases/latest/download/install.sh | sh

# Windows
iwr -useb https://github.com/SeaLoong/xnip/releases/latest/download/install.ps1 | iex

# Any platform with Rust, directly from the release tag
cargo install --git https://github.com/SeaLoong/xnip --tag v0.1.1 --locked xnip
```

See [installation and troubleshooting](docs/installation.md) for platform selection, version pinning, PATH setup, checksums and removal. The GitHub release is the distribution channel; crates.io installation is not currently advertised.

## Default editing entry point

The development branch adds `xnip edit` / `xnip_edit`: create missing files, replace unique literal blocks, delete regular files, and preview/check each operation. Published v0.1.1 does not contain this command; build this checkout for the new capability. See [the edit reference](docs/edit.md).

```sh
xnip edit config.txt --old 'PORT=8080' --text 'PORT=3000'
xnip edit src/new.txt --create --text-file content.txt
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

Start with the [runnable tutorial](docs/cli.md), then consult the [agent guide](docs/SKILL.md), [examples](docs/examples.md), [manifest reference](docs/apply-format.md) and [exit codes](docs/error-codes.md).

## Integrating with LLM agents

xnip is intended to serve as the normal text editing interface. Its skill chooses the appropriate operation and reuses known context, rather than requiring a doctor/peek/find sequence before every write. Repository discovery and semantic analysis remain separate from applying text edits.

### Install the editing skill

Use the generated [Codex/GitHub skill](.github/skills/xnip-editing/SKILL.md) or [Claude Code skill](integrations/claude-code/SKILL.md), including its `references` directory. The entrypoint covers the default workflow; detailed manifests, examples and errors are loaded only when needed.

| Client | Integration | Placement |
|---|---|---|
| Codex | `.github/skills/xnip-editing/` | User skill directory (`~/.codex/skills/xnip-editing/`) |
| Claude Code | `integrations/claude-code/` | `.claude/skills/xnip/` |
| Cursor | [rule](integrations/cursor/xnip.mdc) | Project rule, with automatic always-on injection disabled |
| Copilot | [short instruction](integrations/copilot/xnip.md) | Project instructions |
| Aider | [convention](integrations/aider/CONVENTIONS.md) | Project conventions |
| Other agents | [generic skill](integrations/generic/SKILL.md) | Load for editing tasks; include its references |

The [AGENTS.md snippet](integrations/agents-md/AGENTS.md) is a compact project-level alternative. Avoid duplicating the full manual in every prompt.

### Choose CLI or MCP

CLI uses an existing shell capability. MCP is useful when structured old/new text avoids shell quoting; its registration is an explicit setup choice. Both can implement the same default editing workflow. The development server exposes nine tools; `xnip_edit` supports preview/check, while other MCP write tools commit immediately. The published v0.1.1 server exposes the original eight tools.

Client configurations are maintained in the [MCP reference](docs/mcp.md). Neither installing a skill nor reading its instructions requires automatically registering another MCP server.

## Design

- **Project-agnostic** — no project config, no language detection, no assumptions about structure
- **Cross-platform** — the same commands, with a native binary for each OS and architecture
- **Machine-friendly** — stderr for humans, stdout for machines; optional `--json` NDJSON
- **Atomic writes** — tmpfile in same dir → validate → atomic rename; `.bak` is opt-in (`--backup`)
- **Symmetric revert** — `--revert` supports constrained inverse operations; see the limitations below

Historical design plan: [`PLAN.md`](./PLAN.md) · Version history: [`CHANGELOG.md`](./CHANGELOG.md)

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

Contributor workflow: [CONTRIBUTING.md](CONTRIBUTING.md). Maintainer release procedure: [docs/releasing.md](docs/releasing.md).
