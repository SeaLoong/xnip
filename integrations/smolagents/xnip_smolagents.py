"""Connect smolagents' public MCP tool collection to the xnip editing process."""

from __future__ import annotations

from contextlib import contextmanager
import os
from pathlib import Path

from mcp import StdioServerParameters
from mcpadapt.core import MCPAdapt
from mcpadapt.smolagents_adapter import SmolAgentsAdapter
from smolagents import ToolCollection


class XnipToolAdapter(SmolAgentsAdapter):
    """Translate MCP required/error metadata into smolagents' public tool model."""

    def adapt(self, invoke, definition):
        if "properties" not in definition.inputSchema:
            definition = definition.model_copy(update={
                "inputSchema": {**definition.inputSchema, "properties": {}},
            })

        def call(arguments):
            result = invoke(arguments)
            if result.isError:
                message = "\n".join(item.text for item in result.content if hasattr(item, "text"))
                raise RuntimeError(message)
            return result

        tool = super().adapt(call, definition)
        tool.structured_output = self.structured_output and definition.outputSchema is not None
        required = definition.inputSchema.get("required", [])
        for name, field in tool.inputs.items():
            # smolagents uses nullable to mark an argument that can be omitted.
            field["nullable"] = name not in required
        return tool


@contextmanager
def xnip_tools(executable="xnip", cwd=None, *, full=False):
    """Yield SDK tools with structured xnip receipts and an owned MCP session.

    Supply the collection's tools to the application's usual CodeAgent or
    ToolCallingAgent. Closing this context closes the SDK's stdio session.
    """
    parameters = StdioServerParameters(
        command=str(executable),
        args=["mcp", "--receipts", "structured", *(["--toolset", "full"] if full else [])],
        cwd=str(Path(cwd or Path.cwd()).resolve()),
        env={**os.environ, "XNIP_AUTO_UPDATE": "0"},
    )
    connection = MCPAdapt(parameters, XnipToolAdapter(structured_output=True))
    try:
        connection.start()
        yield ToolCollection(connection.tools())
    finally:
        connection.close()
