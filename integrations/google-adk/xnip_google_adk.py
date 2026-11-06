"""Google ADK's public MCP toolset, configured for xnip file tools."""
from contextlib import asynccontextmanager
import os
from pathlib import Path

from google.adk.tools.mcp_tool.mcp_toolset import McpToolset
from google.adk.tools.mcp_tool.mcp_session_manager import StdioConnectionParams
from mcp import StdioServerParameters


@asynccontextmanager
async def xnip_toolset(*, cwd, executable="xnip", full=False, timeout=20, **toolset_options):
    """Yield an owned ADK toolset for the application's ``LlmAgent.tools``.

    ADK owns tool filters, confirmation, authentication, schema projection and
    result handling. The context closes its actual MCP session and subprocess.
    """
    project = str(Path(cwd).resolve())
    parameters = StdioServerParameters(
        command=str(executable),
        args=["mcp", "--cwd", project, *(["--toolset", "full"] if full else [])],
        cwd=project,
        env={**os.environ, "XNIP_AUTO_UPDATE": "0"},
    )
    toolset = McpToolset(
        connection_params=StdioConnectionParams(server_params=parameters, timeout=timeout),
        **toolset_options,
    )
    try:
        yield toolset
    finally:
        await toolset.close()
