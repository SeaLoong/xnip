# smolagents SDK MCP bridge benchmark

26 scenarios; 3 measured workflows and 1 warm-up per provider/scenario. Zero model calls.

Both lanes execute the same xnip MCP editing engine. The direct lane uses a real JSON-RPC stdio client; the SDK lane invokes the framework's actual public tool pipeline. This measures transport/adaptation overhead and observed SDK result projection. Agent loops, model generation, billed tokens and native builtin replacement have their own boundaries.

OpenAI Agents uses Agent.get_all_tools and FunctionTool.on_invoke_tool; its documented failure_error_function=None setting preserves MCP errors as exceptions. AutoGen constructs an AssistantAgent with the actual McpWorkbench and calls the workbench's public call_tool. smolagents uses ToolCallingAgent.execute_tool_call with actual MCPAdapt tools. SDK options and model traps are retained in the fixture source.

Arguments are equal across providers. Direct MCP results and SDK projections are different returned layers; their byte counts are listed separately. Exceptions have zero returned projection bytes and their actual message bytes in a separate column. Full projected outputs, exceptions, input/result hashes and independent file goals are retained in JSON. Setup includes SDK import/discovery; warm timing includes the complete tool call. Fixture resets, byte accounting and file verification are outside timing. Three measured samples make nearest-rank p95 the maximum.

Each rejected call requires its fixture's specific remote reason. Direct MCP additionally checks JSON-RPC code -32600; Agents requires its actual McpError, AutoGen its actual named ToolResult text, and smolagents its McpError-bearing SDK exception. Successful reads precede the mutation. Connection, startup and unrelated tool failures fail sampling, and warm-up outcomes pass the same checks.

Binary SHA-256 `144f76db0a340d7eede61b7fc2545c8c7c1d7d937300f378fd6c77c8da3e0688`; source `f91bac05145611ee3841e3784e9e58087d358ffe9de599c0fb3ac6b8687999ec`. SDK versions: `{"mcp": "1.30.0", "mcpadapt": "0.1.20", "smolagents": "1.26.0"}`.

| Scenario | Direct edit median / p95 ms | SDK edit median / p95 ms | Direct / SDK read ms | Direct / SDK projected bytes | Direct / SDK exception bytes | Correct direct / SDK |
|---|---:|---:|---:|---:|---:|---:|
| tiny-config | 0.74 / 0.81 | 3.10 / 3.15 | 0.16 / 0.49 | 288 / 184 | 0 / 0 | 3 / 3 |
| large-local-change | 0.87 / 1.04 | 2.92 / 3.14 | 0.18 / 0.47 | 834 / 730 | 0 / 0 | 3 / 3 |
| multiline-function | 0.85 / 0.88 | 2.87 / 2.96 | 0.16 / 0.45 | 407 / 303 | 0 / 0 | 3 / 3 |
| unicode | 0.67 / 0.73 | 3.02 / 3.22 | 0.16 / 0.47 | 295 / 191 | 0 / 0 | 3 / 3 |
| crlf | 0.74 / 0.76 | 3.19 / 3.32 | 0.15 / 0.46 | 294 / 190 | 0 / 0 | 3 / 3 |
| no-final-newline | 1.00 / 1.03 | 3.05 / 3.13 | 0.19 / 0.48 | 266 / 162 | 0 / 0 | 3 / 3 |
| disambiguated-block | 0.83 / 0.98 | 3.01 / 3.23 | 0.27 / 0.47 | 322 / 218 | 0 / 0 | 3 / 3 |
| global-replace | 0.68 / 0.88 | 2.95 / 3.04 | 0.21 / 0.53 | 624 / 520 | 0 / 0 | 3 / 3 |
| same-file-batch | 1.86 / 1.87 | 4.21 / 4.64 | 0.17 / 0.47 | 280 / 176 | 0 / 0 | 3 / 3 |
| multi-file-batch | 3.30 / 3.34 | 5.84 / 6.06 | 0.39 / 1.23 | 449 / 271 | 0 / 0 | 3 / 3 |
| long-line | 0.64 / 0.64 | 3.07 / 3.13 | 0.19 / 0.52 | 815 / 711 | 0 / 0 | 3 / 3 |
| insert-after-anchor | 0.61 / 0.61 | 2.92 / 2.95 | 0.15 / 0.45 | 276 / 172 | 0 / 0 | 3 / 3 |
| delete-block | 0.63 / 0.64 | 3.12 / 3.37 | 0.17 / 0.46 | 297 / 193 | 0 / 0 | 3 / 3 |
| bom-crlf | 0.57 / 0.58 | 2.82 / 2.85 | 0.17 / 0.45 | 269 / 165 | 0 / 0 | 3 / 3 |
| mixed-eol | 0.50 / 0.51 | 2.79 / 2.80 | 0.14 / 0.46 | 263 / 159 | 0 / 0 | 3 / 3 |
| noop | 0.09 / 0.09 | 2.24 / 2.27 | 0.11 / 0.42 | 237 / 131 | 0 / 0 | 3 / 3 |
| reject-missing | 0.10 / 0.11 | 0.60 / 0.62 | 0.15 / 0.43 | 163 / 15 | 0 / 218 | 3 / 3 |
| reject-ambiguous | 0.09 / 0.10 | 0.55 / 0.58 | 0.12 / 0.38 | 151 / 19 | 0 / 198 | 3 / 3 |
| reject-stale-write | 0.10 / 0.13 | 0.54 / 0.58 | 0.13 / 0.36 | 135 / 19 | 0 / 210 | 3 / 3 |
| preview | 0.11 / 0.12 | 2.23 / 2.23 | 0.15 / 0.46 | 336 / 133 | 0 / 0 | 3 / 3 |
| write-create | 1.30 / 1.33 | 3.84 / 3.89 | 0.00 / 0.00 | 161 / 94 | 0 / 0 | 3 / 3 |
| reject-late-batch | 0.12 / 0.16 | 0.65 / 0.65 | 0.14 / 0.41 | 185 / 15 | 0 / 303 | 3 / 3 |
| create-rename-delete | 1.38 / 1.46 | 3.81 / 3.91 | 0.00 / 0.00 | 195 / 128 | 0 / 0 | 3 / 3 |
| large-65536 | 0.74 / 0.74 | 2.96 / 3.07 | 0.21 / 0.53 | 811 / 707 | 0 / 0 | 3 / 3 |
| large-1048576 | 1.55 / 1.81 | 4.58 / 5.89 | 0.58 / 1.21 | 811 / 707 | 0 / 0 | 3 / 3 |
| large-5242880 | 6.80 / 6.90 | 9.72 / 9.91 | 2.22 / 3.07 | 811 / 707 | 0 / 0 | 3 / 3 |

## Reproduce

```sh
python benches/smolagents_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```
