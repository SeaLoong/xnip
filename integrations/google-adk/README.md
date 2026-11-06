# xnip for Google ADK

Install this local package with `python -m pip install ./integrations/google-adk`.
It provides the official Google ADK 2.13.0 `McpToolset` configured for an owned
xnip MCP subprocess. Python 3.11 or later is required.

```python
from google.adk.agents import LlmAgent
from xnip_google_adk import xnip_toolset

async with xnip_toolset(cwd="/absolute/project", executable="/absolute/path/to/xnip") as tools:
    agent = LlmAgent(name="project_agent", model=application_model, tools=[tools])
    # Use the application's Runner while this context remains open.
```

The application owns its model, Runner and session flow. Public ADK filtering,
prefixing and confirmation options pass through as keyword arguments. Context
exit closes the real toolset/session/subprocess.

See [installation, lifecycle, results and real-tool tests](../../docs/google-adk.md)
for the complete integration contract and benchmark boundary.
