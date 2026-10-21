# Changelog

User-visible changes are recorded here. Versions follow Semantic Versioning; compatibility changes during v0.x are called out explicitly.

## [Unreleased]

## [0.5.5] - 2026-10-22

DSH, Pi and Goose integrations, expanded native-editor benchmarks, and npm distribution of the same Rust CLI/MCP executable.

- Add DSH bundle/MCP, Pi session extension and Goose stdio configurations, with explicit cwd, lifecycle and local-package setup.
- Extend horizontal native-editor benchmarks with pinned official DSH, Pi and Goose executors, guarded and whole-write strategies, observable read prerequisites, byte-level checks and separate host MCP accounting.
- Upgrade the MCP SDK for Goose's default modern discovery negotiation while retaining legacy initialization; test both protocol lifecycles, error recovery and clean EOF shutdown.
- Distribute the `xnip` executable through the `xnip-cli` npm parent and six `xnip-*` platform packages, with `xnip-cli/native`, `xnip-cli/dsh` and `xnip-cli/pi` entries. Verify native payload hashes, embedded build identities, immutable execution caches and offline local tarballs. Keep local preparation private and require an explicit, complete public release.
- Cover real harness registration/disposal, editing semantics, package integrity, concurrent first launch, active-session package removal and abrupt wrapper shutdown. Add scoped CI checks and refresh integration, benchmark and packaging documentation.
- Handle delayed npm registry visibility by submitting native packages as a batch and verifying every platform before publishing the parent. Report definite npm rejections with credential-free error categories after one exact-version check; retain bounded verification for uncertain network results.
- Build and validate npm tarballs from the same six-platform release artifacts in CI, publish platform packages before the parent with explicit stable/development channels, verify registry integrity, and preserve resumable release behavior.
- Update `rustls-webpki` to 0.103.16 and `webpki-roots` to 1.0.10 to address RUSTSEC-2026-0382 and RUSTSEC-2026-0381; retain the bundled certificate data license notice.
- Disable dev-profile debug symbols in CI while retaining assertions and payload-size checks; locate the bundled npm CLI through both Windows and POSIX installation layouts.

## [0.4.0] - 2026-10-19

Smaller edit representations, reusable batch buffers and concise MCP receipts. Whole-file writes, literal insertion and compact file groups share the same validation and recovery path.

- Add explicit byte-preserving whole-file writes with optional complete-content guards, plus insertion beside a unique literal anchor.
- Accept compact file-group batches in MCP and CLI: share paths across ordered exact pairs or whole-file writes, reject aliases and validate all groups before committing. Use shared Windows Unicode path identities for alias checks and transaction lookup.
- Support multiline quoted native payloads and guarded `write` instructions without nested JSON escaping.
- Reduce line replacement to byte-span copies, assemble independent line batches once and share transaction snapshots. Ordered unique exact edits reuse copy-on-write buffers after validating each guard; identical edits avoid result allocation.
- Keep normal MCP batch and global-edit receipts to `written`; retain paths, match counts and execution modes in structured results. Add opt-in `mcp --receipts text` to omit structured results and their output schemas for agent clients, without constructing discarded metadata. Omit optional success flags and redundant false-default schema annotations.
- Preserve detailed MCP parameter error causes, move owned manifest strings without cloning, and detect inline JSON without duplicate native parsing.
- Add scoped CI verification, six-platform release builds and automatic development prereleases; stable tags reuse full verification for the exact release commit.
- Refresh CLI/MCP, integration, architecture, update and benchmark documentation and packaged agent references.
- Expand horizontal native benchmarks with relative paths, guarded and snapshot strategies, tighter native encodings, separate transport costs and byte-exact per-task frontiers.

## [0.3.1] - 2026-10-18

Smaller edit inputs and responses, byte-preserving operations, and silent background updates. The README now includes reproducible native-editor benchmark conclusions and an overview chart.

### Compatibility

- MCP advertises five tools by default: edit, apply, peek, find and recover. Use `xnip mcp --toolset full` to expose all ten tools.
- Reads default to 200 lines or 100 search hits and 12,000 payload bytes. Previews also default to 12,000 bytes; zero disables the corresponding cap. Truncation is explicit, including quiet mode.
- MCP rejects unknown arguments, including misspelled preview/check flags.
- `apply --check` no longer writes stdout `OK`. Use structured JSON receipts for scripts.
- Independent overlapping line edits are rejected; use `--sequential` for dependent operations.

### Editing and context

- Accept direct ordered `edits` arrays for MCP exact batches, without a nested manifest string.
- Reduce ordinary MCP write text to `written`; retain complete structured results and exceptional details. CLI defaults to one compact receipt, with JSON and expanded human output available.
- Share CLI/MCP read selection and budget behavior. Stream line selections and search hits without retaining a full line index or match list. Skip unused diffs during checks and non-preview renames.
- Fix empty-marker panics, context overflow, IO exit classification and missing retained rename journals.
- Provide concise Codex AGENTS instructions and an on-demand guide; synchronize the six editing references across integrations.

### Silent updates

- Add detached, nonblocking update checks with an OS-lock singleton per installation. Live MCP leases and a 30-minute CLI idle tail defer installation until the next eligible use; a prepared update has a 24-hour maximum deferral.
- Prefer GH-Proxy for public release metadata and assets; retry failed delivery directly against GitHub within one bounded HTTP budget. `XNIP_UPDATE_PROXY=direct` selects the official HTTPS route alone.
- Verify stable newer releases, archive checksums, paths and executable hashes. Existing mapped processes keep running across replacement. Windows state replacement supports concurrent readers without sleeps or retries.
- Keep checks, failures, fallback markers and retained backups in private state. No update notifications or text enter CLI/MCP responses.

### Evidence and distribution

- Execute native Codex, Gemini CLI, OpenCode and aider editing tools against xnip on 13 scenarios, with three repetitions, byte-level oracles, paired relative costs and reproducible charts. These measure editing executors, not full agent/model token usage.
- Rewrite behavior contracts around independent byte and sequence oracles, including lifecycle, recovery, budgets, transports and updater subprocesses.
- Automatically build six release targets after qualifying default-branch changes and successful CI. Verify all 21 assets in a draft before publishing; support safe reruns and protect existing tags/assets.
- A matching stable tag on the same commit suppresses a duplicate automatic prerelease. Stable releases include the matching changelog section in their release notes.

## [0.2.0] - 2026-10-16

- Add no-clobber whole-file rename to CLI/MCP edit, with same-filesystem identity preservation and explicit conflict checks.
- Import unified text diffs and apply_patch-style documents through `apply --format patch`; validate context, line counts, path boundaries and EOF markers.
- Add native/JSON/YAML create/delete/rename/edit batch operations and ordered virtual planning, with `--sequential` for dependent legacy operations.
- Route batch commits and renames through recovery snapshots, change detection and rollback attempts. Add `recover`/`xnip_recover` for explicit conflict-checked restoration; retain journals on failure.
- Add MCP apply previews/checks without stdout protocol pollution, and synchronize patch/recovery reference documents into installed skills.

- Add `edit` and `xnip_edit` as the primary editing entry point: create missing files without overwrite, replace exact blocks with unique-match guards, delete regular files, and preview/check across CLI and MCP. Preserve literal bytes and skip unchanged replacements.
- Rework the editing skill around replacing the default text editor while reusing known context; generate generic/GitHub/Claude variants and package their on-demand references. Document capability gaps and development/release differences.
- Preserve the preferred manifest parser's diagnostic on failed auto-detection instead of masking a JSON/YAML schema error with an unrelated fallback error.

- Fix `xnip_doctor` hanging in a persistent Windows MCP session by capturing terminal status before the protocol reader starts; add an end-to-end test that keeps stdin open across requests.

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

[Unreleased]: https://github.com/SeaLoong/xnip/compare/v0.5.5...HEAD
[0.5.5]: https://github.com/SeaLoong/xnip/releases/tag/v0.5.5
[0.4.0]: https://github.com/SeaLoong/xnip/releases/tag/v0.4.0
[0.3.1]: https://github.com/SeaLoong/xnip/releases/tag/v0.3.1
[0.2.0]: https://github.com/SeaLoong/xnip/releases/tag/v0.2.0
[0.1.1]: https://github.com/SeaLoong/xnip/releases/tag/v0.1.1
