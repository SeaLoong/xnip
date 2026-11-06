"""The official Agents SDK MCP server, configured for xnip editing tools."""

import os
from pathlib import Path

from agents.mcp import MCPServerStdio


def xnip_server(*, cwd, executable="xnip", full=False, **server_options):
    """Return an owned SDK stdio server for use with ``async with``.

    Pass this server to the application's existing Agent.mcp_servers. SDK tool
    filters, approvals, guardrails, tracing and result projection remain SDK
    options. Closing the context drains the server's native subprocess.
    """
    options = {"name": "xnip", "cache_tools_list": True, "use_structured_content": True,
               **server_options}
    return MCPServerStdio(
        params={"command": str(executable),
                "args": ["mcp", *(["--toolset", "full"] if full else [])],
                "cwd": str(Path(cwd).resolve()),
                "env": {**os.environ, "XNIP_AUTO_UPDATE": "0"}},
        **options,
    )
