# Changelog

User-visible changes are recorded here. Versions follow Semantic Versioning; compatibility changes during v0.x are called out explicitly.

## [Unreleased]

## [0.1.1] - 2026-10-15

First public release of xnip, a CLI and MCP stdio server for precise local text editing. The earlier v0.1.0 tag was an unpublished release candidate; no GitHub Release was published for it.

- Correct Git source installation to explicitly select the `xnip` package in this workspace.

### Features

- Nine commands: `peek`, `find`, `replace`, `insert`, `move`, `indent`, `apply`, `mcp` and `doctor`.
- Line ranges, regex line anchors, literal/regex boundary anchors and byte-pattern replacement with capture expansion.
- Native, JSON and YAML batch manifests; validation before writes, optional parallel preparation, per-file atomic replacement, previews and NDJSON execution events.
- Optional `<file>.bak` backups, expected-content checks for line replacement and constrained reverse operations.
- Eight MCP tools using the same editing core, with client configuration examples and explicit protocol/CLI differences.
- Linux musl, macOS and Windows release archives for x86_64 and ARM64; SHA256 sidecars; POSIX and PowerShell installers; six Agent integration packages.
- Generated generic/GitHub skill references, runnable tutorials, installation troubleshooting, contributor and release procedures.

### Correctness and safety

- Preserve existing file permissions and symbolic links; sync staged writes before replacement.
- Append `.bak` to the complete filename, avoiding collisions between files with different extensions. Reject batch targets that collide with backups.
- Group canonical target paths to avoid overwriting edits when one file has multiple path spellings.
- Resolve batch anchors against original content; reject mixed pattern/line edits and moves combined with other operations on the same file.
- Reject zero-match batch replacement, unknown JSON/YAML fields, invalid insertion positions, unsupported regex inversions and ignored pattern preconditions.
- Correct empty-file insertion, appending after an unterminated line, and move/revert round trips including adjacent no-ops.
- Keep `apply --json --check` valid NDJSON and reject combining JSON events with dry-run diffs.
- Preserve IO error exit codes, print help for an empty invocation and remove process-wide working-directory mutations from tests.
- Upgrade dependencies past the reviewed anyhow and crossbeam-epoch advisories; replace deprecated serde_yaml/unsafe-libyaml with serde-saphyr.

### Performance

- Stop retaining search hits at the requested cap and compute line offsets incrementally.
- Count matches during replacement instead of a separate full regex scan.
- Skip temporary writes for check/dry-run and bound parallel preparation by file count and available CPUs.
- Include Criterion benchmarks for range/pattern replacement, indentation, diff and limited/unlimited search.

### Requirements and limits

- Minimum Rust: **1.89**. Prebuilt binaries do not require Rust.
- Batch commits are atomic per file, not across the whole batch. A phase-2 failure reports already-committed paths and does not automatically roll back.
- `--was` compares the bytes read; it does not lock out concurrent writers. Replacement does not preserve hard-link identity, ownership, ACLs or extended attributes.
- `--revert` is not an undo log; information-losing transformations and ambiguous literal reversals require Git or backups for exact recovery.
- MCP write tools commit immediately; preview/check modes are available through the CLI. The server uses the launching user's filesystem permissions and is intended for trusted local clients.
- Native CLI arguments are literal shell arguments; no automatic `\n` decoding or newline-style conversion is performed.

[Unreleased]: https://github.com/SeaLoong/xnip/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/SeaLoong/xnip/releases/tag/v0.1.1
