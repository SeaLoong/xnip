# Native editing tools versus xnip

This benchmark executes each available harness's own editing implementation and xnip on fresh copies of identical files. It retains real inputs, responses, failures and final file bytes. No third-party tool is simulated by sending its patch through xnip.

- [Results and conclusions](results.md), [raw transcripts and provenance](results.json), [derived statistics](relative-results.json).
- [README summary frontier](charts/readme-frontier.svg), [per-task frontier](charts/frontier.svg), [literal-baseline overview](charts/overview.svg), [input ratios](charts/input-ratios.svg), [scenario matrix](charts/scenario-ratios.svg), [file results](charts/byte-exact.svg).
- [Native methodology](native-methodology.md), [editing-strategy diagnosis](editing-strategies.md), [runner](../../benches/native_tools.py), [report generator](../../benches/native_report.py), [native adapters](../../benches/native/).
- [Supplementary representation results](representation-results.md) and [method](representation-methodology.md) measure current xnip interfaces, schemas, prompts and output budgets separately.
- [Core performance](core-performance.md) compares current in-memory strategies and whole-process CLI planning, with [Criterion samples](core-criterion-results.json) and [CLI samples](core-performance-results.json).

## Execution matrix

The default matrix contains **13 unchanged scenarios and 31 profiles**, with three repetitions in the recorded tokenized run, plus matching-policy probes. Unavailable products are recorded rather than assigned a simulated score.

| Family | Profiles and execution |
|---|---|
| xnip MCP | Original absolute-path `xnip-edit`; case-root relative edit; pre-execution adaptive guarded/snapshot policies, each with default structured and opt-in text-only receipts |
| xnip CLI | Real stdin native programs and compact maps, each with guarded and snapshot policies; cold process startup is recorded |
| Codex | Real executable's apply_patch dispatcher; original three-line context and separately generated unique-context patches |
| Gemini CLI | Real EditTool and StandardFileSystemService; prescribed/minimal inputs, absolute/relative variants and a complete-known-file trim profile |
| OpenCode | Pinned official EditTool/ApplyPatchTool; absolute/relative literal edits, a complete-known-file trim profile and original/unique-context patches |
| aider | Pinned native diff/whole/udiff kernels with platform/LF modes; additional unique-context diff and EOF-aware whole profiles |
| Claude Code / Cursor | No independently callable official editing executor connected; not measured |

All original twelve profiles and all original thirteen task names remain in the same run. The extra profiles distinguish path configuration, edit representation and allowed preconditions rather than replacing inconvenient original rows.

**Guarded** policies retain a supplied old-content, anchor or whole-file precondition. **Snapshot** policies may use explicit whole-file upserts without an old-content check because the complete desired file is supplied. They can still select a guarded edit when it is smaller. Required-old probes always retain their original stale/ambiguous old bytes; snapshot policies cannot bypass them with a write.

The adaptive choice uses the complete serialized candidate input under the selected counter **before any candidate executes**. MCP policies also consider native literal manifests through the existing `manifest_text` argument, including its JSON wrapper cost. Snapshot-capable MCP/native CLI adds existing line programs and unique common-prefix/suffix trims derived from the complete known file; compact CLI adds trimmed pairs; relative OpenCode/Gemini profiles receive the same literal-trimming opportunity. Guarded profiles and required-old probes retain the original supplied old bytes. Candidate names, costs, input hashes and the selected contract are retained. A post-run byte oracle never chooses the candidate. These are editing-executor experiments; a complete agent A/B also needs model trajectories, reads, tool choice, retries and provider usage.

## Read the frontier and cost layers

The primary frontier selects the **best observed byte-exact supplied profile** per task, independently for input and input plus known returned text. The guarded pool accepts guarded profiles only. The snapshot-enabled pool symmetrically permits guarded and snapshot candidates on both sides, since allowing a whole write does not require using one. Xnip MCP and CLI pools remain separate, and winners retain their actual contract. Native candidates can span native transports; the winning profile and its transport are named.

Ratios are native best / xnip best; values above 1 mean xnip's observed input is smaller. Each task gets one geometric-mean vote. The summary chart also reports xnip wins, ties, losses and unavailable comparisons. This derived frontier is not a claim of global optimality or a strategy replayed by the runner.

| Layer | Counted evidence |
|---|---|
| Executor input | Complete tool argument object, native patch/edit message, or complete CLI stdin program |
| Captured returned text | Actual tool text/error at the recorded native boundary; aider's final model-facing return remains unknown |
| Serialized MCP reply | Canonical serialization of the captured normalized JSON-RPC reply, including protocol/result/structured fields, when recorded |
| CLI invocation | Executed argv displayed with host-platform quoting; binary path and `apply --format ... --from-stdin` are counted separately |
| Serialized MCP request | Captured protocol request with tool name and arguments; overlaps executor input and is not added to it |

Two paired `-text` strategies start MCP with `--receipts text`; their defaults keep structured receipts. The report compares actual full replies only when both profiles are byte-exact and their actual inputs and returned text match exactly. The supplementary suite also discovers a real compact/text tool catalog, so the output-schema change is measured rather than inferred.

Unknown values stay unknown. These layers describe different boundaries and must not be summed indiscriminately. The full MCP object may contain fields a host does not put into model context. No column represents billed usage.

The full report also retains ratios to the original literal baseline, all-task and byte-exact paired cohorts, sensitivity without the extreme long line, and every nonexact result. CLI cold-start timing and warm MCP/kernel timing are kept separate; no mixed latency ranking is made.

## Reproduce

Python 3.10+, Node 24, a built xnip and the selected native implementations are required. Aider setup uses Git and uv to create an isolated Python 3.13 environment under target/native-bench.

~~~sh
cargo build --release --locked
python benches/native/setup_aider.py
python benches/native_tools.py --xnip target/release/xnip --repeats 3 --json target/native-bench/results.json --markdown target/native-bench/results.md
~~~

On Windows, use `target/release/xnip.exe`. Codex is discovered on PATH or in the desktop app's bundled binaries; select one with `--codex` or `XNIP_BENCH_CODEX`. Gemini uses the installed core package or `GEMINI_CORE_PATH`. OpenCode preparation uses SHA-verified pinned official sources and locked dependencies under target/native-bench. No account, inference provider or model call is required.

For the recorded tokenizer:

~~~sh
python -m pip install --target target/benchmark-python tiktoken==0.14.0
python benches/native_tools.py --xnip target/release/xnip --encoding o200k_base --repeats 3 --json docs/benchmarks/results.json --markdown docs/benchmarks/results.md
~~~

The runner loads that private dependency directory automatically. Counts are exact for the normalized strings and named encoding; without `--encoding` it reports exact UTF-8 bytes and null tokens. A first tokenizer vocabulary load may need the network; cache it for offline reruns. Every xnip subprocess has `XNIP_AUTO_UPDATE=0`.

Finish builds and source/integration generation before measuring. Adapter and binary changes during a run prevent publication. Original calls and normalized copies remain available; random temporary root names do not determine the score.

## Regenerate reports without editors

~~~sh
python benches/native_tools.py --report-only docs/benchmarks/results.json --markdown docs/benchmarks/results.md
python tests/native_benchmark_contract.py
~~~

Report-only uses the Python standard library. It invokes no editor or tokenizer, performs no network request and leaves raw JSON/provenance unchanged. It writes Markdown, derived JSON and six deterministic SVG charts. The derived JSON records the raw JSON SHA-256.

## Supplementary measurements

~~~sh
python benches/edit_cost.py --xnip target/release/xnip --encoding o200k_base --repeats 3 --json docs/benchmarks/representation-results.json --markdown docs/benchmarks/representation-results.md
~~~

The supplementary suite verifies 139 xnip format/fixture executions per repetition and records actual catalogs, current integration prompts, reads, preview budgets, output modes and rejected-call recovery. Its third-party serialization examples are not native executions; use the primary suite for product behavior.
