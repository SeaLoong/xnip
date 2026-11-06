# xnip for CrewAI

```sh
python -m pip install ./integrations/crewai
```

Use `xnip_crewai.xnip_tools(project, executable="xnip")` as a context manager
and pass its actual CrewAI `MCPNativeTool` instances into the application's
`Agent.tools`. The public resolver owns discovery and cleanup; CrewAI starts
a fresh MCP connection for each invocation.

[Installation, filtering, lifecycle and verification](../../docs/crewai.md)
include the fixed CrewAI 1.15.36 compatibility contract and benchmark.
