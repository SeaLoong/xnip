# PydanticAI MCP tools

The local adapter constructs PydanticAI **2.66.0**'s official
`MCPToolset(StdioTransport(...))` with the installed xnip executable. The agent
keeps PydanticAI's tool discovery, metadata, process callbacks, error handling
and async session lifecycle. Editing uses the same Rust MCP server as other
xnip clients.

## Connect the toolset

Use Python 3.11 or later, build xnip, then install the two local distributions:

```sh
cargo build --release --locked
python -m pip install ./integrations/native-python './integrations/python-agents[pydantic-ai]'
```

Pass the toolset to an application-owned agent:

```python
from pydantic_ai import Agent
from xnip_python_agents.pydantic_ai import xnip_toolset

tools = xnip_toolset(
    "/absolute/project",
    executable="/absolute/path/to/xnip",
)
agent = Agent(model=model, toolsets=[tools])

async def run_request(prompt):
    async with tools:
        return await agent.run(prompt)
```

`model` and request handling belong to the application. On Windows select an
absolute `xnip.exe` path. The toolset starts a stdio server in the given
workspace with `XNIP_AUTO_UPDATE=0` and text receipts. `receipts="structured"`
selects structured MCP receipts. Extra keyword options go to the public
`MCPToolset` constructor, including `process_tool_call` for the application's
permission and audit callback.

The toolset discovers `xnip_apply`, `xnip_edit`, `xnip_find`, `xnip_peek` and
`xnip_recover`. Their schemas and editing behavior are the existing
[MCP contracts](mcp.md). Tools supplement the application's other registrations.
Omit the toolset from the next agent to restore the earlier tool list; exiting
its async context closes the owned stdio session.

## SDK and filesystem boundaries

Tool calls use the process's normal filesystem authority and selected working
directory. PydanticAI owns process callbacks, retries and result projection;
xnip owns exact matching, bounded reads, atomic per-file persistence and
recovery. The [architecture](architecture.md#persistence-and-limits) describes
filesystem identity and concurrent-writer boundaries.

The prepared environment pins MCP **2.3.0** with the current PydanticAI transport.
It has a separate dependency lock from integrations using the MCP 1.x Python
client. Applications can install the selected extra in their own environment.

## Verification and benchmark

```sh
python benches/native/python_agents_setup.py
python benches/native/python_agents_setup.py --check
target/native-hosts/python-agents/.venv/bin/python tests/python_agents_contract.py
target/native-hosts/python-agents/.venv/bin/python tests/python_agents_bench_contract.py
```

The fixture constructs a real `Agent`, discovers tools through the actual
`MCPToolset`, and calls them through its public `call_tool` method and a real
stdio server. It verifies schemas, process callbacks, preview, guards, file
creation/rename/delete, session shutdown and all 13 shared byte fixtures.
Actual model methods are trapped and record zero generations.

The [Python SDK benchmark](benchmarks/python-agents-provider-results.md) measures
this supported tool pipeline, including session construction, warm calls, actual SDK
argument/result payloads and independent final-file hashes. It reports the SDK
MCP boundary directly rather than assigning it an unrelated built-in editor.
