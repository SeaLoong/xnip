# xnip — Architecture

> A 30-second overview for contributors and curious agents.

## Goals (verbatim from `PLAN.md`)

- **Project-agnostic**: no project config, no language detection, no implicit assumptions.
- **Cross-platform**: same binary on macOS / Linux / Windows.
- **Machine-friendly**: stderr for humans, stdout for machines; optional `--json` NDJSON.
- **Atomic writes**: tmpfile in same dir → fsync → atomic rename. `.bak` is opt-in via `--backup`.
- **Symmetric revert**: `--revert` inverts the same args; non-invertible ops error out.

## Layout

```
xnip/
├── Cargo.toml         # workspace = [".", "xtask"]
├── rust-toolchain.toml  # 1.95, edition 2024
├── src/
│   ├── lib.rs         # crate root; re-exports public surface
│   ├── main.rs        # binary entry → cli::run
│   ├── cli/           # clap derive + per-subcommand glue
│   │   ├── mod.rs     # Cli/Command + dispatch
│   │   ├── common.rs  # shared LocatorArgs / ContentArgs
│   │   └── {peek,find,replace,insert,move_op,indent,apply}.rs
│   ├── core/          # pure logic (no IO except atomic.rs)
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

`parse_yaml` is implemented as `serde_yaml::Value → serde_json::Value → parse_json::parse`.
This guarantees JSON ≡ YAML by construction. The native parser produces the same
`Vec<Op>` shape; the integration test `apply_three_formats_produce_equivalent_results`
asserts equivalence end-to-end.

## Atomic write contract

`core::atomic::atomic_write`:

1. Create a temp file in `target.parent()` (so rename stays on the same FS).
2. `write_all` → `sync_all`.
3. Optionally `fs::copy(target, target.with_extension("bak"))` if `make_bak`.
4. `tmp.persist(target)` — atomic rename on POSIX & NTFS.

Failures at step 1/2/3 leave the original file untouched. Failure at step 4
aborts with the temp file dropped automatically.

## Why not a shared "Edit AST" beyond `Op`?

`Op` is intentionally close to CLI args. Each `core::ops::<cmd>` function is a
pure `(bytes, params) → Vec<u8>` transformation. The result: every CLI command
and every apply op share the same algorithm, with zero duplication.
