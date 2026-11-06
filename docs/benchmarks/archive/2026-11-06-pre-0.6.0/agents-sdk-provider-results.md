# agents-sdk SDK MCP bridge benchmark

26 scenarios; 3 measured workflows and 1 warm-up per provider/scenario. Zero model calls.

Both lanes execute the same xnip MCP editing engine. The direct lane uses a real JSON-RPC stdio client; the SDK lane invokes the framework's actual public tool pipeline. This measures transport/adaptation overhead and observed SDK result projection. Agent loops, model generation, billed tokens and native builtin replacement have their own boundaries.

OpenAI Agents uses Agent.get_all_tools and FunctionTool.on_invoke_tool; its documented failure_error_function=None setting preserves MCP errors as exceptions. AutoGen constructs an AssistantAgent with the actual McpWorkbench and calls the workbench's public call_tool. smolagents uses ToolCallingAgent.execute_tool_call with actual MCPAdapt tools. SDK options and model traps are retained in the fixture source.

Arguments are equal across providers. Direct MCP results and SDK projections are different returned layers; their byte counts are listed separately. Exceptions have zero returned projection bytes and their actual message bytes in a separate column. Full projected outputs, exceptions, input/result hashes and independent file goals are retained in JSON. Setup includes SDK import/discovery; warm timing includes the complete tool call. Fixture resets, byte accounting and file verification are outside timing. Three measured samples make nearest-rank p95 the maximum.

Each rejected call requires its fixture's specific remote reason. Direct MCP additionally checks JSON-RPC code -32600; Agents requires its actual McpError, AutoGen its actual named ToolResult text, and smolagents its McpError-bearing SDK exception. Successful reads precede the mutation. Connection, startup and unrelated tool failures fail sampling, and warm-up outcomes pass the same checks.

Binary SHA-256 `16cecdbad176790492338643e29480d2dc7793c5cda5629102e0dde434be8deb`; source `bb02700afd9c13d93ea0ede2d60c7cbbe335879c2349012fc2c3fa705bdf6c22`. SDK versions: `{"autogen-agentchat": "0.7.5", "autogen-ext": "0.7.5", "mcp": "1.30.0", "openai-agents": "0.24.0"}`.

| Scenario | Direct edit median / p95 ms | SDK edit median / p95 ms | Direct / SDK read ms | Direct / SDK projected bytes | Direct / SDK exception bytes | Correct direct / SDK |
|---|---:|---:|---:|---:|---:|---:|
| tiny-config | 0.84 / 0.85 | 3.04 / 3.04 | 0.22 / 0.57 | 288 / 238 | 0 / 0 | 3 / 3 |
| large-local-change | 1.03 / 1.12 | 3.14 / 3.39 | 0.24 / 0.54 | 834 / 784 | 0 / 0 | 3 / 3 |
| multiline-function | 0.79 / 0.79 | 3.03 / 3.12 | 0.17 / 0.48 | 407 / 357 | 0 / 0 | 3 / 3 |
| unicode | 0.85 / 1.14 | 3.13 / 3.49 | 0.20 / 0.50 | 295 / 245 | 0 / 0 | 3 / 3 |
| crlf | 0.78 / 0.97 | 3.11 / 3.33 | 0.19 / 0.52 | 294 / 244 | 0 / 0 | 3 / 3 |
| no-final-newline | 0.98 / 0.99 | 3.01 / 3.21 | 0.20 / 0.50 | 266 / 216 | 0 / 0 | 3 / 3 |
| disambiguated-block | 0.74 / 0.79 | 3.01 / 3.02 | 0.17 / 0.48 | 322 / 272 | 0 / 0 | 3 / 3 |
| global-replace | 0.77 / 0.83 | 2.97 / 3.07 | 0.21 / 0.50 | 624 / 574 | 0 / 0 | 3 / 3 |
| same-file-batch | 1.96 / 2.08 | 4.51 / 4.81 | 0.24 / 0.49 | 280 / 226 | 0 / 0 | 3 / 3 |
| multi-file-batch | 4.24 / 4.26 | 7.71 / 7.82 | 0.51 / 1.31 | 449 / 373 | 0 / 0 | 3 / 3 |
| long-line | 0.86 / 0.89 | 3.08 / 3.12 | 0.22 / 0.53 | 815 / 765 | 0 / 0 | 3 / 3 |
| insert-after-anchor | 0.90 / 0.93 | 3.14 / 3.22 | 0.19 / 0.61 | 276 / 226 | 0 / 0 | 3 / 3 |
| delete-block | 0.83 / 0.95 | 3.12 / 3.25 | 0.19 / 0.48 | 297 / 247 | 0 / 0 | 3 / 3 |
| bom-crlf | 0.73 / 0.77 | 2.82 / 2.93 | 0.24 / 0.50 | 269 / 219 | 0 / 0 | 3 / 3 |
| mixed-eol | 0.73 / 0.74 | 3.01 / 3.22 | 0.20 / 0.47 | 263 / 213 | 0 / 0 | 3 / 3 |
| noop | 0.13 / 0.13 | 2.17 / 2.19 | 0.15 / 0.44 | 237 / 185 | 0 / 0 | 3 / 3 |
| reject-missing | 0.12 / 0.13 | 0.35 / 0.42 | 0.14 / 0.37 | 163 / 38 | 0 / 65 | 3 / 3 |
| reject-ambiguous | 0.10 / 0.13 | 0.32 / 0.38 | 0.17 / 0.36 | 151 / 42 | 0 / 49 | 3 / 3 |
| reject-stale-write | 0.15 / 0.17 | 0.34 / 0.36 | 0.15 / 0.40 | 135 / 42 | 0 / 33 | 3 / 3 |
| preview | 0.13 / 0.15 | 2.20 / 2.26 | 0.15 / 0.45 | 336 / 187 | 0 / 0 | 3 / 3 |
| write-create | 2.02 / 2.03 | 4.46 / 4.47 | 0.00 / 0.00 | 161 / 121 | 0 / 0 | 3 / 3 |
| reject-late-batch | 0.14 / 0.16 | 0.43 / 0.52 | 0.20 / 0.43 | 185 / 38 | 0 / 87 | 3 / 3 |
| create-rename-delete | 1.78 / 1.80 | 4.33 / 4.33 | 0.00 / 0.00 | 195 / 155 | 0 / 0 | 3 / 3 |
| large-65536 | 0.72 / 0.81 | 2.93 / 3.01 | 0.22 / 0.49 | 811 / 761 | 0 / 0 | 3 / 3 |
| large-1048576 | 2.05 / 2.05 | 4.09 / 4.10 | 0.58 / 0.97 | 811 / 761 | 0 / 0 | 3 / 3 |
| large-5242880 | 7.36 / 7.88 | 9.49 / 9.50 | 2.34 / 2.78 | 811 / 761 | 0 / 0 | 3 / 3 |

## Reproduce

```sh
python benches/agents_sdk_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```
