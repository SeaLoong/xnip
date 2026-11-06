"""Actual SDK tool pipelines shared by contracts and the MCP bridge benchmark."""

from contextlib import asynccontextmanager, contextmanager
import json
import os
from pathlib import Path
import sys
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "integrations/agents-sdk"))
sys.path.insert(0, str(ROOT / "integrations/autogen"))


def json_value(value):
    if hasattr(value, "model_dump"):
        return value.model_dump(mode="json")
    return value


@asynccontextmanager
async def fixture(host, cwd, executable, *, full=False, **options):
    with observed_mcp_processes() as processes:
        async with _fixture(host, cwd, executable, full=full, **options) as runtime:
            runtime.processes = processes
            yield runtime


@contextmanager
def observed_mcp_processes():
    import mcp.client.stdio
    processes = []
    original_spawn = mcp.client.stdio._create_platform_compatible_process

    async def spawn(*args, **kwargs):
        process = await original_spawn(*args, **kwargs)
        processes.append(process)
        return process

    with patch.object(mcp.client.stdio, "_create_platform_compatible_process", spawn):
        yield processes
        assert processes, "Actual SDK stdio must start its MCP subprocess"
        assert all(process.returncode is not None for process in processes), "SDK context must settle every owned MCP subprocess"


@asynccontextmanager
async def _fixture(host, cwd, executable, *, full=False, **options):
    if host == "agents-sdk":
        from agents import Agent, RunContextWrapper, set_tracing_disabled
        from agents.models.interface import Model
        from mcp import McpError
        from agents.tool_context import ToolContext
        from xnip_agents import xnip_server

        class IdleModel(Model):
            calls = 0

            async def get_response(self, *args, **kwargs):
                self.calls += 1
                raise AssertionError("SDK tool execution reached model generation")

            async def stream_response(self, *args, **kwargs):
                self.calls += 1
                raise AssertionError("SDK tool execution reached model streaming")
                yield  # Makes this the SDK's required async-iterator interface.

        set_tracing_disabled(True)  # This isolated tool fixture has no trace exporter.
        observations = []

        def observe(result):
            observations.append({"is_error": result.is_error,
                                 "receipt": dict(result.structured_content) if result.structured_content else None})
            return None

        options = {"failure_error_function": None, **options}
        async with xnip_server(cwd=cwd, executable=executable, full=full,
                               custom_data_extractor=observe, **options) as server:
            model = IdleModel()
            agent = Agent(name="xnip-tools", model=model, mcp_servers=[server])
            tools = await agent.get_all_tools(RunContextWrapper(context=None))

            class Runtime:
                schemas = {tool.name: tool.params_json_schema for tool in tools}
                registry = {tool.name: tool for tool in tools}
                call_id = 0
                sdk_agent = agent
                sdk_server = server

                @property
                def model_calls(self):
                    return model.calls

                async def call(self, name, arguments):
                    self.call_id += 1
                    payload = json.dumps(arguments, ensure_ascii=False)
                    context = ToolContext(context=None, tool_name=name,
                        tool_call_id=f"xnip-{self.call_id}", tool_arguments=payload, agent=agent)
                    count = len(observations)
                    try:
                        result = await self.registry[name].on_invoke_tool(context, payload)
                    except McpError as error:
                        assert model.calls == 0
                        return {"projection": None, "is_error": True, "receipt": None,
                                "exception": {"type": type(error).__name__, "message": str(error)}}
                    assert len(observations) == count + 1
                    assert model.calls == 0
                    return {"projection": json_value(result), **observations[-1]}

            runtime = Runtime()
            yield runtime
            assert runtime.model_calls == 0
    elif host == "autogen":
        from autogen_agentchat.agents import AssistantAgent
        from autogen_ext.models.replay import ReplayChatCompletionClient
        from xnip_autogen import xnip_workbench

        class IdleModel(ReplayChatCompletionClient):
            calls = 0

            async def create(self, *args, **kwargs):
                self.calls += 1
                raise AssertionError("SDK workbench reached model generation")

            async def create_stream(self, *args, **kwargs):
                self.calls += 1
                raise AssertionError("SDK workbench reached model streaming")
                yield

        model = IdleModel([])
        async with xnip_workbench(cwd=cwd, executable=executable, full=full, **options) as workbench:
            agent = AssistantAgent("xnip_tools", model_client=model, workbench=workbench)
            tools = await workbench.list_tools()

            class Runtime:
                schemas = {tool["name"]: tool["parameters"] for tool in tools}
                registry = {tool["name"]: tool for tool in tools}
                sdk_agent = agent
                sdk_workbench = workbench

                @property
                def model_calls(self):
                    return model.calls

                async def call(self, name, arguments):
                    result = await workbench.call_tool(name, arguments)
                    assert model.calls == 0
                    return {"projection": result.model_dump(mode="json"),
                            "is_error": result.is_error, "receipt": None}

            runtime = Runtime()
            yield runtime
            assert runtime.model_calls == 0
        await model.close()
    else:
        raise ValueError(host)
