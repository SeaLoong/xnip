"""The official AutoGen MCP workbench configured for xnip."""

import os
from pathlib import Path

from autogen_ext.tools.mcp import McpWorkbench, StdioServerParams


def xnip_workbench(*, cwd, executable="xnip", full=False, **workbench_options):
    """Create an owned workbench for ``async with`` and AssistantAgent.

    AutoGen owns tool discovery, validation, cancellation and result projection.
    The application chooses its model client and approval policy as usual.
    """
    return McpWorkbench(
        StdioServerParams(
            command=str(executable),
            args=["mcp", *(["--toolset", "full"] if full else [])],
            cwd=str(Path(cwd).resolve()),
            env={**os.environ, "XNIP_AUTO_UPDATE": "0"},
        ),
        **workbench_options,
    )
