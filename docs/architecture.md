# xnip — Architecture

> A 30-second overview for contributors and curious agents.

## Design goals

- **Project-agnostic**: no project config, no language detection, no implicit assumptions.
- **Cross-platform**: shared command behavior, with native binaries per OS/architecture.
- **Machine-friendly**: stderr for humans, stdout for machines; optional `--json` NDJSON.
- **Atomic writes**: tmpfile in same dir → fsync → atomic rename. `.bak` is opt-in via `--backup`.
- **Symmetric revert**: `--revert` supports limited inverse operations; use Git or backups for exact recovery.

## Layout

```
xnip/
├── Cargo.toml         # workspace = [".", "xtask"]
├── rust-toolchain.toml  # stable, edition 2024; MSRV 1.89
├── src/
│   ├── lib.rs         # crate root; re-exports public surface
│   ├── main.rs        # binary entry → cli::run
│   ├── cli/           # clap derive + per-subcommand glue
│   │   ├── mod.rs     # Cli/Command + dispatch
│   │   ├── common.rs  # shared LocatorArgs / ContentArgs
│   │   └── {peek,find,replace,insert,move_op,indent,apply}.rs
│   ├── core/          # byte operations plus content/persistence IO
│   │   ├── location.rs   # 5 locators + resolve()
│   │   ├── content.rs    # 4 content sources + load
│   │   ├── atomic.rs     # tempfile + atomic rename + optional .bak
│   │   ├── diff.rs       # similar-based unified diff
│   │   ├── revert.rs     # symmetric inversion helpers
│   │   └── ops/          # one file per write-op + peek/find
│   ├── apply/         # batch edit pipeline
│   │   ├── mod.rs        # Op enum + Target / OpContent / IndentKind
│   │   ├── parse_native.rs
│   │   ├── parse_json.rs
│   │   ├── parse_yaml.rs   # delegates to JSON via serde_json::Value
│   │   ├── detect.rs       # auto-format detection
│   │   └── commit.rs       # two-phase commit executor
│   ├── output/
│   │   ├── exit.rs    # exit-code constants
│   │   ├── human.rs   # placeholder; per-cmd glue currently inlines plain output
│   │   └── json.rs    # NDJSON Event enum + emit()
│   └── doctor.rs      # `xnip doctor` self-diagnostic
├── tests/             # assert_cmd integration tests, one file per command
├── benches/           # criterion benchmarks
├── docs/              # SKILL / apply-format / examples / error-codes / architecture
├── integrations/      # per-tool template directories
└── xtask/             # `cargo run -p xtask -- sync-integrations`
```

## Data flow for write commands

```
clap derive  → cli::<cmd>::Args
            → LocatorArgs::into_locator → core::location::Locator
            → ContentArgs::into_content → core::content::Content
            → core::content::load_path(target)
            → core::ops::<cmd>::<fn>(bytes, locator, payload, ...)
            → core::atomic::atomic_write(target, new_bytes, backup)
```

## Data flow for `apply`

```
manifest text
  → apply::detect::parse_auto / parse_with → Vec<Op>
  → apply::commit::execute(ops, opts)
       ├─ expand_files_from
       ├─ group by file
       ├─ sort each group by start-line desc
       ├─ phase 1: read → fold ops → tmpfile per file
       └─ phase 2: atomic-rename each tmpfile (+ optional .bak)
  → ExitCode (0 / 3 / 4)
```

## Locator semantics

`core::location::Locator` is the single canonical type used by both CLI args
and apply ops:

- `Lines { start, end }` — 1-based inclusive line range
- `MatchLine { regex, occurrence }` — Nth line matching regex
- `Between { start, end, ..., inclusive }` — literal anchors (line-level `contains`)
- `BetweenRe { start, end, ..., inclusive }` — regex anchors
- `Pattern { regex, count }` — byte-level matches across lines (replace only)

`resolve(loc, content) → Resolved { start_line, end_line }` is the entry point
for the first four; `Pattern` is consumed directly by `core::ops::replace::replace_pattern`.

## Three formats, one schema

`parse_yaml` is implemented as `serde_saphyr → serde_json::Value → parse_json::parse`.
This shares field validation between JSON and YAML. The native parser produces the same
`Vec<Op>` shape; the integration test `apply_three_formats_produce_equivalent_results`
asserts equivalence end-to-end.

## Atomic write contract

`core::atomic::atomic_write`:

1. Create a temp file in `target.parent()` (so rename stays on the same FS).
2. `write_all` → preserve permissions → `sync_all`.
3. Optionally stage an atomic backup at `<target>.bak` if `make_bak`.
4. `tmp.persist(target)` — atomic rename on POSIX & NTFS.

Failures at step 1/2/3 leave the original file untouched. Failure at step 4
aborts with the temp file dropped automatically.

## Why not a shared "Edit AST" beyond `Op`?

`Op` is intentionally close to CLI args. Each `core::ops::<cmd>` function is a
pure `(bytes, params) → Vec<u8>` transformation. The result: every CLI command
and apply share core algorithms, while interface validation and output handling remain separate.

## Persistence and operational limits

Both individual edits and `apply` use `core::atomic::prepare_write` to stage and sync data while preserving permissions. `apply` canonicalizes target paths before grouping, so relative/absolute paths and symlinks to one target share one staged file. Symbolic links remain intact. Backup names append `.bak`; backup paths that are also batch targets are rejected.

This is a validation-then-commit pipeline, not a filesystem transaction. Phase-2 errors can leave earlier files committed; no automatic rollback or crash journal is provided. File data is synced before replacement, but parent directories are not synced; full power-loss durability is not guaranteed. Concurrent external writers are not locked out. Ownership, ACLs, extended attributes and hard-link identity are not preserved by replacement. These require a separate platform-specific design before stronger guarantees can be advertised.

Read and edit paths currently load complete files into memory. `find` now bounds retained hits and scans line offsets incrementally; literal regex replacement scans matches once. `apply --check` and `--dry-run` avoid temporary-file writes, and parallelism is bounded. Repeated operations on the same large file still allocate complete buffers; streaming or a shared line index is a future optimization that needs benchmarks and explicit edit-order semantics.

MCP shares the write core and has stdio protocol tests. Its synchronous file work runs on a single runtime thread; large requests can delay other requests and cancellation. Deploy with a trusted local client and an appropriate working directory. Workspace sandboxing and resource limits are not provided by xnip.

## Validation and performance

Regression cases are in `tests/safety.rs`; installer contracts are in `tests/install_sh.py` and `tests/install_ps1.ps1`. CI exercises Linux, macOS and Windows, minimum Rust 1.89, strict Clippy, dependency checks and source packaging. Generated GitHub skill/reference files and the generic skill share canonical docs through `xtask sync-integrations`.

A Criterion quick run measured approximately 0.20 ms for a middle-line replacement over 10K lines, 0.26 ms for a literal pattern replacement over 10K lines, 0.67 ms for indentation over 10K lines, and 4.3 ms for all matches over 100K lines. A first-match-only scan over the same in-memory input took about 80 ns. These are local core-function measurements, exclude process startup and filesystem IO, and are not a before/after comparison or a cross-platform performance guarantee.

Dependency fixes remove `unsafe-libyaml` (RUSTSEC-2024-0449) and upgrade past the advisories for anyhow (RUSTSEC-2026-0190) and crossbeam-epoch (RUSTSEC-2026-0204). YAML parsing now uses [serde-saphyr](https://docs.rs/serde-saphyr/latest/serde_saphyr/), with an explicit Rust 1.89 minimum. Release packaging and verification follow [the maintainer procedure](releasing.md).
