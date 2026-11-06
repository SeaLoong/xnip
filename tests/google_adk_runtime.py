"""Actual ADK toolsets and direct MCP sessions with observed real RPC outcomes."""
from __future__ import annotations

from contextlib import asynccontextmanager
import json
import os
from pathlib import Path
import sys
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "integrations/google-adk"))


def json_value(value):
    return value.model_dump(mode="json", by_alias=True, exclude_none=True) if hasattr(value, "model_dump") else value


@asynccontextmanager
async def fixture(cwd, executable, *, host="google-adk", full=False, **options):
    from google.adk.agents import LlmAgent
    from google.adk.agents.invocation_context import InvocationContext
    from google.adk.models.base_llm import BaseLlm
    from google.adk.models.llm_request import LlmRequest
    from google.adk.sessions import InMemorySessionService
    from google.adk.tools.tool_confirmation import ToolConfirmation
    from google.adk.tools.tool_context import ToolContext
    from mcp import ClientSession, StdioServerParameters
    from mcp.shared.exceptions import MCPError as McpError
    import mcp.client.stdio
    from xnip_google_adk import xnip_toolset

    class IdleModel(BaseLlm):
        calls: int = 0

        async def generate_content_async(self, *args, **kwargs):
            self.calls += 1
            raise AssertionError("ADK tool fixture reached model generation")
            yield

    model = IdleModel(model="xnip-tool-only")
    observed, processes = [], []
    original_call = ClientSession.call_tool
    original_spawn = mcp.client.stdio._create_platform_compatible_process

    async def call(session, name, arguments=None, *args, **kwargs):
        entry = {"name": name, "arguments": arguments}
        try:
            result = await original_call(session, name, arguments, *args, **kwargs)
        except McpError as error:
            entry["rpc_error"] = json_value(error.error)
            observed.append(entry)
            raise
        except Exception as error:
            entry["infrastructure_error"] = {"type": type(error).__name__, "message": str(error)}
            observed.append(entry)
            raise
        entry["result"] = json_value(result)
        observed.append(entry)
        return result

    async def spawn(*args, **kwargs):
        process = await original_spawn(*args, **kwargs)
        processes.append(process)
        return process

    with patch.object(ClientSession, "call_tool", call), patch.object(mcp.client.stdio, "_create_platform_compatible_process", spawn):
        if host == "google-adk":
            async with xnip_toolset(cwd=cwd, executable=executable, full=full, **options) as toolset:
                agent = LlmAgent(name="xnip_tools", model=model, tools=[toolset])
                assert agent.model is model
                session_service = InMemorySessionService()
                session = await session_service.create_session(app_name="xnip", user_id="tool-only", session_id="tool-only")
                invocation = InvocationContext(session_service=session_service, invocation_id="tool-only",
                                               agent=agent, session=session)
                tools = await toolset.get_tools_with_prefix()
                request = LlmRequest(model="tool-only")
                declaration_context = ToolContext(invocation, function_call_id="catalog")
                await toolset.process_llm_request(tool_context=declaration_context, llm_request=request)
                for tool in tools:
                    await tool.process_llm_request(tool_context=declaration_context, llm_request=request)

                class Runtime:
                    registry = {tool.name: tool for tool in tools}
                    schemas = {tool.name: json_value(tool.raw_mcp_tool) for tool in tools}
                    sdk_toolset = toolset
                    sdk_agent = agent
                    declarations = json_value(request.config)
                    call_id = 0

                    @property
                    def model_calls(self):
                        return model.calls

                    async def call(self, name, arguments, *, confirmation=None):
                        self.call_id += 1
                        context = ToolContext(invocation, function_call_id=f"xnip-{self.call_id}",
                            tool_confirmation=ToolConfirmation(confirmed=confirmation) if confirmation is not None else None)
                        count = len(observed)
                        try:
                            result = await self.registry[name].run_async(args=arguments, tool_context=context)
                            exception = None
                        except McpError as error:
                            result, exception = None, {"type": type(error).__name__, "message": str(error)}
                        assert model.calls == 0
                        entries = observed[count:]
                        if any("infrastructure_error" in item for item in entries):
                            raise RuntimeError("ADK MCP transport failed: " + json.dumps(entries))
                        return {"projection": json_value(result), "exception": exception,
                                "rpc": entries, "actions": json_value(context.actions)}

                runtime = Runtime()
                runtime.processes = processes
                yield runtime
                assert model.calls == 0
        elif host == "direct-mcp":
            parameters = StdioServerParameters(command=str(executable),
                args=["mcp", "--cwd", str(cwd), *(["--toolset", "full"] if full else [])],
                cwd=str(cwd), env={**os.environ, "XNIP_AUTO_UPDATE": "0"})
            async with mcp.client.stdio.stdio_client(parameters) as streams:
                async with ClientSession(*streams) as session:
                    await session.initialize()
                    catalog = await session.list_tools()

                    class Runtime:
                        schemas = {tool.name: json_value(tool) for tool in catalog.tools}

                        @property
                        def model_calls(self):
                            return model.calls

                        async def call(self, name, arguments, **kwargs):
                            count = len(observed)
                            try:
                                result = await session.call_tool(name, arguments)
                                exception = None
                            except McpError as error:
                                result, exception = None, {"type": type(error).__name__, "message": str(error)}
                            entries = observed[count:]
                            if any("infrastructure_error" in item for item in entries):
                                raise RuntimeError("Direct MCP transport failed")
                            return {"projection": json_value(result), "exception": exception,
                                    "rpc": entries, "actions": {}}

                    runtime = Runtime()
                    runtime.processes = processes
                    yield runtime
        else:
            raise ValueError(host)
    assert processes, "Actual MCP subprocess must start"
    assert all(process.returncode == 0 for process in processes), "SDK closure must settle its owned native process"
