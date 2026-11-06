# autogen SDK MCP bridge benchmark

26 scenarios; 3 measured workflows and 1 warm-up per provider/scenario. Zero model calls.

Both lanes execute the same xnip MCP editing engine. The direct lane uses a real JSON-RPC stdio client; the SDK lane invokes the framework's actual public tool pipeline. This measures transport/adaptation overhead and observed SDK result projection. Agent loops, model generation, billed tokens and native builtin replacement have their own boundaries.

OpenAI Agents uses Agent.get_all_tools and FunctionTool.on_invoke_tool; its documented failure_error_function=None setting preserves MCP errors as exceptions. AutoGen constructs an AssistantAgent with the actual McpWorkbench and calls the workbench's public call_tool. smolagents uses ToolCallingAgent.execute_tool_call with actual MCPAdapt tools. SDK options and model traps are retained in the fixture source.

Arguments are equal across providers. Direct MCP results and SDK projections are different returned layers; their byte counts are listed separately. Exceptions have zero returned projection bytes and their actual message bytes in a separate column. Full projected outputs, exceptions, input/result hashes and independent file goals are retained in JSON. Setup includes SDK import/discovery; warm timing includes the complete tool call. Fixture resets, byte accounting and file verification are outside timing. Three measured samples make nearest-rank p95 the maximum.

Each rejected call requires its fixture's specific remote reason. Direct MCP additionally checks JSON-RPC code -32600; Agents requires its actual McpError, AutoGen its actual named ToolResult text, and smolagents its McpError-bearing SDK exception. Successful reads precede the mutation. Connection, startup and unrelated tool failures fail sampling, and warm-up outcomes pass the same checks.

Binary SHA-256 `b01cc02a39864cb7f460cdd29fc84c8059855229acc6aec97c43d1070505d012`; source `3c0fdc96d2a33f9c2b8f28770b6d60bb4a8c91219bb8266d1d8f1621b3a20583`. SDK versions: `{"autogen-agentchat": "0.7.5", "autogen-ext": "0.7.5", "mcp": "1.30.0", "openai-agents": "0.24.0"}`.

| Scenario | Direct edit median / p95 ms | SDK edit median / p95 ms | Direct / SDK read ms | Direct / SDK projected bytes | Direct / SDK exception bytes | Correct direct / SDK |
|---|---:|---:|---:|---:|---:|---:|
| tiny-config | 0.75 / 0.87 | 2.85 / 2.94 | 0.20 / 0.47 | 288 / 292 | 0 / 0 | 3 / 3 |
| large-local-change | 0.92 / 1.06 | 2.86 / 2.92 | 0.21 / 0.49 | 834 / 838 | 0 / 0 | 3 / 3 |
| multiline-function | 0.84 / 0.92 | 2.82 / 2.84 | 0.19 / 0.44 | 407 / 411 | 0 / 0 | 3 / 3 |
| unicode | 0.80 / 1.05 | 2.82 / 2.90 | 0.18 / 0.45 | 295 / 299 | 0 / 0 | 3 / 3 |
| crlf | 0.82 / 0.82 | 2.83 / 2.84 | 0.18 / 0.44 | 294 / 298 | 0 / 0 | 3 / 3 |
| no-final-newline | 0.79 / 1.10 | 2.84 / 3.05 | 0.17 / 0.44 | 266 / 270 | 0 / 0 | 3 / 3 |
| disambiguated-block | 0.79 / 0.83 | 2.92 / 2.94 | 0.17 / 0.46 | 322 / 326 | 0 / 0 | 3 / 3 |
| global-replace | 0.77 / 0.78 | 2.82 / 2.96 | 0.19 / 0.44 | 624 / 627 | 0 / 0 | 3 / 3 |
| same-file-batch | 2.18 / 2.34 | 4.30 / 4.46 | 0.20 / 0.45 | 280 / 275 | 0 / 0 | 3 / 3 |
| multi-file-batch | 4.45 / 4.50 | 6.62 / 6.98 | 0.45 / 1.15 | 449 / 510 | 0 / 0 | 3 / 3 |
| long-line | 0.93 / 0.94 | 2.85 / 2.89 | 0.24 / 0.46 | 815 / 819 | 0 / 0 | 3 / 3 |
| insert-after-anchor | 0.89 / 1.07 | 2.82 / 2.84 | 0.21 / 0.45 | 276 / 280 | 0 / 0 | 3 / 3 |
| delete-block | 0.87 / 0.90 | 2.79 / 2.81 | 0.18 / 0.44 | 297 / 301 | 0 / 0 | 3 / 3 |
| bom-crlf | 0.74 / 0.85 | 2.72 / 2.78 | 0.17 / 0.43 | 269 / 277 | 0 / 0 | 3 / 3 |
| mixed-eol | 0.74 / 0.75 | 2.69 / 2.76 | 0.18 / 0.46 | 263 / 271 | 0 / 0 | 3 / 3 |
| noop | 0.12 / 0.13 | 2.12 / 2.12 | 0.15 / 0.40 | 237 / 243 | 0 / 0 | 3 / 3 |
| reject-missing | 0.10 / 0.11 | 0.29 / 0.31 | 0.14 / 0.39 | 163 / 299 | 0 / 0 | 3 / 3 |
| reject-ambiguous | 0.09 / 0.09 | 0.26 / 0.27 | 0.13 / 0.36 | 151 / 287 | 0 / 0 | 3 / 3 |
| reject-stale-write | 0.10 / 0.11 | 0.28 / 0.29 | 0.13 / 0.34 | 135 / 271 | 0 / 0 | 3 / 3 |
| preview | 0.10 / 0.12 | 2.14 / 2.15 | 0.15 / 0.39 | 336 / 341 | 0 / 0 | 3 / 3 |
| write-create | 1.66 / 1.80 | 3.68 / 3.86 | 0.00 / 0.00 | 161 / 117 | 0 / 0 | 3 / 3 |
| reject-late-batch | 0.11 / 0.12 | 0.32 / 0.34 | 0.15 / 0.36 | 185 / 322 | 0 / 0 | 3 / 3 |
| create-rename-delete | 1.84 / 2.08 | 3.93 / 4.04 | 0.00 / 0.00 | 195 / 118 | 0 / 0 | 3 / 3 |
| large-65536 | 0.69 / 0.70 | 2.74 / 2.75 | 0.18 / 0.49 | 811 / 819 | 0 / 0 | 3 / 3 |
| large-1048576 | 1.61 / 1.70 | 3.64 / 3.89 | 0.52 / 0.85 | 811 / 819 | 0 / 0 | 3 / 3 |
| large-5242880 | 6.84 / 6.91 | 9.52 / 9.84 | 2.37 / 3.43 | 811 / 819 | 0 / 0 | 3 / 3 |

## Reproduce

```sh
python benches/autogen_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```
