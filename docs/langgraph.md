# LangGraph native file tools

`XnipLangGraphTools` registers file tools through LangChain's public
`StructuredTool` and LangGraph's `ToolNode` interface. Support is verified with
**LangGraph 1.2.20** and **langchain-core 1.7.6**. The framework owns graph
dispatch, schema validation, tool messages and application permission wrappers;
the existing xnip core owns matching and persistence.

## Register the tools

Use Python 3.11 or later and install the local packages alongside the built
xnip executable:

```sh
cargo build --release --locked
python -m pip install ./integrations/native-python './integrations/python-agents[langgraph]'
```

Register the returned tools in the application's existing graph:

```python
from langgraph.prebuilt import ToolNode
from xnip_python_agents.langgraph import XnipLangGraphTools

with XnipLangGraphTools(
    "/absolute/project",
    executable="/absolute/path/to/xnip",
) as files:
    tool_node = ToolNode(files.tools, handle_tool_errors=True)
    # Add tool_node to the application's graph and bind files.tools to its model.
```

Use `xnip.exe` on Windows. The context owns one native process; close it after
the graph's pending calls settle. An application can supply its ordinary
`wrap_tool_call`/`awrap_tool_call` to `ToolNode`. Those wrappers authorize calls
before the native tool executes.

| Tool | Arguments | Behavior |
|---|---|---|
| `xnip_read` | `file_path`, `max_bytes=12000`, `offset=0` | Bounded UTF-8 file text; truncation reports a byte offset for the next read; zero selects all bytes |
| `xnip_edit` | `file_path`, `old`, `text`, `replace_all=False` | Exact replacement with uniqueness or explicit global replacement |
| `xnip_write` | `file_path`, `text` | Complete-file create or replacement |

The tool names belong to this explicit registration. LangGraph supplies the
registry and executor rather than a built-in editing backend. Removing these
tools from the next graph restores the application's earlier tool list.

## Authority and persistence

Relative paths resolve against the selected workspace. Core writes default to
that workspace root; `writable_roots` can select another list, with `[]`
permitting zero writes. Reads use the process's ordinary OS authority. ToolNode
and the application own approvals and any additional read policy.

Edits retain literal UTF-8/BOM/CRLF/EOF bytes according to xnip's matching
contract. Receipts distinguish `written` from `unchanged`. Core checks prepared
snapshots again before persistence and uses its existing rollback/recovery
journal on failure. Atomic replacement and symbolic-link boundaries are in
[architecture](architecture.md#persistence-and-limits). The native protocol
accepts serialized request lines up to 32 MiB.

## Actual SDK verification

```sh
python benches/native/python_agents_setup.py
python benches/native/python_agents_setup.py --check
target/native-hosts/python-agents/.venv/bin/python tests/python_agents_contract.py
target/native-hosts/python-agents/.venv/bin/python tests/python_agents_bench_contract.py
```

The fixture compiles a real `StateGraph` with an actual `ToolNode`, executes its
registered tools, checks permission wrappers and native write policy, and runs
all 13 shared byte fixtures. Its graph has a tool node and no model node;
actual model-generation methods remain at zero calls. The local package imports
come from the prepared environment's installed distributions.

The [SDK benchmark](benchmarks/python-agents-provider-results.md) measures the
real graph tool pipeline and file outcomes. It records that pipeline directly;
an original file editor baseline belongs only to frameworks that supply one.
