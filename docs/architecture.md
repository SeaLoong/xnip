# xnip architecture

xnip minimizes the context an agent must read and generate to make a correct text edit. Correctness takes priority over shorter payloads: exact matching, byte preservation, validation before writes and explicit recovery remain editing contracts.

## Goals and acceptance criteria

| Goal | Observable acceptance criterion |
|---|---|
| Smaller default interface | Five advertised MCP tools; full toolset explicitly available |
| Less repeated knowledge | CLI and MCP read adapters share selection, validation and output budgets |
| Bounded agent output | Reads default to 200 lines / 100 hits / 12,000 payload bytes; truncation is explicit |
| Cheap ordinary edits | Exact replacement consumes known old/new blocks without a required read; MCP batches accept direct `edits` arrays |
| Reliable receipts | Write, preview, check and no-op results report whether bytes were committed |
| Smaller prompts | Canonical entry guide selects operations and discloses branch-specific references |
| Behavioral verification | Fresh contract suites cover byte output, rejected writes, lifecycle, recovery, platform safety and transports |
| Reproducible cost evidence | Paired native-editor/xnip execution records real bytes, inputs and results; representation costs are a supplementary experiment |

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

The edit module preserves literal bytes, rejects missing or ambiguous exact blocks (including overlaps), and skips identical writes. The apply module parses native/JSON/YAML/text patches into operations and validates their edit plan. MCP direct `edits` are ordered exact replacements using that same plan; they do not bypass transaction validation.

MCP advertises `edit/apply/peek/find/recover` by default. `xnip mcp --toolset full` restores all ten tools, including specialized line editing and diagnostics. CLI keeps all commands. Schemas omit presentation-only titles and nullable alternatives for optional fields; optional inputs can be omitted. Unknown MCP fields are rejected so a misspelled preview flag cannot quietly become a write.

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
- `tests`: observable CLI/MCP and distribution contracts; core/apply contracts are colocated with their modules.
- `benches`: core latency benchmarks and offline editing-cost comparisons.
- `docs/SKILL.md`: canonical agent guide, copied by `xtask sync-integrations`.

## Output contracts

`edit/apply/recover` default to one compact receipt on stderr; previews print a bounded diff on stdout. `--output json` returns one complete JSON receipt, including a full diff when requested. `--output human` prints the full diff and expanded human receipt. `apply --json` is the existing NDJSON progress format and conflicts with a nondefault `--output`. Errors retain their exit codes.

MCP ordinary writes return `written`, with no repeated input path or operation; complete details stay in structured metadata. Multiple-match counts, batch path counts and retained journals remain visible. Preview/check explicitly state `written=false`; previews append a bounded diff without duplicating it in metadata. Checks and non-preview rename receipts do not build an unused diff. Preview payloads default to 12,000 bytes; `max_bytes=0` requests a full preview. Read output has independent item and byte budgets; zero disables the respective cap. A truncation notice may exceed the payload budget by a small fixed amount.

## Persistence and limits

Writes use temporary files in the target directory. Batch commit is atomic per file, with recovery snapshots and rollback attempts on errors. It does not provide cross-file atomic visibility or a lock against arbitrary external writers. Expected bytes are preconditions, not locks. Interrupted or incomplete recovery must be inspected before retrying.

Pure renames use no-clobber hard-link/unlink and require one filesystem. Rewritten files retain ordinary permissions but not complete ownership/ACL/xattr/hard-link identity. Create never overwrites; deletion refuses directories and symbolic links. Text patches reject directory traversal, unsupported binary/mode changes and ambiguous context.

Reads still load files into memory; bounded rendering limits returned context, not input file memory. MCP performs synchronous filesystem work on a single runtime thread. Workspace sandboxing, semantic symbol analysis and binary/document editing belong to the invoking harness. Full power-loss durability is not guaranteed.

## Background update module

The update module absorbs scheduling, process leases, singleton coordination and private markers behind `begin_usage` and the hidden worker entry. Editing never makes a network request. The policy is a pure decision seam; CLI/MCP lifetimes and verified release delivery are concrete adapters exercised by real-process contracts.

A new generation after idle opens a short startup opportunity. Older live leases cancel that opportunity; a 24-hour prepared-release deadline bypasses usage-based deferral. Existing images continue running across installation. Windows workers explicitly disable handle inheritance so background HTTP cannot delay the agent's output EOF. See [update policy](updates.md) and [update tests](update-testing.md).

## Validation

Run `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo fmt --all --check`. The [test map](testing.md) covers suites and platform-specific runners. Tests use independently computed byte/sequence oracles and public request interfaces; private forwarding layout is not a test contract.
