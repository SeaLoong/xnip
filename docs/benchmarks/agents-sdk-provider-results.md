# agents-sdk SDK MCP bridge benchmark

26 scenarios; 3 measured workflows and 1 warm-up per provider/scenario. Zero model calls.

Both lanes execute the same xnip MCP editing engine. The direct lane uses a real JSON-RPC stdio client; the SDK lane invokes the framework's actual public tool pipeline. This measures transport/adaptation overhead and observed SDK result projection. Agent loops, model generation, billed tokens and native builtin replacement have their own boundaries.

OpenAI Agents uses Agent.get_all_tools and FunctionTool.on_invoke_tool; its documented failure_error_function=None setting preserves MCP errors as exceptions. AutoGen constructs an AssistantAgent with the actual McpWorkbench and calls the workbench's public call_tool. smolagents uses ToolCallingAgent.execute_tool_call with actual MCPAdapt tools. SDK options and model traps are retained in the fixture source.

Arguments are equal across providers. Direct MCP results and SDK projections are different returned layers; their byte counts are listed separately. Exceptions have zero returned projection bytes and their actual message bytes in a separate column. Full projected outputs, exceptions, input/result hashes and independent file goals are retained in JSON. Setup includes SDK import/discovery; warm timing includes the complete tool call. Fixture resets, byte accounting and file verification are outside timing. Three measured samples make nearest-rank p95 the maximum.

Each rejected call requires its fixture's specific remote reason. Direct MCP additionally checks JSON-RPC code -32600; Agents requires its actual McpError, AutoGen its actual named ToolResult text, and smolagents its McpError-bearing SDK exception. Successful reads precede the mutation. Connection, startup and unrelated tool failures fail sampling, and warm-up outcomes pass the same checks.

Binary SHA-256 `b01cc02a39864cb7f460cdd29fc84c8059855229acc6aec97c43d1070505d012`; source `8bb7a6be8372e69b7cd85133eee6e4f9c6251abf32217ce049147c5110dc55f9`. SDK versions: `{"autogen-agentchat": "0.7.5", "autogen-ext": "0.7.5", "mcp": "1.30.0", "openai-agents": "0.24.0"}`.

| Scenario | Direct edit median / p95 ms | SDK edit median / p95 ms | Direct / SDK read ms | Direct / SDK projected bytes | Direct / SDK exception bytes | Correct direct / SDK |
|---|---:|---:|---:|---:|---:|---:|
| tiny-config | 0.93 / 1.21 | 2.98 / 3.07 | 0.19 / 0.51 | 288 / 238 | 0 / 0 | 3 / 3 |
| large-local-change | 1.47 / 1.51 | 3.04 / 3.06 | 0.21 / 0.54 | 834 / 784 | 0 / 0 | 3 / 3 |
| multiline-function | 0.91 / 0.95 | 2.89 / 2.97 | 0.14 / 0.52 | 407 / 357 | 0 / 0 | 3 / 3 |
| unicode | 0.86 / 0.87 | 2.97 / 3.00 | 0.15 / 0.46 | 295 / 245 | 0 / 0 | 3 / 3 |
| crlf | 0.95 / 1.48 | 3.00 / 3.02 | 0.17 / 0.43 | 294 / 244 | 0 / 0 | 3 / 3 |
| no-final-newline | 1.11 / 1.16 | 2.99 / 3.19 | 0.19 / 0.44 | 266 / 216 | 0 / 0 | 3 / 3 |
| disambiguated-block | 0.98 / 0.99 | 3.10 / 3.10 | 0.16 / 0.54 | 322 / 272 | 0 / 0 | 3 / 3 |
| global-replace | 1.05 / 1.17 | 3.00 / 3.28 | 0.15 / 0.50 | 624 / 574 | 0 / 0 | 3 / 3 |
| same-file-batch | 2.75 / 2.79 | 5.07 / 7.89 | 0.17 / 0.53 | 280 / 226 | 0 / 0 | 3 / 3 |
| multi-file-batch | 4.95 / 5.46 | 6.74 / 7.91 | 0.35 / 1.31 | 449 / 373 | 0 / 0 | 3 / 3 |
| long-line | 0.81 / 0.85 | 2.96 / 3.16 | 0.16 / 0.55 | 815 / 765 | 0 / 0 | 3 / 3 |
| insert-after-anchor | 0.77 / 0.80 | 2.92 / 2.92 | 0.16 / 0.52 | 276 / 226 | 0 / 0 | 3 / 3 |
| delete-block | 0.78 / 0.81 | 3.17 / 3.18 | 0.21 / 0.53 | 297 / 247 | 0 / 0 | 3 / 3 |
| bom-crlf | 0.68 / 0.72 | 2.97 / 3.00 | 0.15 / 0.54 | 269 / 219 | 0 / 0 | 3 / 3 |
| mixed-eol | 0.72 / 0.75 | 2.94 / 3.04 | 0.14 / 0.45 | 263 / 213 | 0 / 0 | 3 / 3 |
| noop | 0.10 / 0.12 | 2.21 / 2.39 | 0.13 / 0.46 | 237 / 185 | 0 / 0 | 3 / 3 |
| reject-missing | 0.13 / 0.24 | 0.36 / 0.44 | 0.21 / 0.37 | 163 / 38 | 0 / 65 | 3 / 3 |
| reject-ambiguous | 0.15 / 0.16 | 0.37 / 0.39 | 0.18 / 0.45 | 151 / 42 | 0 / 49 | 3 / 3 |
| reject-stale-write | 0.14 / 0.15 | 0.41 / 0.42 | 0.17 / 0.43 | 135 / 42 | 0 / 33 | 3 / 3 |
| preview | 0.14 / 0.19 | 2.30 / 2.41 | 0.17 / 0.49 | 336 / 187 | 0 / 0 | 3 / 3 |
| write-create | 1.86 / 1.95 | 4.15 / 4.23 | 0.00 / 0.00 | 161 / 121 | 0 / 0 | 3 / 3 |
| reject-late-batch | 0.17 / 0.18 | 0.37 / 0.40 | 0.20 / 0.39 | 185 / 38 | 0 / 87 | 3 / 3 |
| create-rename-delete | 2.79 / 2.79 | 4.29 / 4.31 | 0.00 / 0.00 | 195 / 155 | 0 / 0 | 3 / 3 |
| large-65536 | 0.84 / 0.90 | 2.84 / 2.87 | 0.23 / 0.49 | 811 / 761 | 0 / 0 | 3 / 3 |
| large-1048576 | 1.78 / 1.91 | 3.84 / 3.93 | 0.58 / 0.86 | 811 / 761 | 0 / 0 | 3 / 3 |
| large-5242880 | 7.57 / 7.73 | 9.80 / 9.93 | 2.33 / 3.09 | 811 / 761 | 0 / 0 | 3 / 3 |

## Reproduce

```sh
python benches/agents_sdk_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```
