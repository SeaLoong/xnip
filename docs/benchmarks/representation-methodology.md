# Supplementary representation and xnip measurements

This experiment measures complete serialized edit representations and current xnip interfaces. Outputs are [representation-results.md](representation-results.md) and [representation-results.json](representation-results.json); the runner is [edit_cost.py](../../benches/edit_cost.py). The [primary native experiment](README.md) executes actual third-party editors and adaptive xnip policies.

## Fixtures and verified interfaces

The thirteen unchanged fixtures cover small configuration changes, a local change in a 2,000-line file, multiline code, Unicode, CRLF, missing EOF newline, repeated text, global replacement, same/multiple-file edits, a long line, insertion and deletion. Separate cases exercise file lifecycle and rejected edits followed by a read/corrected retry.

Every xnip execution checks the expected file bytes. The suite runs:

- independent exact MCP edits and direct edits-array batches;
- compact `files` maps with ordered guarded pairs or full old-file guards;
- explicit snapshot maps and write operations without old-content preconditions;
- line manifests, native literal manifests and compact CLI maps;
- imported conventional/apply_patch-style patches where the example generator supports the newline semantics.

There are **139 format/fixture executions per repetition** with the complete current matrix, plus recovery/output probes. Guarded and snapshot rows are labeled separately. A smaller explicit whole write has a different precondition contract from literal replacement.

Other columns serialize whole-file JSON, old/new JSON and aider-style blocks. They measure representation size, not those products' matching behavior, success rates, latency or model quality. Use the primary experiment for real Codex/Gemini/OpenCode/aider execution.

## Accounting

| Cost | Captured boundary |
|---|---|
| Catalog | Actual initialize instructions plus tools/list: compact/structured, full/structured, compact/text receipts |
| Agent guidance | Current installed-role entry points with text, hashes and counts; disclosed references excluded |
| Initial context | Known old blocks or full files, as separate supplied-context assumptions |
| Payload | Complete serialized arguments or complete textual manifest/patch |
| Result | Normalized serialized xnip MCP result or CLI stdout/stderr record |
| Retry | Rejected call, actual read, corrected call and their responses |
| Local runtime | Median actual execution with warm MCP/cold CLI scope identified |

Independent strings/calls are counted separately so unrelated contents do not merge token boundaries. A bounded per-counter cache reuses exact-string measurements; independent calls are still added separately, and cached results cannot be changed by callers. Known-block and full-file context are alternatives, not cumulative charges. No model generates the predetermined payloads.

Native manifest and compact-map CLI payloads are raw stdin. Encoding the same program inside MCP manifest_text adds a JSON envelope and escaping; direct typed maps avoid that string nesting. The primary suite additionally executes raw multiline native programs, safe bare-token encodings and pre-execution selection, with invocation cost shown separately. This supplementary suite retains its fixed canonical representation choices rather than claiming every row is a minimal encoding. The primary suite additionally explores snapshot-only original-coordinate line manifests/native CLI programs and known-file unique literal trims across MCP/native/compact transports, with equivalent literal-trim profiles for OpenCode and Gemini; these policy candidates do not alter this supplementary suite's 139 fixed rows.

The simple patch/block example generator excludes CRLF and missing-EOF cases because it does not encode their special newline semantics. This is a generator restriction; the primary native experiment retains real accepted encodings and failures.

MCP result counting here serializes the returned result object. The primary report separately records captured tool text, complete serialized protocol reply and invocation/request overhead. Their accounting boundaries differ, so the columns must not be interchanged.

## Reproduce

~~~sh
cargo build --release --locked
python benches/edit_cost.py --xnip target/release/xnip --repeats 3 --json target/edit-cost/representation-results.json --markdown target/edit-cost/representation-results.md
~~~

On Windows, use `target/release/xnip.exe`. Python 3.10+ and a built xnip suffice for exact UTF-8 byte and character counts; tokens remain null.

~~~sh
python -m pip install --target target/benchmark-python tiktoken==0.14.0
python benches/edit_cost.py --xnip target/release/xnip --encoding o200k_base --repeats 3 --json docs/benchmarks/representation-results.json --markdown docs/benchmarks/representation-results.md
~~~

Counts are exact for the named o200k_base encoding and captured serialization. A first vocabulary load may use the network; cache it for offline use. Provider framing/billing, hidden prompts, caching and reasoning are outside scope. No bytes-divided-by-four estimates are labeled tokens.

Each xnip subprocess has XNIP_AUTO_UPDATE=0 without changing the host environment. Output probes compare current bounded/unlimited reads and previews plus compact/human/JSON CLI modes; they do not require a historical binary baseline.

## Provenance and interpretation

JSON retains fixture/goal bytes, every generated payload, real responses, catalog snapshots, instruction text/hashes/counts and retries. A source fingerprint includes tracked and untracked Rust/build/instruction inputs and the runner. Source and executable hashes are checked before/after execution; changes prevent publication of a mixed run. Finish builds and integration generation first.

Whole-file output can be smaller on a short final file, while line context can dominate a substring edit in a long line. Shared-file batches avoid repeated paths. The primary suite measures these strategies under explicit guarded/snapshot contracts, preserves the original literal baseline and reports a best-observed frontier rather than a universal winner.

Representation choices follow primary documentation: [Codex patch implementation](https://github.com/openai/codex/tree/main/codex-rs/apply-patch), [aider formats](https://aider.chat/docs/more/edit-formats.html), [OpenCode tools](https://opencode.ai/docs/tools/) and [Gemini source](https://github.com/google-gemini/gemini-cli/tree/main/packages/core/src/tools). Exact native revisions and source hashes are in the primary run. No invented Claude Code or Cursor executor is scored.

A complete agent study must also retain model/harness versions, instructions, schemas, initial reads, selected tools, attempts, provider usage and task-level assertions. These offline suites make no paid model calls.
