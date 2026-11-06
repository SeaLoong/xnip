# smolagents SDK MCP bridge benchmark

26 scenarios; 3 measured workflows and 1 warm-up per provider/scenario. Zero model calls.

Both lanes execute the same xnip MCP editing engine. The direct lane uses a real JSON-RPC stdio client; the SDK lane invokes the framework's actual public tool pipeline. This measures transport/adaptation overhead and observed SDK result projection. Agent loops, model generation, billed tokens and native builtin replacement have their own boundaries.

OpenAI Agents uses Agent.get_all_tools and FunctionTool.on_invoke_tool; its documented failure_error_function=None setting preserves MCP errors as exceptions. AutoGen constructs an AssistantAgent with the actual McpWorkbench and calls the workbench's public call_tool. smolagents uses ToolCallingAgent.execute_tool_call with actual MCPAdapt tools. SDK options and model traps are retained in the fixture source.

Arguments are equal across providers. Direct MCP results and SDK projections are different returned layers; their byte counts are listed separately. Exceptions have zero returned projection bytes and their actual message bytes in a separate column. Full projected outputs, exceptions, input/result hashes and independent file goals are retained in JSON. Setup includes SDK import/discovery; warm timing includes the complete tool call. Fixture resets, byte accounting and file verification are outside timing. Three measured samples make nearest-rank p95 the maximum.

Each rejected call requires its fixture's specific remote reason. Direct MCP additionally checks JSON-RPC code -32600; Agents requires its actual McpError, AutoGen its actual named ToolResult text, and smolagents its McpError-bearing SDK exception. Successful reads precede the mutation. Connection, startup and unrelated tool failures fail sampling, and warm-up outcomes pass the same checks.

Binary SHA-256 `16cecdbad176790492338643e29480d2dc7793c5cda5629102e0dde434be8deb`; source `089504a31ecd82143e7af7269f7e0b81c0c7438ec3c77f8bf1c38c64f7223706`. SDK versions: `{"mcp": "1.30.0", "mcpadapt": "0.1.20", "smolagents": "1.26.0"}`.

| Scenario | Direct edit median / p95 ms | SDK edit median / p95 ms | Direct / SDK read ms | Direct / SDK projected bytes | Direct / SDK exception bytes | Correct direct / SDK |
|---|---:|---:|---:|---:|---:|---:|
| tiny-config | 0.89 / 0.97 | 3.16 / 3.33 | 0.20 / 0.55 | 288 / 184 | 0 / 0 | 3 / 3 |
| large-local-change | 0.97 / 0.98 | 3.53 / 3.63 | 0.21 / 0.55 | 834 / 730 | 0 / 0 | 3 / 3 |
| multiline-function | 0.89 / 1.12 | 3.23 / 3.49 | 0.21 / 0.53 | 407 / 303 | 0 / 0 | 3 / 3 |
| unicode | 0.99 / 1.16 | 3.11 / 3.19 | 0.18 / 0.53 | 295 / 191 | 0 / 0 | 3 / 3 |
| crlf | 0.93 / 0.96 | 3.23 / 3.31 | 0.20 / 0.51 | 294 / 190 | 0 / 0 | 3 / 3 |
| no-final-newline | 0.87 / 0.95 | 3.14 / 3.20 | 0.19 / 0.51 | 266 / 162 | 0 / 0 | 3 / 3 |
| disambiguated-block | 0.87 / 0.90 | 3.18 / 3.26 | 0.21 / 0.54 | 322 / 218 | 0 / 0 | 3 / 3 |
| global-replace | 0.98 / 1.02 | 3.26 / 3.39 | 0.18 / 0.52 | 624 / 520 | 0 / 0 | 3 / 3 |
| same-file-batch | 2.67 / 2.88 | 5.25 / 5.25 | 0.21 / 0.54 | 280 / 176 | 0 / 0 | 3 / 3 |
| multi-file-batch | 6.15 / 6.68 | 8.02 / 8.18 | 0.47 / 1.25 | 449 / 271 | 0 / 0 | 3 / 3 |
| long-line | 0.95 / 1.01 | 3.24 / 3.25 | 0.23 / 0.51 | 815 / 711 | 0 / 0 | 3 / 3 |
| insert-after-anchor | 0.90 / 0.91 | 3.20 / 3.24 | 0.20 / 0.49 | 276 / 172 | 0 / 0 | 3 / 3 |
| delete-block | 0.99 / 1.03 | 3.27 / 3.35 | 0.19 / 0.49 | 297 / 193 | 0 / 0 | 3 / 3 |
| bom-crlf | 0.83 / 0.96 | 3.21 / 3.30 | 0.19 / 0.54 | 269 / 165 | 0 / 0 | 3 / 3 |
| mixed-eol | 0.87 / 0.89 | 3.35 / 3.45 | 0.19 / 0.54 | 263 / 159 | 0 / 0 | 3 / 3 |
| noop | 0.15 / 0.18 | 2.41 / 2.45 | 0.18 / 0.55 | 237 / 131 | 0 / 0 | 3 / 3 |
| reject-missing | 0.16 / 0.18 | 0.72 / 0.72 | 0.15 / 0.55 | 163 / 15 | 0 / 218 | 3 / 3 |
| reject-ambiguous | 0.16 / 0.16 | 0.67 / 0.87 | 0.15 / 0.47 | 151 / 19 | 0 / 198 | 3 / 3 |
| reject-stale-write | 0.18 / 0.19 | 0.69 / 0.76 | 0.16 / 0.53 | 135 / 19 | 0 / 210 | 3 / 3 |
| preview | 0.16 / 0.16 | 2.44 / 2.58 | 0.18 / 0.55 | 336 / 133 | 0 / 0 | 3 / 3 |
| write-create | 1.94 / 2.14 | 4.49 / 4.57 | 0.00 / 0.00 | 161 / 94 | 0 / 0 | 3 / 3 |
| reject-late-batch | 0.17 / 0.18 | 0.73 / 0.76 | 0.17 / 0.51 | 185 / 15 | 0 / 303 | 3 / 3 |
| create-rename-delete | 2.36 / 2.39 | 5.18 / 5.27 | 0.00 / 0.00 | 195 / 128 | 0 / 0 | 3 / 3 |
| large-65536 | 0.90 / 0.92 | 3.18 / 3.19 | 0.30 / 0.53 | 811 / 707 | 0 / 0 | 3 / 3 |
| large-1048576 | 1.84 / 2.01 | 4.48 / 4.51 | 0.61 / 0.93 | 811 / 707 | 0 / 0 | 3 / 3 |
| large-5242880 | 6.75 / 6.90 | 10.09 / 10.23 | 2.18 / 3.27 | 811 / 707 | 0 / 0 | 3 / 3 |

## Reproduce

```sh
python benches/smolagents_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```
