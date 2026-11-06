# Integration and editing benchmarks

Start with the [support matrix](../support.md): it maps current catalog entries
to their tool surfaces, contracts and reports. Benchmarks execute real host
tools, plugins, framework SDKs and MCP clients with model generation idle.
Each cohort records its own API, storage and timing boundary.

## Current integration reports

Ten current cohorts contain 1,878 measured workflows with zero model calls.
The recorded groups use one warm-up and three measured workflows. Reports and
their JSON companions retain every sample, argument/result byte counts and
hashes, captured payloads where recorded, file oracles, package versions,
source fingerprints and executable identity. Each report is a snapshot of its
recorded source and executable; release builds have their own identities.

| Report | Fixtures | Groups / measured workflows | Executed boundary |
|---|---:|---:|---|
| [OpenCode](opencode-provider-results.md) · [JSON](opencode-provider-results.json) | 23 | 46 / 138 | Actual CLI builtin and plugin tools, including startup and disposal |
| [OpenHands SDK](openhands-provider-results.md) · [JSON](openhands-provider-results.json) | 23 | 46 / 138 | Original `file_editor` versus xnip IO through `Conversation.execute_tool` |
| [aider](aider-provider-results.md) · [JSON](aider-provider-results.json) | 24 | 142 / 426 | Original Coder `diff` / `whole` / `udiff` pipelines and native persistence |
| [DeepAgents / LangGraph / PydanticAI](python-agents-provider-results.md) · [JSON](python-agents-provider-results.json) | 23 | 92 / 276 | DeepAgents backend pair, registered ToolNode tools and official MCPToolset |
| [OpenAI Agents SDK](agents-sdk-provider-results.md) · [JSON](agents-sdk-provider-results.json) | 26 | 52 / 156 | Direct MCP reference and actual FunctionTool invocation |
| [AutoGen](autogen-provider-results.md) · [JSON](autogen-provider-results.json) | 26 | 52 / 156 | Direct MCP reference and official McpWorkbench |
| [smolagents](smolagents-provider-results.md) · [JSON](smolagents-provider-results.json) | 26 | 52 / 156 | Direct MCP reference and ToolCallingAgent tool execution |
| [Google ADK](google-adk-provider-results.md) · [JSON](google-adk-provider-results.json) | 26 | 52 / 156 | ClientSession reference and actual McpToolset / ToolContext |
| [CrewAI](crewai-provider-results.md) · [JSON](crewai-provider-results.json) | 26 | 52 / 156 | ClientSession reference and official MCP resolver / BaseTools |
| [Client profiles / Goose](host-profiles-results.md) · [JSON](host-profiles-results.json) | 20 | 40 / 120 | Fourteen configuration profiles, shared stdio and the official Goose client |

The [OpenCode plugin guide](../opencode.md) and
[CLI benchmark runner](../../benches/opencode_provider.mjs) identify its four
additional xnip tools and actual CLI execution surface.
Its recorded CLI debug entry includes startup, complete stdout capture and
decoding, with formatters and LSP disabled in both lanes. Original fuzzy/EOL
editing and the plugin's exact matching retain their named contracts.

Provider comparisons use a real original editor where the host supplies one.
LangGraph tools and MCP SDK cohorts measure their registered tool/transport
pipeline. SDK and direct-MCP projections remain separate returned layers.
Client-profile timings describe shared stdio; Goose has an executed official
client lane. Actual Codex, Claude Code and Copilot CLI configuration checks
are recorded separately from GUI application behavior.

## Byte goals and host behavior

Each sample checks the complete expected file set, exact bytes or absence,
the SDK receipt and the specific error for a rejected operation. A host's
format/newline policy and the independent literal-byte goal are separate:

| Cohort | Host-policy / SDK-compatible workflows | Literal byte-goal workflows | Recorded difference |
|---|---:|---:|---|
| OpenHands | 138 / 138 | 120 / 138 | CRLF, BOM+CRLF and mixed-EOL normalization |
| aider | 426 / 426 | 390 / 426 | CRLF and final-newline representation across three formats |
| Python frameworks | 276 / 276 | 258 / 276 | DeepAgents normalizes three newline cases; LangGraph and PydanticAI retain literal goals |

The original 13 shared fixtures retain their supplied inputs. Each format or
SDK route declares its own matching, permission and representation policy.
Warm-ups use the same correctness gates as measured workflows.

## Timing and byte accounting

Read, mutation and setup fields retain their named scopes. `setup_ms` describes
runtime/session construction. In the DeepAgents/LangGraph/PydanticAI cohort,
Python interpreter and module imports happen before that timer. CrewAI and
client-profile workflows include a fresh MCP connection and graceful closure;
CrewAI's actual SDK retries and backoff remain timed. Each report states its
session and bootstrap policy.

| Counter | Captured data |
|---|---|
| Public arguments | Actual tool-call object or supplied Coder reply |
| SDK projection | Actual ToolMessage content, tool result or returned SDK value |
| Exception bytes | Actual thrown error text, kept separate from returned content |
| MCP frames | Measured request/response bytes where the client observes them |
| Native JSONL | Actual child stdin/stdout for adapters that own that connection |

These layers retain their own byte counts. An MCP frame contains arguments
already counted at the tool boundary; SDK projection and raw RPC results are
separate representations. Model calls stay at zero, and reports make claims
at their recorded local execution boundary.

Three measured samples yield a median and nearest-rank p95; p95 is the maximum
of those three samples. Machine, interpreter, SDK, source and binary provenance
accompany each cohort. Compare original and native lanes within the declared
host policy and lifecycle.

## Reproduce a current cohort

Prepare the matching isolated environment using
[host/framework setup](../testing.md#native-host-e2e), finish the release build,
then run measurements sequentially. Each report contains its exact invocation.
For example:

```sh
cargo build --release --locked
target/native-hosts/python-agents/.venv/bin/python benches/python_agents_provider.py \
  --executable target/release/xnip --repeats 3 --warmups 1 \
  --output-prefix target/benchmarks/python-agents
```

On Windows use the environment's `Scripts/python.exe` and `xnip.exe`.
`--output-prefix` selects fresh local artifacts; `--scenarios` selects a named
fixture subset. SDK families use their own virtual environments and locked MCP
versions. The test guide lists actual measured-runner contracts alongside
the full 369-check execution matrix.

## Earlier provider snapshots

These records keep their original implementation versions and fingerprints:

| Report | Fixtures | Groups / measured workflows | Boundary |
|---|---:|---:|---|
| [DSH rc.2 / rc.5](native-provider-results.md) · [JSON](native-provider-results.json) | 32 | 128 / 384 | Official filesystem tools, observation/sandbox policy and native IPC |
| [Pi 1.3.0](pi-provider-results.md) · [JSON](pi-provider-results.json) | 20 | 40 / 120 | Original SDK tools, mutation queue, hooks and public JSON |
| [Gemini core 0.27.2](gemini-provider-results.md) · [JSON](gemini-provider-results.json) | 30 | 60 / 180 | Local Config service, original tool invocations and native IPC |
| [Cline SDK/core 0.2.0](cline-provider-results.md) · [JSON](cline-provider-results.json) | 31 | 62 / 186 | Official patch parser, SDK dispatch and operation output |

Gemini's two lanes match SDK results and storage; its independent literal goal
holds for 174 of 180 workflows because both lanes normalize mixed EOL.
Cline's large-file cases use the official EOF hint in both lanes, alongside an
unhinted 64 KiB pressure case. Its [matcher probe](cline-matcher-probe.json) and
[partial diagnostic](cline-provider-partial-diagnostic.json) retain component
and partial-run evidence separately from the complete SDK matrix.

The [DSH encoding control](native-provider-encoding.md) compares byte arrays and
UTF-8 with one fixed release binary, source and tool flow. It isolates that
bridge representation choice; provider reports retain the original host's
local execution cost beside xnip's IO and transaction work.

## Historical editing-executor archive

The [archive index](archive/README.md) retains the original **13-task / 41-profile**
editing-executor run, six host-MCP exchange profiles, supplementary
representation measurements and core-performance snapshots. Its raw JSON,
frontier statistics and charts retain the recorded cohorts. Current plugin,
SDK and client-profile evidence has the named boundaries above.

The [historical methodology](native-methodology.md) documents input selection,
guards, probes and accounting. To render the stored run into fresh local files:

```sh
python benches/native_tools.py --report-only docs/benchmarks/results.json \
  --markdown target/native-bench/report-only/results.md
```

Report-only uses the Python standard library and the captured raw run.
