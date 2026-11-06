# autogen SDK MCP bridge benchmark

26 scenarios; 3 measured workflows and 1 warm-up per provider/scenario. Zero model calls.

Both lanes execute the same xnip MCP editing engine. The direct lane uses a real JSON-RPC stdio client; the SDK lane invokes the framework's actual public tool pipeline. This measures transport/adaptation overhead and observed SDK result projection. Agent loops, model generation, billed tokens and native builtin replacement have their own boundaries.

OpenAI Agents uses Agent.get_all_tools and FunctionTool.on_invoke_tool; its documented failure_error_function=None setting preserves MCP errors as exceptions. AutoGen constructs an AssistantAgent with the actual McpWorkbench and calls the workbench's public call_tool. smolagents uses ToolCallingAgent.execute_tool_call with actual MCPAdapt tools. SDK options and model traps are retained in the fixture source.

Arguments are equal across providers. Direct MCP results and SDK projections are different returned layers; their byte counts are listed separately. Exceptions have zero returned projection bytes and their actual message bytes in a separate column. Full projected outputs, exceptions, input/result hashes and independent file goals are retained in JSON. Setup includes SDK import/discovery; warm timing includes the complete tool call. Fixture resets, byte accounting and file verification are outside timing. Three measured samples make nearest-rank p95 the maximum.

Each rejected call requires its fixture's specific remote reason. Direct MCP additionally checks JSON-RPC code -32600; Agents requires its actual McpError, AutoGen its actual named ToolResult text, and smolagents its McpError-bearing SDK exception. Successful reads precede the mutation. Connection, startup and unrelated tool failures fail sampling, and warm-up outcomes pass the same checks.

Binary SHA-256 `144f76db0a340d7eede61b7fc2545c8c7c1d7d937300f378fd6c77c8da3e0688`; source `95c9bccfa8f2b8a8f3a7035fb0992dc4c37249a4a9f32c5f93fa2e3d343cca66`. SDK versions: `{"autogen-agentchat": "0.7.5", "autogen-ext": "0.7.5", "mcp": "1.30.0", "openai-agents": "0.24.0"}`.

| Scenario | Direct edit median / p95 ms | SDK edit median / p95 ms | Direct / SDK read ms | Direct / SDK projected bytes | Direct / SDK exception bytes | Correct direct / SDK |
|---|---:|---:|---:|---:|---:|---:|
| tiny-config | 0.84 / 0.84 | 2.99 / 3.38 | 0.23 / 0.45 | 288 / 292 | 0 / 0 | 3 / 3 |
| large-local-change | 0.95 / 1.04 | 3.13 / 3.22 | 0.24 / 0.46 | 834 / 838 | 0 / 0 | 3 / 3 |
| multiline-function | 0.85 / 0.98 | 2.87 / 2.96 | 0.23 / 0.45 | 407 / 411 | 0 / 0 | 3 / 3 |
| unicode | 0.82 / 0.86 | 2.80 / 2.84 | 0.23 / 0.49 | 295 / 299 | 0 / 0 | 3 / 3 |
| crlf | 0.83 / 1.11 | 2.85 / 2.94 | 0.22 / 0.46 | 294 / 298 | 0 / 0 | 3 / 3 |
| no-final-newline | 0.83 / 0.86 | 2.89 / 2.89 | 0.22 / 0.44 | 266 / 270 | 0 / 0 | 3 / 3 |
| disambiguated-block | 0.82 / 0.83 | 2.93 / 3.32 | 0.21 / 0.44 | 322 / 326 | 0 / 0 | 3 / 3 |
| global-replace | 0.83 / 0.90 | 2.85 / 2.97 | 0.21 / 0.46 | 624 / 627 | 0 / 0 | 3 / 3 |
| same-file-batch | 2.09 / 2.13 | 3.85 / 3.95 | 0.22 / 0.44 | 280 / 275 | 0 / 0 | 3 / 3 |
| multi-file-batch | 4.29 / 4.52 | 5.52 / 5.67 | 0.53 / 1.17 | 449 / 510 | 0 / 0 | 3 / 3 |
| long-line | 0.90 / 0.90 | 2.83 / 2.86 | 0.25 / 0.45 | 815 / 819 | 0 / 0 | 3 / 3 |
| insert-after-anchor | 0.92 / 0.96 | 2.73 / 2.79 | 0.23 / 0.44 | 276 / 280 | 0 / 0 | 3 / 3 |
| delete-block | 0.83 / 0.94 | 2.75 / 2.87 | 0.21 / 0.43 | 297 / 301 | 0 / 0 | 3 / 3 |
| bom-crlf | 0.72 / 0.78 | 2.91 / 5.28 | 0.22 / 0.44 | 269 / 277 | 0 / 0 | 3 / 3 |
| mixed-eol | 0.69 / 0.70 | 2.70 / 2.86 | 0.20 / 0.43 | 263 / 271 | 0 / 0 | 3 / 3 |
| noop | 0.17 / 0.18 | 2.13 / 2.20 | 0.19 / 0.41 | 237 / 243 | 0 / 0 | 3 / 3 |
| reject-missing | 0.15 / 0.16 | 0.27 / 0.27 | 0.18 / 0.33 | 163 / 299 | 0 / 0 | 3 / 3 |
| reject-ambiguous | 0.15 / 0.16 | 0.29 / 0.30 | 0.18 / 0.31 | 151 / 287 | 0 / 0 | 3 / 3 |
| reject-stale-write | 0.19 / 0.24 | 0.27 / 0.27 | 0.18 / 0.33 | 135 / 271 | 0 / 0 | 3 / 3 |
| preview | 0.17 / 0.18 | 2.16 / 2.25 | 0.17 / 0.39 | 336 / 341 | 0 / 0 | 3 / 3 |
| write-create | 1.74 / 1.77 | 3.54 / 3.67 | 0.00 / 0.00 | 161 / 117 | 0 / 0 | 3 / 3 |
| reject-late-batch | 0.12 / 0.15 | 0.30 / 0.30 | 0.17 / 0.35 | 185 / 322 | 0 / 0 | 3 / 3 |
| create-rename-delete | 1.67 / 1.98 | 3.83 / 4.00 | 0.00 / 0.00 | 195 / 118 | 0 / 0 | 3 / 3 |
| large-65536 | 0.73 / 0.74 | 2.72 / 2.76 | 0.21 / 0.44 | 811 / 819 | 0 / 0 | 3 / 3 |
| large-1048576 | 2.00 / 2.11 | 3.66 / 3.72 | 0.61 / 0.83 | 811 / 819 | 0 / 0 | 3 / 3 |
| large-5242880 | 6.47 / 6.68 | 9.03 / 9.28 | 2.18 / 2.91 | 811 / 819 | 0 / 0 | 3 / 3 |

## Reproduce

```sh
python benches/autogen_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```
