# autogen SDK MCP bridge benchmark

26 scenarios; 3 measured workflows and 1 warm-up per provider/scenario. Zero model calls.

Both lanes execute the same xnip MCP editing engine. The direct lane uses a real JSON-RPC stdio client; the SDK lane invokes the framework's actual public tool pipeline. This measures transport/adaptation overhead and observed SDK result projection. Agent loops, model generation, billed tokens and native builtin replacement have their own boundaries.

OpenAI Agents uses Agent.get_all_tools and FunctionTool.on_invoke_tool; its documented failure_error_function=None setting preserves MCP errors as exceptions. AutoGen constructs an AssistantAgent with the actual McpWorkbench and calls the workbench's public call_tool. smolagents uses ToolCallingAgent.execute_tool_call with actual MCPAdapt tools. SDK options and model traps are retained in the fixture source.

Arguments are equal across providers. Direct MCP results and SDK projections are different returned layers; their byte counts are listed separately. Exceptions have zero returned projection bytes and their actual message bytes in a separate column. Full projected outputs, exceptions, input/result hashes and independent file goals are retained in JSON. Setup includes SDK import/discovery; warm timing includes the complete tool call. Fixture resets, byte accounting and file verification are outside timing. Three measured samples make nearest-rank p95 the maximum.

Each rejected call requires its fixture's specific remote reason. Direct MCP additionally checks JSON-RPC code -32600; Agents requires its actual McpError, AutoGen its actual named ToolResult text, and smolagents its McpError-bearing SDK exception. Successful reads precede the mutation. Connection, startup and unrelated tool failures fail sampling, and warm-up outcomes pass the same checks.

Binary SHA-256 `16cecdbad176790492338643e29480d2dc7793c5cda5629102e0dde434be8deb`; source `b09d070323142d27a6e422cf01e50fd8cab70f421ea4534ca224868babd66bfb`. SDK versions: `{"autogen-agentchat": "0.7.5", "autogen-ext": "0.7.5", "mcp": "1.30.0", "openai-agents": "0.24.0"}`.

| Scenario | Direct edit median / p95 ms | SDK edit median / p95 ms | Direct / SDK read ms | Direct / SDK projected bytes | Direct / SDK exception bytes | Correct direct / SDK |
|---|---:|---:|---:|---:|---:|---:|
| tiny-config | 0.84 / 0.91 | 2.91 / 2.97 | 0.19 / 0.46 | 288 / 292 | 0 / 0 | 3 / 3 |
| large-local-change | 1.09 / 1.35 | 2.97 / 2.98 | 0.20 / 0.46 | 834 / 838 | 0 / 0 | 3 / 3 |
| multiline-function | 1.02 / 1.07 | 3.12 / 3.20 | 0.24 / 0.44 | 407 / 411 | 0 / 0 | 3 / 3 |
| unicode | 0.98 / 1.01 | 3.10 / 3.11 | 0.21 / 0.47 | 295 / 299 | 0 / 0 | 3 / 3 |
| crlf | 0.94 / 0.98 | 3.13 / 3.18 | 0.19 / 0.53 | 294 / 298 | 0 / 0 | 3 / 3 |
| no-final-newline | 0.87 / 0.96 | 3.11 / 3.19 | 0.21 / 0.42 | 266 / 270 | 0 / 0 | 3 / 3 |
| disambiguated-block | 0.90 / 1.01 | 3.14 / 3.33 | 0.22 / 0.49 | 322 / 326 | 0 / 0 | 3 / 3 |
| global-replace | 0.85 / 0.89 | 3.06 / 3.23 | 0.21 / 0.46 | 624 / 627 | 0 / 0 | 3 / 3 |
| same-file-batch | 2.32 / 2.52 | 4.64 / 4.84 | 0.20 / 0.46 | 280 / 275 | 0 / 0 | 3 / 3 |
| multi-file-batch | 5.42 / 5.43 | 8.61 / 9.07 | 0.40 / 1.14 | 449 / 510 | 0 / 0 | 3 / 3 |
| long-line | 0.91 / 1.08 | 3.28 / 3.43 | 0.22 / 0.52 | 815 / 819 | 0 / 0 | 3 / 3 |
| insert-after-anchor | 0.83 / 0.91 | 3.25 / 3.26 | 0.20 / 0.42 | 276 / 280 | 0 / 0 | 3 / 3 |
| delete-block | 0.86 / 0.88 | 3.17 / 3.19 | 0.19 / 0.46 | 297 / 301 | 0 / 0 | 3 / 3 |
| bom-crlf | 0.73 / 0.74 | 2.96 / 3.16 | 0.19 / 0.41 | 269 / 277 | 0 / 0 | 3 / 3 |
| mixed-eol | 0.75 / 0.80 | 2.93 / 2.97 | 0.22 / 0.45 | 263 / 271 | 0 / 0 | 3 / 3 |
| noop | 0.17 / 0.18 | 2.10 / 2.11 | 0.18 / 0.40 | 237 / 243 | 0 / 0 | 3 / 3 |
| reject-missing | 0.15 / 0.15 | 0.30 / 0.30 | 0.17 / 0.33 | 163 / 299 | 0 / 0 | 3 / 3 |
| reject-ambiguous | 0.15 / 0.17 | 0.28 / 0.31 | 0.18 / 0.34 | 151 / 287 | 0 / 0 | 3 / 3 |
| reject-stale-write | 0.16 / 0.19 | 0.32 / 0.32 | 0.20 / 0.34 | 135 / 271 | 0 / 0 | 3 / 3 |
| preview | 0.17 / 0.26 | 2.12 / 2.18 | 0.19 / 0.44 | 336 / 341 | 0 / 0 | 3 / 3 |
| write-create | 2.22 / 2.34 | 4.27 / 4.89 | 0.00 / 0.00 | 161 / 117 | 0 / 0 | 3 / 3 |
| reject-late-batch | 0.18 / 0.20 | 0.31 / 0.31 | 0.19 / 0.35 | 185 / 322 | 0 / 0 | 3 / 3 |
| create-rename-delete | 2.38 / 2.46 | 4.32 / 5.41 | 0.00 / 0.00 | 195 / 118 | 0 / 0 | 3 / 3 |
| large-65536 | 1.02 / 1.02 | 3.20 / 3.21 | 0.23 / 0.54 | 811 / 819 | 0 / 0 | 3 / 3 |
| large-1048576 | 1.74 / 1.85 | 3.88 / 4.45 | 0.55 / 0.89 | 811 / 819 | 0 / 0 | 3 / 3 |
| large-5242880 | 7.24 / 8.69 | 10.20 / 10.26 | 2.33 / 3.06 | 811 / 819 | 0 / 0 | 3 / 3 |

## Reproduce

```sh
python benches/autogen_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```
