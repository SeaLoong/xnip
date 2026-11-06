"""PydanticAI v2's official MCPToolset connected to the installed xnip process."""
from __future__ import annotations

import os
from pathlib import Path

from pydantic_ai.mcp import MCPToolset, StdioTransport


def xnip_toolset(cwd, *, executable="xnip", receipts="text", **options):
    """Pass the returned toolset to Agent(toolsets=[...]) and own its context."""
    return MCPToolset(StdioTransport(command=str(executable),
        args=["mcp", "--receipts", receipts], cwd=str(Path(cwd).resolve()),
        env={**os.environ, "XNIP_AUTO_UPDATE": "0"}), **options)
