# Python agent SDK integrations

This local distribution provides three optional SDK entry points backed by the
existing xnip executable and its core transactions:

| Module | Framework | Integration |
|---|---|---|
| `xnip_python_agents.deepagents` | DeepAgents 0.7.34 | Filesystem backend for the original middleware tools |
| `xnip_python_agents.langgraph` | LangGraph 1.2.20 | Native file tools for a real `ToolNode` |
| `xnip_python_agents.pydantic_ai` | PydanticAI 2.66.0 | Official MCP toolset and stdio transport |

From the xnip repository, install the shared client plus the selected extras:

```sh
python -m pip install ./integrations/native-python './integrations/python-agents[deepagents,langgraph,pydantic-ai]'
```

Select one extra to install one framework. `executable` accepts an absolute xnip
path, including `xnip.exe` on Windows; it defaults to `xnip` on `PATH`. The owned
process uses the selected workspace and `XNIP_AUTO_UPDATE=0`.

See [DeepAgents](../../docs/deepagents.md), [LangGraph](../../docs/langgraph.md)
and [PydanticAI](../../docs/pydantic-ai.md) for setup, SDK scope, tool governance,
restoration and actual tool-only verification.
