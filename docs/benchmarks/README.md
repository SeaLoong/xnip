# Native editing tools versus xnip

The primary benchmark executes each available harness's own editor against the same isolated files, then executes xnip against fresh copies. It records actual native inputs, outputs, status and final file bytes. No third-party patch is passed through xnip to stand in for that product.

- [Results, relative comparisons and conclusions](results.md).
- [Raw transcripts and provenance](results.json), [derived paired ratios](relative-results.json).
- Charts: [overview](charts/overview.svg), [input geometric means](charts/input-ratios.svg), [scenario ratios](charts/scenario-ratios.svg), [file correctness](charts/byte-exact.svg).
- [Native method and limits](native-methodology.md), [runner](../../benches/native_tools.py), [report generator](../../benches/native_report.py), [adapters](../../benches/native/).
- [Supplementary representation results](representation-results.md) and [method](representation-methodology.md): serialized formats and current xnip catalogs, prompts, batches and output modes. This is a separate experiment.

## What is actually run

| Harness | Native execution path | Scope |
|---|---|---|
| Codex | Installed Codex executable's own argv0=apply_patch stdin dispatcher | Native patch executor |
| Gemini CLI | Installed EditTool.build(args).execute(signal) and StandardFileSystemService | Native edit executor; model repair requests captured and blocked |
| OpenCode | Pinned official EditTool / ApplyPatchTool / Tool / FSUtil / Patch implementations | Native executors with isolated surrounding context |
| aider | Pinned official get_edits → apply_edits → InputOutput, for diff/whole/udiff | Native editing kernel, including its read/write and newline policy |
| xnip | Real stdio MCP xnip_edit | Same editing tasks, automatic updates disabled |
| Claude Code / Cursor | No independently invocable native editor connected for this run | Not measured; no simulated score |

The recorded run has 13 tasks, 12 profiles and 3 repetitions, plus separate matching-policy probes. The report names the installed versions and pinned source revisions. Gemini has both its prompt-prescribed context profile and an executor-accepted minimal profile. Aider has explicit platform and LF newline profiles.

These are tool executions, not complete agent sessions. Predetermined edits measure file semantics and captured payloads. A complete agent comparison requires a separate same-harness, same-model native-versus-xnip A/B, including mandatory reads, tool selection, retries and provider usage. Aider's final model-facing response is not captured; its response cost is unknown.

## Read the relative values

Every ratio is native input / xnip input for the same task; xnip = 1x. The report publishes an equal-task geometric mean for all supplied inputs and separately for tasks where both editors are byte-exact in every repetition. Exact subsets differ between profiles; they do not form a common quality ranking. The all-task metric includes byte-incompatible outputs and is a payload-size observation.

Returned-text and input-plus-captured-return ratios use separate columns for the six known output boundaries. Aider is unknown and is excluded from those comparisons. These observable strings exclude schemas, initialize instructions, repair requests and complete agent context; a shorter input can be offset by a longer receipt.

The report also removes the extreme long-line task in a sensitivity column. Heatmap stars identify incompatible file bytes. Absolute values and every failed or incompatible result remain accessible. Native CLI startup and warm editor-kernel timing have different scopes and are not ranked together.

## Reproduce native execution

Requires Python 3.10+, Node 24, a built xnip, and selected native backends. Unavailable products are recorded explicitly.

~~~sh
cargo build --locked
python benches/native/setup_aider.py
python benches/native_tools.py --repeats 3 --json target/native-bench/results.json --markdown target/native-bench/results.md
~~~

The aider setup pins official source and creates an isolated Python 3.13 environment under target/native-bench; it requires Git and uv. Existing valid environments are checked without reinstalling. Codex is discovered on PATH or in the desktop app's bundled binaries; use --codex PATH or XNIP_BENCH_CODEX to select one.

Gemini uses the installed core package or GEMINI_CORE_PATH. For a separate installation, use a private npm prefix and set the variable to its @google/gemini-cli-core directory. The recorded version is 0.27.2. OpenCode downloads SHA-verified official sources and installs locked npm dependencies only under target/native-bench on first use. No model, account or authentication is required.

For the recorded tokenizer:

~~~sh
python -m pip install --target target/benchmark-python tiktoken==0.14.0
python benches/native_tools.py --encoding o200k_base --repeats 3 --json docs/benchmarks/results.json --markdown docs/benchmarks/results.md
~~~

Counts are exact for the path-normalized strings and named encoding. They are not provider billing. A first tiktoken vocabulary load may use the network; keep resources cached for offline reruns. Without --encoding, the runner reports exact UTF-8 bytes and leaves tokens null. Every xnip child has XNIP_AUTO_UPDATE=0.

Finish application builds and integration generation before measuring. The runner refuses source or binary changes during a run, retains native source/binary fingerprints and preserves raw calls alongside normalized counts.

## Regenerate reports without rerunning editors

~~~sh
python benches/native_tools.py --report-only docs/benchmarks/results.json --markdown docs/benchmarks/results.md
python tests/native_benchmark_contract.py
~~~

Report-only needs the Python standard library. It does not load a tokenizer, invoke an editor, perform network requests, change the source JSON or replace its recorded provenance. It writes Markdown, relative-results.json and four deterministic SVG charts (overview, input ratios, scenario ratios and file correctness) beside the Markdown. The derived JSON carries the original JSON's SHA-256. PNG previews are optional local inspection artifacts.

## Supplementary measurements

~~~sh
python benches/edit_cost.py --encoding o200k_base --repeats 3 --json docs/benchmarks/representation-results.json --markdown docs/benchmarks/representation-results.md
~~~

The supplementary runner executes xnip, captures its advertised catalogs and current integration instructions, and serializes other format examples. Its third-party representation rows are not native product executions; use the primary experiment for those results.
