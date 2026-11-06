"""Actual DeepAgents/LangGraph/PydanticAI tool dispatch, with zero model requests."""
from contextlib import contextmanager
from pathlib import Path
from unittest.mock import patch

from langchain_core.messages import AIMessage
from langchain_core.language_models.chat_models import BaseChatModel
from langgraph.graph import StateGraph, MessagesState, START, END
from langgraph.prebuilt import ToolNode
from pydantic_ai import Agent, RunContext
from pydantic_ai.models.test import TestModel
from pydantic_ai.usage import RunUsage


@contextmanager
def model_guard():
    guards = [patch.object(cls, method, side_effect=AssertionError("Tool-only SDK execution requested a model"))
              for cls, method in [(BaseChatModel, "generate"), (BaseChatModel, "agenerate"),
                                  (TestModel, "request"), (TestModel, "request_stream")]]
    mocks = [guard.start() for guard in guards]
    try:
        yield lambda: sum(mock.call_count for mock in mocks)
    finally:
        for guard in reversed(guards):
            guard.stop()
        assert sum(mock.call_count for mock in mocks) == 0


class Runtime:
    def __init__(self, framework, cwd, executable, *, native=True, permissions=None,
                 writable_roots=None, process_tool_call=None, wrap_tool_call=None):
        self.framework, self.cwd, self.executable = framework, Path(cwd), str(executable)
        self.native, self.permissions, self.writable_roots = native, permissions, writable_roots
        self.process_tool_call, self.wrap_tool_call = process_tool_call, wrap_tool_call
        self.owner = None
        self.calls = 0
        self.closed = False

    async def __aenter__(self):
        if self.framework == "deepagents":
            from deepagents.backends import FilesystemBackend
            from deepagents.middleware.filesystem import FilesystemMiddleware
            from xnip_python_agents.deepagents import XnipFilesystemBackend
            self.owner = XnipFilesystemBackend(self.cwd, executable=self.executable,
                writable_roots=self.writable_roots) if self.native else FilesystemBackend(self.cwd)
            self.middleware = FilesystemMiddleware(backend=self.owner, permissions=self.permissions,
                                                   tool_token_limit_before_evict=None)
            self.tools = self.middleware.tools
            node = ToolNode(self.tools, handle_tool_errors=True,
                            wrap_tool_call=self.middleware.wrap_tool_call,
                            awrap_tool_call=self.middleware.awrap_tool_call)
        elif self.framework == "langgraph":
            from xnip_python_agents.langgraph import XnipLangGraphTools
            self.owner = XnipLangGraphTools(self.cwd, executable=self.executable,
                                          writable_roots=self.writable_roots)
            self.tools = self.owner.tools
            node = ToolNode(self.tools, handle_tool_errors=True, wrap_tool_call=self.wrap_tool_call)
        elif self.framework == "pydantic-ai":
            from xnip_python_agents.pydantic_ai import xnip_toolset
            self.owner = xnip_toolset(self.cwd, executable=self.executable,
                                     process_tool_call=self.process_tool_call)
            self.model = TestModel()
            self.agent = Agent(model=self.model, toolsets=[self.owner])
            self.context = RunContext(deps=None, model=self.model, usage=RunUsage(), agent=self.agent)
            await self.owner.__aenter__()
            self.definitions = await self.owner.get_tools(self.context)
            return self
        else:
            raise ValueError(self.framework)
        builder = StateGraph(MessagesState)
        builder.add_node("tools", node)
        builder.add_edge(START, "tools")
        builder.add_edge("tools", END)
        self.graph = builder.compile()
        return self

    def catalog(self):
        if self.framework == "pydantic-ai":
            return {name: {"description": tool.tool_def.description,
                           "parameters": tool.tool_def.parameters_json_schema}
                    for name, tool in self.definitions.items()}
        return {tool.name: {"description": tool.description,
                           "parameters": tool.tool_call_schema.model_json_schema()}
                for tool in self.tools}

    async def call(self, name, arguments):
        self.calls += 1
        if self.framework == "pydantic-ai":
            result = await self.owner.call_tool(name, arguments, self.context, self.definitions[name])
            return {"name": name, "content": result, "status": "success"}
        message = AIMessage(content="", tool_calls=[{"name": name, "args": arguments,
            "id": f"xnip-python-{self.calls}", "type": "tool_call"}])
        result = await self.graph.ainvoke({"messages": [message]})
        tool_message = result["messages"][-1]
        return {"name": tool_message.name, "content": tool_message.content, "status": tool_message.status}

    def request(self, action, file, *, old=None, text=None, replace_all=False):
        if self.framework == "deepagents":
            if action == "read": return "read_file", {"file_path": "/" + file}
            if action == "write": return "write_file", {"file_path": "/" + file, "content": text}
            return "edit_file", {"file_path": "/" + file, "old_string": old,
                                  "new_string": text, "replace_all": replace_all}
        if self.framework == "langgraph":
            if action == "read": return "xnip_read", {"file_path": file}
            if action == "write": return "xnip_write", {"file_path": file, "text": text}
            return "xnip_edit", {"file_path": file, "old": old, "text": text, "replace_all": replace_all}
        if action == "read": return "xnip_peek", {"file": file, "all": True}
        if action == "write": return "xnip_edit", {"file": file, "text": text, "operation": "write"}
        return "xnip_edit", {"file": file, "old": old, "text": text, "all": replace_all}

    async def read(self, name):
        return await self.call(*self.request("read", name))

    async def edit(self, name, old, text, replace_all=False):
        return await self.call(*self.request("edit", name, old=old, text=text, replace_all=replace_all))

    async def write(self, name, text):
        return await self.call(*self.request("write", name, text=text))

    async def __aexit__(self, *error):
        if self.closed:
            return
        self.closed = True
        if self.framework == "pydantic-ai":
            await self.owner.__aexit__(*error)
        elif hasattr(self.owner, "close"):
            self.owner.close()
