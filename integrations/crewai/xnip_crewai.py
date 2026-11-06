"""CrewAI's public MCP resolver, tool classes and lifecycle for xnip."""
from contextlib import contextmanager
from pathlib import Path

from crewai.mcp import MCPServerStdio, MCPToolResolver
from crewai.utilities.logger import Logger


def _skip_cache(_arguments=None, _result=None):
    return False


def xnip_server(project, executable="xnip", *, tool_filter=None):
    """Build the official server configuration with explicit path resolution."""
    return MCPServerStdio(
        command=str(executable),
        args=["mcp", "--cwd", str(Path(project).resolve()), "--receipts", "text"],
        env={"XNIP_AUTO_UPDATE": "0"},
        tool_filter=tool_filter,
    )


@contextmanager
def xnip_tools(project, executable="xnip", *, agent=None, tool_filter=None):
    """Yield actual CrewAI BaseTools; CrewAI owns validation/events/IO cleanup.

    Public tool names use their original xnip names. CrewAI's native MCP tool
    implementation starts and closes one server connection for each invocation.
    """
    resolver = MCPToolResolver(agent=agent, logger=Logger())
    try:
        tools = resolver.resolve([xnip_server(project, executable, tool_filter=tool_filter)])
        for tool in tools:
            tool.name = tool.original_tool_name
            tool.cache_function = _skip_cache
        yield tools
    finally:
        resolver.cleanup()
