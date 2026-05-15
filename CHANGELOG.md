# Changelog

All notable changes to xnip will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **MCP server (`xnip mcp`)**: in-process [Model Context Protocol](https://modelcontextprotocol.io/)
  stdio server that exposes 8 tools (`xnip_peek` / `xnip_find` / `xnip_replace` /
  `xnip_insert` / `xnip_move` / `xnip_indent` / `xnip_apply` / `xnip_doctor`) to MCP
  clients (Claude Desktop, Cursor, Cline, Continue, Zed, ...). Tool input schemas
  derive from `LocatorArgs`/`ContentArgs` field names so they map 1:1 to cli flags.
  Tools delegate to the same `core::ops::*` and `apply::commit::*` functions used by
  the cli, guaranteeing behavioural parity. See `docs/mcp.md` and PLAN §6.10 / §8.5.
  - Built on `rmcp 1.7` (official Rust SDK) + `tokio 1` (single-threaded current-thread
    runtime) + `schemars 1.0`.
  - 5 e2e integration tests in `tests/mcp.rs` (initialize handshake, `tools/list`
    contains 8 tools, `xnip_peek` correctness, `xnip_replace` `was`-failure leaves
    file untouched, `xnip_replace` happy path).
  - All cli convenience flags **not exposed** to MCP (`--dry-run`, `--check`,
    `--revert`, `--json`, `--text-stdin`, `apply --from-stdin`, op-level `@-`)—see
    `docs/mcp.md` for rationale.
- **CLI**: 9 commands: 7 editing commands `peek`, `find`, `replace`, `insert`,
  `move`, `indent`, `apply` plus `mcp` (stdio MCP server) and `doctor`
  (self-diagnostic). Editing commands feature a full locator system (5 kinds) and
  content sources (4 kinds).
- **`apply`** with three equivalent input formats:
  - native compact (one-op-per-line, supports `s/PAT/REPL/g`, `=/regex/`,
    `~/A/..~/B/`, literal `"A".."B"[i]` between, `+N`/`-N`, `t2s:N`, `s2t:N`,
    `was=...`, `revert`)
  - JSON (top-level array, kebab-case fields)
  - YAML (same schema as JSON, parsed via `serde_yaml::Value` → `serde_json::Value`)
  - Auto-detect by extension; fallback chain JSON → YAML → native.
  - Two-phase commit: phase 1 writes tmpfiles, phase 2 atomic-renames.
  - `--check`, `--dry-run` (unified diff), `--from-stdin`, `--backup`, `--json` (NDJSON).
  - **`--parallel <N>`**: run phase 1 on N worker threads (rayon); phase 2 remains serial.
  - **Op-level `@-`** content (consumes process stdin once per manifest, at most one
    `@-` per manifest) and **`--stdin-file <path>`** to provide that payload from a file
    (required when `--from-stdin` is also used).
- **`--revert`** across all write commands with round-trip tested semantics:
  - `replace --pattern --repl --revert` (swap pattern/repl, regex-escaped)
  - `replace --lines a-b --text X --was Y --revert` (swap text/was with pre-condition check)
  - `insert --lines A --text X --revert` (delete the inserted block with pre-condition check)
  - `move --from-lines S-E --to T --position P --revert`
    (via `core::ops::move_op::reverse_params` — 4-case round-trip verified)
  - `indent --add/--remove/--tabs-to-spaces/--spaces-to-tabs --revert` (op inversion)
- **Atomic write contract** in `core::atomic::atomic_write`:
  same-dir tmpfile → `sync_all` → `tmp.persist(target)`. `.bak` is opt-in.
- **Global CLI flags** (`clap global = true`, available before or after the subcommand):
  - `--quiet`: suppress non-error stderr notices (`note!` macro). All write commands
    (`replace`, `insert`, `move`, `indent`, `apply`) emit a `wrote ...` / `committed ...`
    summary on success, suppressed when `--quiet` is set.
  - `--no-color`: disable ANSI (also honors `NO_COLOR` env var per https://no-color.org).
    Currently affects `apply --dry-run` unified-diff output (git-diff style colors,
    auto-disabled when stdout is not a TTY).
  - `--trace`: emit `[xnip trace]` diagnostic lines on stderr (`trace!` macro)
- **`xnip doctor`** self-diagnostic (version, OS/arch/family, target triple,
  cwd writability, TTY flags).
- **Colored unified diff** for `apply --dry-run` via `core::diff::colorize_unified_diff`:
  bold `--- / +++` headers, cyan `@@` hunk headers, red deletions, green additions, dim
  `\ No newline ...` markers. Activates only when stdout is a TTY and `--no-color` /
  `NO_COLOR` is not set. Hand-rolled ANSI sequences (no extra crate dependency).
- **NDJSON event stream** for `apply --json`: `start`, `done`, `error` events.
- **Documentation suite** under `docs/`:
  - `SKILL.md` — primary agent-facing skill (used by all integrations)
  - `apply-format.md` — full manifest reference
  - `examples.md` — 13 recipe scenarios
  - `error-codes.md` — exit-code semantics
  - `architecture.md` — contributor-facing data flow
- **Integration templates** under `integrations/`:
  - `claude-code/SKILL.md` (with YAML frontmatter)
  - `cursor/xnip.mdc`
  - `aider/CONVENTIONS.md`
  - `copilot/xnip.md`
  - `agents-md/AGENTS.md`
  - `generic/SKILL.md` (auto-synced via `cargo xtask sync-integrations`)
  - Each ships a `package.sh` that produces a `.tar.gz` for releases.
- **Installers**: `install.sh` (POSIX) and `install.ps1` (Windows).
- **CI** workflows: `ci.yml` (fmt + clippy + test on Linux/macOS/Windows + cargo-deny)
  and `release.yml` (cross-compile 6 targets + GH release upload + integrations).
- **Tests** (`cargo test` totals: **312 passed, 0 failed, 1 ignored** across 15 suites,
  under `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`):
  - 211 unit tests across `core/`, `apply/`, `cli/common`, `output/*`, `doctor`
    (includes 4 colorize-diff tests in `core::diff`)
  - 12 `apply` integration tests including a three-format equivalence assertion
  - 14 `m6` tests: `@-` stdin (2), `--stdin-file`, `--parallel` (3),
    full-op `--revert` round-trip (4), native literal `between` (2),
    `move --from-match-line --revert` rejection, lazy stdin (no `@-`) non-consumption
  - 13 `global_flags` tests: `--quiet` (5) / `--trace` (2) / `--no-color` (4) /
    cross-position (1) / write-command `note!` round (5)
  - 5 `m4` tests (doctor + `apply --json`)
  - 5 `tests/mcp.rs` end-to-end MCP tests (handshake, `tools/list` reports 8 tools,
    `xnip_peek` correctness, `xnip_replace` happy path, `was`-mismatch keeps file untouched)
  - 8 `peek`, 8 `find`, 10 `replace`, 7 `insert`, 7 `move_op`, 8 `indent`
    integration tests
  - 4 smoke tests on the binary
  - 1 ignored doc-test placeholder (no production doctest yet).

### Changed

- **MSRV** bumped from **1.85** to **1.95** (required by the `rmcp 1.7` +
  `schemars 1.0` + transitive deps stack in the resolved `Cargo.lock`; individual
  crates' `package.rust-version` aggregate up to 1.95). `rust-toolchain.toml` channel
  changed from `"1.85"` to `"stable"` to follow the latest stable;
  `dtolnay/rust-toolchain@1.85` in `.github/workflows/{ci,release}.yml` bumped to
  `@1.95`. Edition stays at 2024.

### Notes & design boundaries (v0.1.0)

- `--revert` for range-locator `replace`, `insert`, `move`, `indent` all implemented
  and round-trip tested.
  - For `replace`: range-locator revert requires `--lines a-b` + `--was` (the original
    bytes to restore). Anchor-based locators (`match-line`, `between`, `between-re`)
    cannot be safely inverted after forward execution because the anchor may no longer
    exist; those combinations return an explicit error rather than silently succeeding.
  - For `indent`: `Add(N)`↔`Remove(N)` and `TabsToSpaces(N)`↔`SpacesToTabs(N)` are
    logically inverse but `Remove`/`SpacesToTabs` forward is *not strictly* invertible
    (information may be lost), so revert may differ from the original bytes.
- `apply @-`: a manifest may reference `@-` at most once (stdin is a linear byte stream
  and there's no unambiguous splitter). Multiple `@-` tokens result in exit code 3.
  When no op uses `@-`, process stdin is **not** consumed (lazy), so an unrelated
  upstream pipe to `xnip apply manifest.txt` will not be silently swallowed.
- `move --revert` accepts only `--from-lines`; `--from-match-line` is rejected because
  the anchor refers to a different block in the post-forward file (the parameters that
  describe the original source can no longer be recovered from a regex match).
- `apply --parallel N`: phase-1 runs in parallel; phase-2 (atomic rename) stays serial
  to preserve commit-order-dependent rollback semantics.
- Colored output: `--no-color` and `NO_COLOR` (https://no-color.org) are honored.
  v0.1.0 only colors `apply --dry-run` unified-diff output; other commands emit no
  ANSI today. Color is auto-disabled when stdout is not a TTY (so piping to `less` or
  files always yields plain bytes).

[Unreleased]: https://github.com/SeaLoong/xnip/compare/HEAD...HEAD
