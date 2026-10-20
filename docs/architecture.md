# xnip architecture

xnip minimizes the context an agent must read and generate to make a correct text edit. Correctness takes priority over shorter payloads: exact matching, byte preservation, validation before writes and explicit recovery remain editing contracts.

## Goals and acceptance criteria

| Goal | Observable acceptance criterion |
|---|---|
| Smaller default interface | Five advertised MCP tools; full toolset explicitly available |
| Less repeated knowledge | CLI and MCP read adapters share selection, validation and output budgets |
| Bounded agent output | Reads default to 200 lines / 100 hits / 12,000 payload bytes; truncation is explicit |
| Cheap ordinary edits | Literal edits consume known blocks/anchors without a required read; MCP `files` groups support exact pairs and complete-file writes |
| Reliable receipts | Write, preview, check and no-op results report whether bytes were committed |
| Smaller prompts | Canonical entry guide selects operations and discloses branch-specific references |
| Behavioral verification | Fresh contract suites cover byte output, rejected writes, lifecycle, recovery, platform safety and transports |
| Reproducible cost evidence | Paired native-editor/xnip execution records real bytes, inputs and results; representation costs are a supplementary experiment |
| Harness reuse | DSH/Pi host registration and Goose stdio use the same MCP edit engine; local npm distribution runs the same pinned native executable |

The [glossary](../GLOSSARY.md) defines editing terms. Benchmark methodology and measured results live under [benchmarks](benchmarks/README.md). Representation cost is distinct from model quality or native harness reliability.

## Modules and seams

```text
CLI adapter ─┬─ read module ───────────── selection + scan + bounded text
MCP adapter ─┤
             ├─ edit module ───────────── exact blocks + lifecycle
             └─ apply module ──────────── manifests/patches + edit plan
                         └─ transaction module ─ snapshots + commit + recovery
Receipts ──── output module ───────────── compact / JSON / human presentation
```

The read module accepts a query and returns bounded output, absorbing regex validation, selection, limits and partial IO reporting. Peek uses borrowed line cursors and merges context online; find visits hits without retaining a match list. Once the byte prefix is full, unlimited find still counts selected hits for compatible receipts but stops formatting discarded hits. Both transport adapters share this seam; fixes and contract tests apply to both.

The edit module preserves literal bytes, rejects missing or ambiguous exact blocks (including overlaps), and skips identical writes. Literal insertion adds bytes beside a unique nonempty anchor without newline synthesis. Explicit whole-file write creates or replaces a regular file, rejects final symlinks and optionally checks the complete existing content; missing and empty are distinct states.

The apply module parses compact file groups, native/JSON/YAML operations and text patches into the same edit plan. Compact pairs run in order within a file; map entries are independent and normalized aliases are rejected. MCP `files` and direct `edits` use shared validation and transaction commit. Operation arrays express dependencies across paths.

Transaction entries initially share immutable byte snapshots through `Arc`. General ordered transformations borrow current virtual bytes and allocate their result without first cloning the input. Unique exact edits validate before mutation, copy shared bytes on the first real change, then reuse the buffer: equal-length edits overwrite their span and other sizes splice it. Complete replacements use the supplied bytes directly. Validated no-ops avoid output allocation; global replacements retain non-overlapping matching. Original and renamed recovery snapshots remain immutable; failed guards retain the preceding virtual state. Line-range replacement locates byte spans without materializing all lines. Independent grouped range replacements collect the required boundaries once and assemble one final buffer per file, while preserving original-range guards, newline behavior and overlap rejection. These reduce copies and scans; editing contracts remain the acceptance criteria.

MCP advertises `edit/apply/peek/find/recover` by default. `xnip mcp --toolset full` restores all ten tools, including specialized line editing and diagnostics. CLI keeps all commands. Schemas omit presentation-only titles, nullable alternatives for optional fields and false-default annotations; instructions declare that omitted input flags are false. Validation constraints remain. Owned inline manifests are consumed without cloning, and content detection tries each parser at most once. Unknown MCP fields are rejected so a misspelled preview flag cannot quietly become a write.

## Layout

- `src/cli`: argument parsing and terminal adapter.
- `src/mcp`: stdio adapter, tool discovery and structured receipts.
- `src/core/read.rs`: shared read queries and bounded rendering.
- `src/core/edit.rs`: guarded literal edits and standalone file lifecycle.
- `src/core/ops`, `location.rs`, `content.rs`: byte transformations, locators and content loading.
- `src/core/transaction.rs`: edit plans, snapshots, commit and recovery.
- `src/apply`: manifest/patch parsing and batch planning.
- `src/output`: receipts, UTF-8-safe preview budgets, human/JSON output and exit codes.
- `src/update`: background leases, singleton scheduling, release verification and platform replacement.
- `npm`: private local package preparation, native payload verification and immutable execution cache; no npm publication workflow.
- `integrations/dsh`, `integrations/pi`: host registration adapters for the existing MCP tools; `integrations/goose`: stdio configuration and concise editing hints.
- `tests`: observable CLI/MCP and distribution contracts; core/apply contracts are colocated with their modules.
- `benches`: core latency benchmarks and offline editing-cost comparisons.
- `docs/SKILL.md`: canonical agent guide, copied by `xtask sync-integrations`.

## Output contracts

`edit/apply/recover` default to one compact receipt on stderr; previews print a bounded diff on stdout. `--output json` returns one complete JSON receipt, including a full diff when requested. `--output human` prints the full diff and expanded human receipt. `apply --json` is the existing NDJSON progress format and conflicts with a nondefault `--output`. Errors retain their exit codes.

MCP ordinary writes return `written`, with no repeated input path or operation. Default structured receipts retain complete details. The server option `--receipts text` omits structured-result serialization and removes the advertised output schemas for edit/apply/recover, with the same engine and errors. Successful results omit the optional `isError` field. Counts and paths stay in structured metadata; retained journals remain visible in the text receipt. Preview/check explicitly state `written=false`; previews append a bounded diff without duplicating it in metadata. Checks and non-preview rename receipts do not build an unused diff. Preview payloads default to 12,000 bytes; `max_bytes=0` requests a full preview. Read output has independent item and byte budgets; zero disables the respective cap. A truncation notice may exceed the payload budget by a small fixed amount.

## Harness and distribution adapters

The DSH and Pi adapters register the existing xnip MCP process through each host's official extension interface. They set an absolute working directory and disable native automatic updates for that child. Host tool naming and model-facing projection remain responsibilities of the official bridges; xnip's raw result and a host's rendered text are distinct measurements. Goose uses the same stdio entry with its own extension configuration. The MCP adapter supports modern `server/discover` with per-request metadata and legacy 2025 initialization, preserving the public editing behavior and error categories.

npm packages contain the same Rust executable, with version/target identity and SHA256 checked during preparation. Host-only local packages stay private and bundle their actual host payload; an explicit public release requires six matching source-commit attestations and complete platform artifacts. Public packages use exact-version optional dependencies and are verified again before publication. The launcher verifies the packaged executable before copying it into a version/platform/hash execution directory, then atomically publishes a complete directory. Existing entries are immutable, and their hash is checked again on use. Running Windows processes map this cache instead of `node_modules`, allowing package replacement or uninstall without interrupting an existing MCP session. Cache entries are intentionally not automatically collected while process lifetimes may be unknown.

The wrapper owns the native process's input pipe and forwards stdout/stderr and exit behavior. Closing or abruptly losing the wrapper therefore closes MCP input instead of leaving the original harness writer inherited by the native child. Native automatic updating is disabled because npm owns the selected package version; the standalone updater retains its own policy. Preparation and contracts do not publish packages, edit a user profile or activate a host bundle.

The native harness benchmark adapters live under `benches/native`, independently of product editing. Pinned official editors and bridges perform the actual matching, policy and IO; their isolated surroundings and source/dependency hashes are explicit. These fixtures do not establish full agent/model performance. See [DSH](dsh.md), [Pi](pi.md), [Goose](goose.md) and the [test map](testing.md).

## Persistence and limits

Writes use temporary files in the target directory. Batch commit is atomic per file, with recovery snapshots and rollback attempts on errors. It does not provide cross-file atomic visibility or a lock against arbitrary external writers. Expected bytes are preconditions, not locks. Interrupted or incomplete recovery must be inspected before retrying.

Pure renames use no-clobber hard-link/unlink and require one filesystem. Rewritten files retain ordinary permissions but not complete ownership/ACL/xattr/hard-link identity. Create never overwrites; explicit whole-file write allows create-or-replace with an optional complete-content guard. Whole-file write and deletion refuse directories and final symbolic links. Text patches reject directory traversal, unsupported binary/mode changes and ambiguous context.

Reads still load files into memory; bounded rendering limits returned context, not input file memory. MCP performs synchronous filesystem work on a single runtime thread. Workspace sandboxing, semantic symbol analysis and binary/document editing belong to the invoking harness. Full power-loss durability is not guaranteed.

## Background update module

The update module absorbs scheduling, process leases, singleton coordination and private markers behind `begin_usage` and the hidden worker entry. Editing never makes a network request. The policy is a pure decision seam; CLI/MCP lifetimes and verified release delivery are concrete adapters exercised by real-process contracts.

A new generation after idle opens a short startup opportunity. Older live leases cancel that opportunity; a 24-hour prepared-release deadline bypasses usage-based deferral. Existing images continue running across installation. Windows workers explicitly disable handle inheritance so background HTTP cannot delay the agent's output EOF. See [update policy](updates.md) and [update tests](update-testing.md).

## Validation

Run `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo fmt --all --check`. The [test map](testing.md) covers suites and platform-specific runners. Tests use independently computed byte/sequence oracles and public request interfaces; private forwarding layout is not a test contract.
