"""Real CrewAI resolver, Agent registration and BaseTool execution."""
from __future__ import annotations

from contextlib import contextmanager, redirect_stdout
import io
import json
import os
from pathlib import Path
import sys
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
os.environ["CREWAI_TELEMETRY_ENABLED"] = "false"
os.environ["OTEL_SDK_DISABLED"] = "true"
os.environ["CREWAI_STORAGE_DIR"] = str(ROOT / "target/native-hosts/crewai-storage")
sys.path.insert(0, str(ROOT / "integrations/crewai"))

from crewai import Agent
from crewai.llms.base_llm import BaseLLM
from crewai.mcp.transports.stdio import StdioTransport
from mcp import ClientSession
import mcp.client.stdio
from mcp.shared.exceptions import McpError
from xnip_crewai import xnip_tools


class IdleLLM(BaseLLM):
    calls: int = 0

    def call(self, *args, **kwargs):
        self.calls += 1
        raise AssertionError("CrewAI tool tests entered model generation")

    async def acall(self, *args, **kwargs):
        self.calls += 1
        raise AssertionError("CrewAI tool tests entered async model generation")


class CrewFixture:
    def __init__(self, cwd, executable, tool_filter=None):
        self.diagnostics = io.StringIO()
        self.calls = []
        self.transports = []
        self.processes = []
        self.model = IdleLLM(model="xnip-tool-only")
        call_tool = ClientSession.call_tool
        connect = StdioTransport.connect
        spawn = mcp.client.stdio._create_platform_compatible_process

        async def trace_session(session, name, arguments=None, *args, **kwargs):
            record = {"name": name, "arguments": dict(arguments or {})}
            self.calls.append(record)
            try:
                reply = await call_tool(session, name, arguments, *args, **kwargs)
                record["result"] = reply.model_dump(mode="json", exclude_none=True)
                return reply
            except McpError as error:
                record["rpc_error"] = error.error.model_dump(mode="json", exclude_none=True)
                raise
            except Exception as error:
                record["infrastructure_error"] = {"type": type(error).__name__, "message": str(error)}
                raise

        async def trace_connect(transport, *args, **kwargs):
            self.transports.append(transport)
            return await connect(transport, *args, **kwargs)

        async def trace_spawn(*args, **kwargs):
            process = await spawn(*args, **kwargs)
            self.processes.append(process)
            return process

        self.trace = patch.object(ClientSession, "call_tool", trace_session)
        self.transport_trace = patch.object(StdioTransport, "connect", trace_connect)
        self.process_trace = patch.object(mcp.client.stdio, "_create_platform_compatible_process", trace_spawn)
        self.trace.start(); self.transport_trace.start()
        self.process_trace.start()
        self.owner = xnip_tools(cwd, executable, tool_filter=tool_filter)
        try:
            with redirect_stdout(self.diagnostics):
                tools = self.owner.__enter__()
                self.agent = Agent(role="Local editor", goal="Use registered file tools", backstory="SDK tool contract",
                                   tools=tools, llm=self.model, verbose=False)
            self.tools = {tool.original_tool_name: tool for tool in self.agent.tools}
        except Exception:
            self.trace.stop(); self.transport_trace.stop()
            self.process_trace.stop()
            raise

    def call(self, name, arguments):
        before = len(self.calls)
        with redirect_stdout(self.diagnostics):
            value = self.tools[name].run(**arguments)
        assert self.model.calls == 0
        return value, self.calls[before:]

    def close(self):
        try:
            with redirect_stdout(self.diagnostics):
                self.owner.__exit__(None, None, None)
            assert self.model.calls == 0
            assert all(not item.connected for item in self.transports)
            assert self.processes and all(item.returncode == 0 for item in self.processes)
        finally:
            self.trace.stop(); self.transport_trace.stop()
            self.process_trace.stop()


@contextmanager
def fixture(cwd, executable, tool_filter=None):
    runtime = CrewFixture(cwd, executable, tool_filter)
    try:
        yield runtime
    finally:
        runtime.close()
