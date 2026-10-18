# Native editor execution methodology

## Question and evidence

This experiment compares real editing executors on identical known-input tasks: supplied input, captured results, matching/rejection policy and resulting file bytes. It does not invoke a model. The [supplementary suite](representation-methodology.md) instead compares serialized representations and current xnip interfaces.

Every repetition receives a fresh workspace. The oracle compares every file, including unexpected side effects, with the goal bytes. Native success status alone cannot pass it. The thirteen original tasks cover small/large edits, multiline code, Unicode, CRLF, missing EOF newline, disambiguation, global replacement, same/multiple-file edits, a long line, insertion and deletion.

All original twelve profiles remain. Added path, unique-context, whole-file and adaptive configurations are separately named. The full available matrix has 31 profiles; actual availability, versions and repetition count are in raw JSON.

## Native implementations

Codex is the installed executable, versioned and fingerprinted by SHA-256. Its native argv0=apply_patch dispatcher reads the unchanged patch from stdin. The fixture cwd is the case workspace. No Python patch reproduction or xnip patch parser stands in for Codex.

Gemini CLI is the installed core package's unchanged EditTool and StandardFileSystemService. The adapter supplies workspace authorization and captures telemetry/console output. Its default correction path remains enabled, but generateJson calls are recorded and blocked before inference. Prescribed-context and executor-accepted minimal profiles remain separate.

OpenCode is pinned to official commit `e3c4eb242585dcd77be1a867391b7c25fd47ce52` / 1.19.3. Matching, validation, filesystem operations, diffs and output handling use SHA-verified upstream sources. LSP, formatting, permission UI, events and agent lookup are isolated surrounding adapters and are disclosed in provenance.

Aider is pinned to `5dc9490bb35f9729ef2c95d00a19ccd30c26339c`. Its get_edits/apply_edits and InputOutput perform parsing and IO. Model orchestration, authorization, git commits, linting and reflection are outside this editing-kernel call. The final model-visible message is unavailable; an empty kernel output is not a measured zero-token agent response.

Native source files, binary hashes, package versions, lockfiles and adapter fingerprints remain in the raw report. Claude Code and Cursor are unavailable in this experiment and receive no invented schema or simulated score.

## Path and context configurations

The original `xnip-edit` profile retains one MCP process at the outer temporary directory and sends absolute case paths. New relative/adaptive MCP profiles launch a process with the actual case directory as cwd; they never change the parent process cwd. Initialization is timed separately from warm calls.

Codex's cwd, OpenCode's instance directory and aider's coder.root are the actual case root. Original Gemini/OpenCode literal profiles keep absolute arguments. Added relative variants invoke their real existing path handling: OpenCode joins its instance directory; Gemini validates/corrects relative paths against its configured target directory. Original and native-corrected arguments are retained. Workspace configuration savings are not attributed to a new matching algorithm.

Original patches/aider blocks use three context lines. Added unique-context variants enumerate symmetric 0..3-line contexts and select the smallest serialized uniquely anchored candidate; if none is unique, they widen until one exists. This selection reads the known source and intended change, never execution results. It is a stated candidate set rather than proof of the shortest native encoding.

The prescribed three-line Gemini profile remains intact, alongside its minimal accepted profile and corresponding relative variants. Additional `gemini-relative-known-file-trim` and `opencode-relative-known-file-trim` profiles use the same known-file literal-trimming helper as snapshot-capable xnip. Their permission cohort is snapshot because deriving a shorter guard needs the supplied complete file; their selected edits still carry a unique old literal and retain the actual guarded contract. Required-old probes use the unchanged supplied old text. Mandatory native Read/prompt workflows are outside this known-context experiment, so these inputs are not complete agent-context totals.

Aider platform mode follows the host newline policy; LF is explicit. Original whole-file bodies use closed fences. Separate EOF-aware whole profiles close newline-terminated goals and use an unclosed last fence only for goals with no final newline and no trailing whitespace that the native response accessor would strip. Earlier files are closed to delimit the next listing. Other unterminated trailing-whitespace goals are not claimed as supported by that encoding. Native InputOutput still applies its configured line endings. LF normalization is reported separately and does not erase missing final-newline differences.

## Xnip contracts and pre-execution selection

| Profile | Contract and transport |
|---|---|
| xnip-edit | Original absolute-path literal edits, one call per supplied block; shared warm MCP |
| xnip-relative | Same supplied guarded edits using relative slash paths and case-root MCP |
| xnip-adaptive-guarded | Relative MCP; exact calls, direct batches, compact per-file pairs, native literal manifests, anchor insertion or guarded whole writes |
| xnip-adaptive-snapshot | Relative MCP; guarded candidates, known-file unique trims, existing native line manifests, explicit whole-file and mixed per-file upserts |
| xnip-adaptive-guarded-text / snapshot-text | Same candidate selection as the structured profiles, starting MCP once with `--receipts text` |
| xnip-cli-native-guarded/snapshot | Actual complete native stdin programs; snapshot also reuses known-file trims and line programs; cold CLI process |
| xnip-cli-compact-guarded/snapshot | Actual complete compact maps; snapshot also reuses known-file trimmed pairs; cold CLI process |

Guarded means an old block/anchor or whole expected old file is supplied. Snapshot permits complete whole-file upserts without old bytes because the full desired snapshot is already available; such a policy can still select a guarded edit. Contract groups describe permitted operations, while selection records the actual operation/precondition chosen.

Candidates are built from the known before/after fixture and supplied edits, scored under the run's exact token or byte counter, then chosen before any request executes. Ties prefer fewer calls, then a guarded contract, then deterministic name order. The record includes all candidate costs and input hashes, the chosen candidate and a flag stating that observed output was not consulted.

Compact per-file old/text pairs retain order. Global replacements keep an explicit all flag through a supported literal interface, or an eligible complete-file guard/write; an ambiguous pair is not silently treated as a global replacement. Native programs enumerate equivalent safe bare/quoted tokens and literal-multiline/escaped encodings, counting the whole resulting program. Reserved payload/locator prefixes remain quoted. MCP policies reuse these programs through the existing `manifest_text` argument, scoring the complete JSON wrapper; both policies also consider omitting the optional final record newline. Required-old probes keep their original edit form and supplied old bytes.

Snapshot-only literal candidates remove equal prefixes/suffixes from each original edit while keeping old nonempty and unique, counting overlapping occurrences as Rust does. Each edit is checked against the virtual file produced by preceding edits; its final virtual files must match the supplied goal. Explicit all-match edits keep their original old/text and all flag. Small shared contexts enumerate trim offsets; large shared contexts use bounded offsets. The smallest compact pair serialization is retained for each edit before complete calls, direct batches and per-file pairs are scored. Original guarded representations remain eligible, so this bounded heuristic is not a claim of a global minimum.

Snapshot-only line candidates use the existing default `xnip_apply({manifest_text})` interface. LF-only diff groups with 0/1/3 context produce original-coordinate replace/insert programs; records are emitted descending. Payloads are always quoted so modifier-looking content cannot become a guard, locator or external payload. Source-derived simulation admits a program only when it preserves the complete expected bytes, including CRLF, blank lines and EOF. A terminal payload LF is omitted only when the existing operation's automatic LF restores the exact payload; an empty inserted line and repeated trailing LF cannot be shortened that way. A program need not end with a record newline. These are current five-tool capabilities, with no added tool catalog. CLI snapshot policies reuse the same helper and actual counter: native profiles receive the complete line program directly, while compact profiles add only trimmed compact pairs. Guarded and required-old CLI profiles preserve their original candidates and old bytes.

The snapshot policy does not receive a free hidden read. These fixtures already supply complete before/after files to all generators. A live policy must charge any additional read or full-file generation needed to make a snapshot available. Server-side persistence optimization cannot remove input already transmitted.

## Probes and file oracle

Separate probes observe ambiguous targets, stale old bytes, whitespace drift, overlapping literals and aider's accepted unclosed whole encoding. Unsupported representation/probe combinations are omitted explicitly.

Required-old probes force every xnip policy, including snapshot-capable policies, to use the exact supplied old bytes. They cannot substitute the current file as a regenerated guard, convert the attempt into an insertion or use unconditional write. These probes retain native rejection/mutation/repair behavior rather than assigning a universal quality score.

Each output is checked against expected bytes in every repetition. CRLF, EOF, missing/extra files, native status and whether a file remained unchanged are retained. A byte-incompatible output remains visible and cannot qualify for the equivalent-outcome frontier.

## Cost layers and timing

Inputs are complete native edit messages, tool argument objects or full CLI stdin programs. Counter totals independently tokenize separate calls. The counter caches repeated exact strings within bounded per-instance storage; caching does not merge calls or alter their summed cost. Temporary case roots are normalized to /workspace; platform separators are retained where present. Exact token counts name the encoding; bytes-only runs leave tokens null.

Captured native returned text is distinct from adapter debug objects and a complete host/provider response. Aider's final returned text is unknown. Blocked Gemini repair requests are captured separately, never executed or charged as paid inference.

For xnip MCP, the runner additionally captures the protocol request and reply objects. Canonical serialization of the normalized JSON-RPC reply includes protocol IDs, content and structured fields; it can be much larger than the one-line returned text and may contain material a host never injects into model context. The serialized request includes its arguments and overlaps the input column, so it is not added to input again.

Two opt-in text-receipt profiles retain the same candidate input policy while the server omits structuredContent and outputSchema. Default structured profiles remain. Paired receipt statistics require byte equality in all repetitions, known complete reply costs, identical captured inputs and identical returned text. Equal token counts alone do not prove equivalent content. The resulting ratio isolates the captured canonical full JSON-RPC reply layer; unknown evidence stays unavailable and is excluded from sums/geometric means.

For CLI profiles, invocation_cost counts the executed argv displayed with host-platform quoting, including the real binary path and apply/format/from-stdin arguments. The complete stdin program is the executor input. These are separate layers; a shell harness may have further quoting/framing costs. A raw-program advantage does not establish that using a shell costs less than an MCP call in a complete agent.

CLI elapsed time includes cold process startup. MCP/kernel call time excludes initialization, with MCP setup timings recorded separately. No cross-transport speed ranking is calculated. Every xnip subprocess receives XNIP_AUTO_UPDATE=0, and native adapters block inference/network during execution.

## Statistics and visualizations

The report preserves all-task and byte-exact paired ratios against the original literal xnip-edit baseline. An all-task payload statistic includes incompatible native bytes; exact pairs require both profiles to meet the goal in every repetition. Cohorts can differ in members and size.

The best-observed frontier instead groups profiles by guarded/snapshot contract and keeps xnip MCP/CLI transports separate. For each task, it independently selects:

1. the lowest observed input among byte-exact profiles;
2. the lowest input plus known captured returned text among byte-exact profiles.

Native minima can span native transports; every winner retains its transport and actual contract. The guarded pool excludes snapshot profiles. The snapshot-enabled pool symmetrically admits guarded and snapshot candidates for both native and xnip: permission to overwrite a complete file does not force that representation on a local edit. Aider can win an input comparison but cannot supply an unknown combined-cost value. Full serialized MCP replies and invocation/request costs remain additional columns rather than fabricated native/provider measurements.

The ratio is native best / xnip best. Aggregation is exp(mean(log(per-task ratio))), with one vote per task. Deterministic repetitions verify stability; they do not create extra independent tasks. The compact summary reports xnip input wins, near-equal ties using math.isclose, losses and missing comparisons. A frontier is a post-run descriptive lower envelope, not a strategy secretly fed back to execution or a claim of global optimality.

Six deterministic SVGs and derived JSON are generated by [native_report.py](../../benches/native_report.py): literal overview, input geometric means, task matrix, byte correctness, per-task transport-separated frontier and compact README frontier. Report-only never changes raw data or sampling fingerprints. Derived JSON records the raw SHA-256, the renderer fingerprint sampled during execution, and the current renderer SHA-256, so a later layout-only regeneration has separate provenance. See [reproduction](README.md).

A full agent study remains separate: hold model/harness/context constant, retain mandatory reads, all selected tools and retries, provider usage and repository-level assertions. Neither offline suite measures total agent savings or paid billing.
