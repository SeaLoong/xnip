# Native editor execution methodology

## Question and two different experiments

The primary experiment asks: for the same intended edit, how do a harness's native editor and xnip differ in accepted input, actual file bytes, matching/rejection behavior, returned text and local execution time?

It directly executes native tools. The separate representation experiment is available as [supplementary data](representation-results.md). Serializing an aider block or applying a Codex-looking patch through xnip cannot establish the behavior of aider or Codex.

A full agent experiment asks a different question: with the same harness, model, prompt and initial repository, what changes when the native editor is replaced by xnip? That needs real model trajectories, tool selection, provider token usage, mandatory reads, retries and repeated runs. It has not been run here. No claim about total agent token savings follows from these executor measurements.

## Native implementations and surrounding context

Codex is the actual installed executable, fingerprinted by SHA256 and queried for its version. Its native argv0 dispatcher consumes the unchanged patch on stdin. This avoids Windows argv size limits without reproducing the parser. Native stdout, stderr and status are retained.

Gemini CLI is the installed core package. The adapter invokes the unchanged EditTool and its real StandardFileSystemService. Workspace authorization is restricted to the temporary fixture, telemetry is disabled, and developer console output is captured separately. The default repair path stays enabled, but its generateJson request is intercepted and recorded without inference or network access. A blocked repair means the complete repair outcome is unmeasured. The exact native request size is available as an observation, not billed usage.

OpenCode is pinned to official commit `e3c4eb242585dcd77be1a867391b7c25fd47ce52` / 1.19.3. Matching, parameter validation, diff generation, output truncation and filesystem operations use SHA-verified upstream sources. LSP, formatting, permission UI, events and agent lookup are isolated context adapters. These choices are included in provenance; they do not create a full OpenCode session.

aider is pinned to official commit `5dc9490bb35f9729ef2c95d00a19ccd30c26339c`. Its actual parser, edit application and InputOutput perform the reads/writes. Model orchestration, git commits, edit authorization, linting and reflection are outside the editing-kernel call. Consequently the final model-visible response is unknown; the human report marks that response cost as not captured. Native exceptions, IO output and internal returns are retained.

## Same tasks, different accepted inputs

Thirteen deterministic tasks cover small and large edits, multiline code, Unicode, CRLF, missing EOF newline, disambiguated repeated text, global replacement, same/multiple-file changes, a long line, insertion and deletion. Each backend and repetition receives its own fresh workspace. Every file, including unexpected side effects, is included in the byte oracle.

These fixtures modify existing UTF-8 text files. They are not a comprehensive comparison of file lifecycle, sandbox policy, concurrency safety or language correctness.

Inputs are generated deterministically from each task's intended result. Patches use changed lines and three context lines; their execution uses the actual native patch executors. Aider diff/udiff/whole use their own native formats. The whole-file message uses LF line breaks and canonical closed fences; native InputOutput handles its configured file line endings.

Gemini has two explicit profiles: prescribed three-line context, and an executor-accepted minimal literal. The minimal profile does not represent compliance with Gemini's full prompting rules. The tasks supply the same semantic instruction; no model generates the arguments.

Gemini and OpenCode prompts require a prior Read. This experiment assumes the edit context is already supplied and does not run the full Read/prompt workflow. Its token counts must not be described as whole-agent context totals.

The current host is Windows. Aider `platform` therefore writes platform newlines; `lf` is a separate explicit native option. Byte-exact and LF-normalized checks are both reported. LF normalization does not erase missing-EOF differences.

Aider WholeFileCoder also accepts an unclosed final fence, which can preserve missing EOF. That alternate accepted encoding is tested separately. A closed-fence generator limitation must not be attributed to every possible native whole-file invocation.

## Independent observations

A native success status is compared with expected final bytes; the status alone never passes the oracle. Per-repetition file hashes and status are compared for determinism. Failures and nonexact outcomes remain in the report.

Separate probes supply ambiguous, stale, whitespace-drift and overlapping targets to observe each interface's matching and repair policy. Some representations cannot express an overlapping substring precondition; those combinations are omitted rather than given a fabricated result. These probes expose contract differences, not automatic product bug rankings.

## Accounting and timing

Original captured calls and normalized copies are both stored. Counts use the normalized `/workspace` paths so random temporary directory names do not affect comparisons.

Input counts are the actual native patch/body/argument text. Returned-text counts are captured native output or errors. UI-rich objects and model-facing fields are distinguished. Adapter-return size includes instrumentation and duplicated fields; it is debug data, not agent context.

The optional tiktoken encoding is named in every report. Bytes-only runs leave tokens null. Actual provider usage, hidden prompts, chat framing, caching and generated model reasoning are unmeasured.

Warm MCP/Node/kernel calls exclude worker initialization. Codex CLI timing includes native process startup. Aider IO setup and import timing are retained separately. These scopes differ and are not ranked as a universal speed comparison.

All xnip child processes set `XNIP_AUTO_UPDATE=0`. Native adapters never invoke a model. Gemini and aider block model/network activity during execution. OpenCode dependency/source preparation can use the network before timing; actual editing has no model context.


## Paired relative statistics and charts

The [derived JSON](relative-results.json) and four SVG charts—[overview](charts/overview.svg), input ratios, scenario ratios and file correctness—are generated by [native_report.py](../../benches/native_report.py) from the raw native results. Report-only regeneration never rewrites raw transcripts or their measured source fingerprints. The derived file records the original JSON SHA-256.

For task i, input ratio r_i = native_input_i / xnip_input_i under the run's selected counter. xnip is 1x. The aggregate is exp(sum(log(r_i)) / N), with one equal vote per task. Neither token volume, call count nor the number of deterministic repetitions increases a task's weight. A ratio below 1x means the native representation is smaller on that task.

Two cohorts are reported. All-task ratios describe supplied input payloads, including native outputs with different final bytes. Exact-pair ratios require both tools to match the expected file bytes in every repetition. Exact subsets can have different members and sizes; they are identified in JSON and cannot support a common-corpus product ranking. The report publishes the intersection across all profiles explicitly.

The extreme long-line task also has a leave-one-out sensitivity column. The geometric-mean chart uses a logarithmic x axis, the task heatmap uses a logarithmic color scale, and a star identifies byte-incompatible native output. Absolute counts remain available. The report separately computes returned-text and input-plus-captured-return paired ratios for the known captured text boundaries. The combined numerator and denominator each sum independently counted input and returned strings before taking the task ratio. Aider's final model-facing response remains unknown and is excluded; it is never converted to zero. A genuinely captured empty string is zero, while a zero returned-text denominator yields an undefined return-only ratio. Schemas, initialization, model-repair requests and host framing are excluded. These observable-string ratios are not a full-agent token or universal latency ranking.

## Unavailable products

Claude Code and Cursor are explicitly unmeasured in this run. No invented schema, approximate replacement function or reused xnip parser is allowed to fill those rows. A future authenticated full-agent run should retain its installed version and native tool traces before making claims about those products.
