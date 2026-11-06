# smolagents SDK MCP bridge benchmark

26 scenarios; 3 measured workflows and 1 warm-up per provider/scenario. Zero model calls.

Both lanes execute the same xnip MCP editing engine. The direct lane uses a real JSON-RPC stdio client; the SDK lane invokes the framework's actual public tool pipeline. This measures transport/adaptation overhead and observed SDK result projection. Agent loops, model generation, billed tokens and native builtin replacement have their own boundaries.

OpenAI Agents uses Agent.get_all_tools and FunctionTool.on_invoke_tool; its documented failure_error_function=None setting preserves MCP errors as exceptions. AutoGen constructs an AssistantAgent with the actual McpWorkbench and calls the workbench's public call_tool. smolagents uses ToolCallingAgent.execute_tool_call with actual MCPAdapt tools. SDK options and model traps are retained in the fixture source.

Arguments are equal across providers. Direct MCP results and SDK projections are different returned layers; their byte counts are listed separately. Exceptions have zero returned projection bytes and their actual message bytes in a separate column. Full projected outputs, exceptions, input/result hashes and independent file goals are retained in JSON. Setup includes SDK import/discovery; warm timing includes the complete tool call. Fixture resets, byte accounting and file verification are outside timing. Three measured samples make nearest-rank p95 the maximum.

Each rejected call requires its fixture's specific remote reason. Direct MCP additionally checks JSON-RPC code -32600; Agents requires its actual McpError, AutoGen its actual named ToolResult text, and smolagents its McpError-bearing SDK exception. Successful reads precede the mutation. Connection, startup and unrelated tool failures fail sampling, and warm-up outcomes pass the same checks.

Binary SHA-256 `b01cc02a39864cb7f460cdd29fc84c8059855229acc6aec97c43d1070505d012`; source `e40dd0573ae4cdcf448f7d44eaaa4ac609e532370fd785946626c2eb4fac4ef7`. SDK versions: `{"mcp": "1.30.0", "mcpadapt": "0.1.20", "smolagents": "1.26.0"}`.

| Scenario | Direct edit median / p95 ms | SDK edit median / p95 ms | Direct / SDK read ms | Direct / SDK projected bytes | Direct / SDK exception bytes | Correct direct / SDK |
|---|---:|---:|---:|---:|---:|---:|
| tiny-config | 0.77 / 0.79 | 3.23 / 3.28 | 0.17 / 0.57 | 288 / 184 | 0 / 0 | 3 / 3 |
| large-local-change | 1.01 / 1.03 | 3.28 / 3.32 | 0.19 / 0.57 | 834 / 730 | 0 / 0 | 3 / 3 |
| multiline-function | 0.80 / 0.85 | 3.10 / 3.57 | 0.17 / 0.54 | 407 / 303 | 0 / 0 | 3 / 3 |
| unicode | 0.75 / 0.79 | 3.08 / 3.19 | 0.18 / 0.52 | 295 / 191 | 0 / 0 | 3 / 3 |
| crlf | 0.77 / 0.77 | 3.12 / 3.20 | 0.16 / 0.50 | 294 / 190 | 0 / 0 | 3 / 3 |
| no-final-newline | 0.81 / 0.94 | 3.03 / 3.06 | 0.17 / 0.53 | 266 / 162 | 0 / 0 | 3 / 3 |
| disambiguated-block | 0.73 / 0.74 | 3.10 / 3.12 | 0.16 / 0.50 | 322 / 218 | 0 / 0 | 3 / 3 |
| global-replace | 0.75 / 0.78 | 3.17 / 3.22 | 0.17 / 0.51 | 624 / 520 | 0 / 0 | 3 / 3 |
| same-file-batch | 2.75 / 6.57 | 4.63 / 4.69 | 0.19 / 0.52 | 280 / 176 | 0 / 0 | 3 / 3 |
| multi-file-batch | 5.00 / 5.07 | 7.02 / 7.27 | 0.48 / 1.39 | 449 / 271 | 0 / 0 | 3 / 3 |
| long-line | 0.85 / 0.94 | 3.21 / 3.38 | 0.21 / 0.54 | 815 / 711 | 0 / 0 | 3 / 3 |
| insert-after-anchor | 0.84 / 1.00 | 3.15 / 3.16 | 0.19 / 0.52 | 276 / 172 | 0 / 0 | 3 / 3 |
| delete-block | 0.79 / 0.89 | 3.16 / 3.53 | 0.18 / 0.52 | 297 / 193 | 0 / 0 | 3 / 3 |
| bom-crlf | 0.75 / 0.77 | 3.02 / 3.24 | 0.19 / 0.53 | 269 / 165 | 0 / 0 | 3 / 3 |
| mixed-eol | 0.73 / 0.74 | 2.96 / 2.98 | 0.16 / 0.48 | 263 / 159 | 0 / 0 | 3 / 3 |
| noop | 0.10 / 0.13 | 2.40 / 2.68 | 0.15 / 0.52 | 237 / 131 | 0 / 0 | 3 / 3 |
| reject-missing | 0.09 / 0.10 | 0.70 / 0.75 | 0.14 / 0.44 | 163 / 15 | 0 / 218 | 3 / 3 |
| reject-ambiguous | 0.10 / 0.11 | 0.61 / 0.61 | 0.14 / 0.42 | 151 / 19 | 0 / 198 | 3 / 3 |
| reject-stale-write | 0.14 / 0.18 | 0.62 / 0.62 | 0.18 / 0.43 | 135 / 19 | 0 / 210 | 3 / 3 |
| preview | 0.12 / 0.14 | 2.36 / 2.45 | 0.15 / 0.47 | 336 / 133 | 0 / 0 | 3 / 3 |
| write-create | 2.04 / 2.05 | 4.31 / 4.51 | 0.00 / 0.00 | 161 / 94 | 0 / 0 | 3 / 3 |
| reject-late-batch | 0.13 / 0.16 | 0.70 / 0.71 | 0.18 / 0.46 | 185 / 15 | 0 / 303 | 3 / 3 |
| create-rename-delete | 2.32 / 2.42 | 4.50 / 4.58 | 0.00 / 0.00 | 195 / 128 | 0 / 0 | 3 / 3 |
| large-65536 | 0.90 / 1.02 | 3.06 / 3.17 | 0.21 / 0.54 | 811 / 707 | 0 / 0 | 3 / 3 |
| large-1048576 | 1.80 / 2.52 | 4.00 / 4.05 | 0.57 / 0.89 | 811 / 707 | 0 / 0 | 3 / 3 |
| large-5242880 | 6.61 / 6.62 | 8.63 / 8.69 | 2.12 / 2.83 | 811 / 707 | 0 / 0 | 3 / 3 |

## Reproduce

```sh
python benches/smolagents_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```
