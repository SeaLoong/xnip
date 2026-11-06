# CrewAI

The local `xnip-crewai` package supplies actual CrewAI `MCPNativeTool`
instances through its public `MCPToolResolver`. CrewAI owns tool schemas,
argument validation, events, execution, retry policy and result projection.
The owned stdio MCP process applies edits through xnip's existing Rust engine.

Verified versions are **CrewAI 1.15.36 and MCP 1.30.0**, tested on Python
**3.12 and 3.13**. CrewAI's declared Python range is 3.10–3.13; the frozen test
environment uses Python 3.12–3.13. CrewAI includes its MCP dependency. This entry point
serves applications using CrewAI's public Python SDK.

## Install and attach

```sh
cargo build --release --locked
python -m pip install ./integrations/crewai
```

```python
from crewai import Agent
from xnip_crewai import xnip_tools

with xnip_tools(
    project="/absolute/project",
    executable="/absolute/path/to/xnip",
) as tools:
    agent = Agent(
        role="Project editor",
        goal="Complete the application's project task",
        backstory="Uses the application's registered tools",
        llm=application_model,
        tools=[*application_tools, *tools],
    )
    # Keep the application's Crew/Task execution inside this context.
```

The application supplies its model, Crew, Tasks and existing tools. Keep the
context open for their tool usage. On Windows use an absolute `xnip.exe`
path; `executable` otherwise defaults to `xnip` on `PATH`.

The child receives `xnip mcp --cwd /absolute/project --receipts text` and
`XNIP_AUTO_UPDATE=0`. Its catalog contains `xnip_edit`, `xnip_apply`,
`xnip_peek`, `xnip_find` and `xnip_recover`. `--cwd` selects the directory
used to resolve relative paths. Operating-system process permissions define
filesystem authority.

## Public tool policy and lifecycle

Each tool keeps its original xnip name in CrewAI's registry. The factory
sets CrewAI's public `cache_function` to return false: file reads observe
current bytes, and repeated mutations pass through the actual tool. CrewAI's
structured-tool conversion carries that public cache policy forward.

CrewAI's native MCP implementation starts and closes a fresh connection for
each invocation. Initial discovery is a separate connection. This lifecycle
and the SDK's retry policy remain owned by CrewAI. The factory's context
cleans up its public resolver when application tool usage finishes.

Use CrewAI's public filters to select tools:

```python
from crewai.mcp import create_static_tool_filter

selected = create_static_tool_filter(
    allowed_tool_names=["xnip_peek", "xnip_edit"],
)
with xnip_tools(project="/absolute/project", tool_filter=selected) as tools:
    agent = Agent(
        role="Project editor",
        goal="Update the selected files",
        backstory="Uses the selected project tools",
        llm=application_model,
        tools=[*application_tools, *tools],
    )
```

`xnip_server()` also exposes the official `MCPServerStdio` configuration for
applications that manage their own resolver. The optional `agent=` argument
to `xnip_tools()` passes the application agent into `MCPToolResolver`.
Application approval and tool-selection rules belong to its normal CrewAI
execution flow.

To remove the integration, finish the context and construct the next agent
with the application's earlier tools list. Uninstall the local package with
`python -m pip uninstall xnip-crewai`.

## Results and verification

CrewAI flattens successful MCP text content into its public tool result:
successful edits return `written`, and preview receipts retain
`written=false`. The raw MCP result still includes the underlying structured
receipt. Remote JSON-RPC errors pass through CrewAI's actual exception and
retry handling. Applications consume that public SDK result or exception.

```sh
python benches/native/crewai_setup.py
python benches/native/crewai_setup.py --check
target/native-hosts/crewai-1.15.36/.venv/bin/python tests/crewai_contract.py
target/native-hosts/crewai-1.15.36/.venv/bin/python tests/crewai_provider_bench_contract.py
```

Setup creates an isolated environment under `target`, installs platform-universal
hash-locked dependencies, verifies the installed versions and records hashes
of CrewAI's Python sources. `--check` verifies the existing environment and
receipt. Setup uses the invoking interpreter; use Python 3.12–3.13 for a
fresh cache. SDK dependencies include CrewAI's standard storage and model
packages; tests use local tool execution and a dedicated `target` storage
directory with test telemetry disabled.

The **18 E2E checks** cover the common 13 byte fixtures, genuine
`MCPNativeTool` instances registered in an actual `Agent`, schemas and public
cache policy, preview/read/find/create/rename/delete, specific stale and
ambiguous remote errors, official filtering, fresh reads, connection closure
and a locally installed package. An actual custom `BaseLLM` counts and
rejects attempted generation; every check records zero model calls. Tests
observe the real spawned processes and require each to finish successfully.
Application-driven agent generation remains part of its Crew/Task execution.

The independent benchmark contract executes both providers for seven
scenarios and verifies that altered file goals, missing actual RPC evidence
and startup failures cause validation to fail.

## Benchmark

[The CrewAI benchmark](benchmarks/crewai-provider-results.md) compares actual
public `BaseTool.run()` with direct `ClientSession` calls using the same
binary, MCP arguments and independent file goals. Both providers start and
close a fresh MCP session for each tool call, matching CrewAI's native
lifecycle. Discovery and Agent registration have separate setup timings.

The suite has **26 fixtures and 52 provider/fixture groups**: the common 13,
a no-op, stale/ambiguous guards, creation/deletion/rename, a late batch
failure, BOM/CRLF and mixed line endings, preview, and 64 KiB, 1 MiB and
5 MiB files.

```sh
target/native-hosts/crewai-1.15.36/.venv/bin/python benches/crewai_provider.py \
  --executable target/release/xnip --repeats 3 --warmups 1
```

Every measured call crosses real stdio. An observer delegates the original
`ClientSession.call_tool` and retains the actual cleaned arguments, raw MCP
result or typed remote error. Successful reads precede requested mutations.
Guard cases require their specific remote code and message. CrewAI may
retry ambiguous edits; all attempts are recorded, and SDK backoff remains
inside timing. Process and transport failures fail sampling. The final
actual RPC outcome and complete file bytes must agree between providers.

Timing includes complete public calls, process creation, initialization,
argument/result processing, retries and closure. Fixture reset, payload
accounting and byte validation run outside it. The JSON retains projected
text, underlying results, errors, each attempt, file goals/hashes, setup,
per-call timing and binary/source/SDK hashes. Payload counts describe local
serialized JSON layers. Model call counts stay at zero; model latency and
billing belong to application-level measurements.

Official references: [CrewAI 1.15.36 on PyPI](https://pypi.org/project/crewai/1.15.36/)
and [CrewAI's repository](https://github.com/crewAIInc/crewAI).
