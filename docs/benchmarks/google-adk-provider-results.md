# Google ADK MCP toolset benchmark

Google ADK 2.13.0; 26 fixtures, 52 groups and 156 measured workflows. Zero model calls.

Both providers call the same xnip MCP process with the same public arguments. The direct lane uses ClientSession; the ADK lane uses its actual public McpToolset, prefixed-tool registry, idle LlmAgent/ToolContext and McpTool.run_async. The public pre-model metadata path produces the actual ADK function declarations without requesting model output. ADK confirmation, authentication hooks and result/error projection belong to the SDK. The production integration supplies the public toolset rather than replacing builtin file tools.

One real ClientSession observer delegates the original method in both lanes and retains each actual result or typed remote error. Successful reads must precede the requested mutation, and rejected cases require their specific remote code/message. Startup/transport/process failures fail the experiment. The resulting file set and independent byte hashes must match each fixture, and both lanes must have identical raw RPC outcomes. ADK's projected response remains separate: MCP 2 resultType is removed, and remote errors use its actual SDK error handling.

Timing includes each complete public tool call. Fixture reset, serialized-byte accounting and file checks run outside it. Setup includes SDK construction/discovery, and the first lane includes cold import costs. Each provider keeps one persistent session. Actual JSON payload bytes, rather than model billing, are recorded. Warm-up counts are retained in the JSON methodology; with three measured samples nearest-rank p95 is the maximum.

Binary SHA-256 `b01cc02a39864cb7f460cdd29fc84c8059855229acc6aec97c43d1070505d012`; source `158f6dac24d126ec00ad5e66240a4617263d5878260c44153b2a281296c08042`. Python 3.11.17; MCP 2.3.0.

| Fixture | Direct mutation median / p95 ms | ADK mutation median / p95 ms | Direct / ADK projected JSON bytes | Direct / ADK correct |
|---|---:|---:|---:|---:|
| tiny-config | 1.23 / 1.61 | 1.27 / 1.46 | 368 / 320 | 3/3 / 3/3 |
| large-local-change | 1.29 / 1.34 | 1.36 / 1.37 | 914 / 866 | 3/3 / 3/3 |
| multiline-function | 1.16 / 1.17 | 1.18 / 1.19 | 487 / 439 | 3/3 / 3/3 |
| unicode | 1.19 / 1.19 | 1.23 / 1.25 | 375 / 327 | 3/3 / 3/3 |
| crlf | 1.10 / 1.16 | 1.24 / 1.52 | 374 / 326 | 3/3 / 3/3 |
| no-final-newline | 1.18 / 1.26 | 1.23 / 1.27 | 346 / 298 | 3/3 / 3/3 |
| disambiguated-block | 1.19 / 1.22 | 1.19 / 1.22 | 402 / 354 | 3/3 / 3/3 |
| global-replace | 1.19 / 1.21 | 1.19 / 1.21 | 704 / 656 | 3/3 / 3/3 |
| same-file-batch | 2.90 / 5.23 | 2.72 / 2.84 | 402 / 354 | 3/3 / 3/3 |
| multi-file-batch | 5.44 / 5.66 | 5.24 / 5.34 | 735 / 639 | 3/3 / 3/3 |
| long-line | 1.14 / 1.15 | 1.26 / 1.29 | 895 / 847 | 3/3 / 3/3 |
| insert-after-anchor | 1.13 / 1.14 | 1.20 / 1.33 | 356 / 308 | 3/3 / 3/3 |
| delete-block | 1.14 / 1.17 | 1.21 / 1.25 | 377 / 329 | 3/3 / 3/3 |
| bom-crlf | 1.01 / 1.06 | 1.09 / 1.23 | 349 / 301 | 3/3 / 3/3 |
| mixed-eol | 1.13 / 1.15 | 1.16 / 1.21 | 343 / 295 | 3/3 / 3/3 |
| noop | 0.35 / 0.36 | 0.56 / 0.56 | 317 / 269 | 3/3 / 3/3 |
| reject-missing | 0.26 / 0.27 | 0.43 / 0.44 | 96 / 172 | 3/3 / 3/3 |
| reject-ambiguous | 0.24 / 0.27 | 0.40 / 0.41 | 100 / 160 | 3/3 / 3/3 |
| reject-stale-write | 0.27 / 0.27 | 0.41 / 0.42 | 100 / 144 | 3/3 / 3/3 |
| preview | 0.35 / 0.36 | 0.49 / 0.49 | 416 / 368 | 3/3 / 3/3 |
| write-create | 0.98 / 1.00 | 1.01 / 1.03 | 202 / 178 | 3/3 / 3/3 |
| reject-late-batch | 0.45 / 0.64 | 0.39 / 0.43 | 188 / 313 | 3/3 / 3/3 |
| create-rename-delete | 2.63 / 2.76 | 2.44 / 2.53 | 277 / 253 | 3/3 / 3/3 |
| large-65536 | 1.18 / 1.27 | 1.23 / 1.30 | 891 / 843 | 3/3 / 3/3 |
| large-1048576 | 2.28 / 2.33 | 2.22 / 2.30 | 891 / 843 | 3/3 / 3/3 |
| large-5242880 | 7.68 / 7.80 | 8.08 / 8.50 | 891 / 843 | 3/3 / 3/3 |

## Reproduce

```sh
python benches/native/google_adk_setup.py
target/native-hosts/google-adk-2.13.0/.venv/bin/python benches/google_adk_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```

[SDK integration and lifecycle](../google-adk.md). The raw JSON retains actual arguments/projections/RPC errors, independent file goals, SDK source hashes and setup costs.
