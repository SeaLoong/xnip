# agents-sdk SDK MCP bridge benchmark

26 scenarios; 3 measured workflows and 1 warm-up per provider/scenario. Zero model calls.

Both lanes execute the same xnip MCP editing engine. The direct lane uses a real JSON-RPC stdio client; the SDK lane invokes the framework's actual public tool pipeline. This measures transport/adaptation overhead and observed SDK result projection. Agent loops, model generation, billed tokens and native builtin replacement have their own boundaries.

OpenAI Agents uses Agent.get_all_tools and FunctionTool.on_invoke_tool; its documented failure_error_function=None setting preserves MCP errors as exceptions. AutoGen constructs an AssistantAgent with the actual McpWorkbench and calls the workbench's public call_tool. smolagents uses ToolCallingAgent.execute_tool_call with actual MCPAdapt tools. SDK options and model traps are retained in the fixture source.

Arguments are equal across providers. Direct MCP results and SDK projections are different returned layers; their byte counts are listed separately. Exceptions have zero returned projection bytes and their actual message bytes in a separate column. Full projected outputs, exceptions, input/result hashes and independent file goals are retained in JSON. Setup includes SDK import/discovery; warm timing includes the complete tool call. Fixture resets, byte accounting and file verification are outside timing. Three measured samples make nearest-rank p95 the maximum.

Each rejected call requires its fixture's specific remote reason. Direct MCP additionally checks JSON-RPC code -32600; Agents requires its actual McpError, AutoGen its actual named ToolResult text, and smolagents its McpError-bearing SDK exception. Successful reads precede the mutation. Connection, startup and unrelated tool failures fail sampling, and warm-up outcomes pass the same checks.

Binary SHA-256 `144f76db0a340d7eede61b7fc2545c8c7c1d7d937300f378fd6c77c8da3e0688`; source `2d47c0a4249dc270d096cacdad5afecfa418e49be7bc2eb17c03a48085db0c9a`. SDK versions: `{"autogen-agentchat": "0.7.5", "autogen-ext": "0.7.5", "mcp": "1.30.0", "openai-agents": "0.24.0"}`.

| Scenario | Direct edit median / p95 ms | SDK edit median / p95 ms | Direct / SDK read ms | Direct / SDK projected bytes | Direct / SDK exception bytes | Correct direct / SDK |
|---|---:|---:|---:|---:|---:|---:|
| tiny-config | 0.82 / 0.97 | 2.96 / 3.25 | 0.18 / 0.50 | 288 / 238 | 0 / 0 | 3 / 3 |
| large-local-change | 0.92 / 0.96 | 3.03 / 3.06 | 0.22 / 0.50 | 834 / 784 | 0 / 0 | 3 / 3 |
| multiline-function | 0.94 / 0.97 | 2.99 / 3.06 | 0.19 / 0.49 | 407 / 357 | 0 / 0 | 3 / 3 |
| unicode | 0.90 / 0.94 | 2.94 / 2.98 | 0.19 / 0.48 | 295 / 245 | 0 / 0 | 3 / 3 |
| crlf | 0.89 / 0.96 | 2.94 / 2.95 | 0.19 / 0.54 | 294 / 244 | 0 / 0 | 3 / 3 |
| no-final-newline | 0.89 / 0.92 | 2.88 / 3.10 | 0.18 / 0.52 | 266 / 216 | 0 / 0 | 3 / 3 |
| disambiguated-block | 0.88 / 0.91 | 2.96 / 3.17 | 0.18 / 0.57 | 322 / 272 | 0 / 0 | 3 / 3 |
| global-replace | 0.88 / 0.90 | 2.94 / 3.34 | 0.19 / 0.48 | 624 / 574 | 0 / 0 | 3 / 3 |
| same-file-batch | 2.38 / 2.39 | 4.06 / 4.23 | 0.20 / 0.48 | 280 / 226 | 0 / 0 | 3 / 3 |
| multi-file-batch | 4.28 / 5.04 | 6.41 / 6.43 | 0.51 / 1.23 | 449 / 373 | 0 / 0 | 3 / 3 |
| long-line | 0.80 / 0.82 | 2.93 / 2.99 | 0.20 / 0.50 | 815 / 765 | 0 / 0 | 3 / 3 |
| insert-after-anchor | 0.79 / 0.79 | 2.94 / 2.97 | 0.18 / 0.47 | 276 / 226 | 0 / 0 | 3 / 3 |
| delete-block | 0.79 / 0.80 | 2.94 / 2.96 | 0.18 / 0.51 | 297 / 247 | 0 / 0 | 3 / 3 |
| bom-crlf | 0.59 / 0.64 | 2.72 / 2.75 | 0.17 / 0.48 | 269 / 219 | 0 / 0 | 3 / 3 |
| mixed-eol | 0.62 / 0.71 | 2.78 / 2.90 | 0.19 / 0.49 | 263 / 213 | 0 / 0 | 3 / 3 |
| noop | 0.14 / 0.15 | 2.24 / 2.26 | 0.16 / 0.42 | 237 / 185 | 0 / 0 | 3 / 3 |
| reject-missing | 0.13 / 0.14 | 0.38 / 0.44 | 0.16 / 0.38 | 163 / 38 | 0 / 65 | 3 / 3 |
| reject-ambiguous | 0.12 / 0.13 | 0.35 / 0.35 | 0.14 / 0.36 | 151 / 42 | 0 / 49 | 3 / 3 |
| reject-stale-write | 0.14 / 0.17 | 0.39 / 0.89 | 0.16 / 0.61 | 135 / 42 | 0 / 33 | 3 / 3 |
| preview | 0.14 / 0.15 | 2.57 / 3.02 | 0.16 / 0.60 | 336 / 187 | 0 / 0 | 3 / 3 |
| write-create | 1.47 / 1.76 | 3.58 / 3.73 | 0.00 / 0.00 | 161 / 121 | 0 / 0 | 3 / 3 |
| reject-late-batch | 0.17 / 0.17 | 0.43 / 0.44 | 0.16 / 0.42 | 185 / 38 | 0 / 87 | 3 / 3 |
| create-rename-delete | 1.47 / 1.62 | 3.84 / 3.88 | 0.00 / 0.00 | 195 / 155 | 0 / 0 | 3 / 3 |
| large-65536 | 0.78 / 1.01 | 2.89 / 3.01 | 0.22 / 0.61 | 811 / 761 | 0 / 0 | 3 / 3 |
| large-1048576 | 1.72 / 1.80 | 3.89 / 3.96 | 0.58 / 0.91 | 811 / 761 | 0 / 0 | 3 / 3 |
| large-5242880 | 6.75 / 6.80 | 9.06 / 9.10 | 2.14 / 2.79 | 811 / 761 | 0 / 0 | 3 / 3 |

## Reproduce

```sh
python benches/agents_sdk_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```
