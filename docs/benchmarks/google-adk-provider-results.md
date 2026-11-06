# Google ADK MCP toolset benchmark

Google ADK 2.13.0; 26 fixtures, 52 groups and 156 measured workflows. Zero model calls.

Both providers call the same xnip MCP process with the same public arguments. The direct lane uses ClientSession; the ADK lane uses its actual public McpToolset, prefixed-tool registry, idle LlmAgent/ToolContext and McpTool.run_async. The public pre-model metadata path produces the actual ADK function declarations without requesting model output. ADK confirmation, authentication hooks and result/error projection belong to the SDK. The production integration supplies the public toolset rather than replacing builtin file tools.

One real ClientSession observer delegates the original method in both lanes and retains each actual result or typed remote error. Successful reads must precede the requested mutation, and rejected cases require their specific remote code/message. Startup/transport/process failures fail the experiment. The resulting file set and independent byte hashes must match each fixture, and both lanes must have identical raw RPC outcomes. ADK's projected response remains separate: MCP 2 resultType is removed, and remote errors use its actual SDK error handling.

Timing includes each complete public tool call. Fixture reset, serialized-byte accounting and file checks run outside it. Setup includes SDK construction/discovery, and the first lane includes cold import costs. Each provider keeps one persistent session. Actual JSON payload bytes, rather than model billing, are recorded. Warm-up counts are retained in the JSON methodology; with three measured samples nearest-rank p95 is the maximum.

Binary SHA-256 `144f76db0a340d7eede61b7fc2545c8c7c1d7d937300f378fd6c77c8da3e0688`; source `5489a81f54f73e71dc514d49f65570610572f607a8af7aef7ca751d0d1453fe5`. Python 3.11.17; MCP 2.3.0.

| Fixture | Direct mutation median / p95 ms | ADK mutation median / p95 ms | Direct / ADK projected JSON bytes | Direct / ADK correct |
|---|---:|---:|---:|---:|
| tiny-config | 1.30 / 1.42 | 1.36 / 1.41 | 368 / 320 | 3/3 / 3/3 |
| large-local-change | 1.35 / 1.41 | 1.42 / 1.68 | 914 / 866 | 3/3 / 3/3 |
| multiline-function | 1.25 / 1.34 | 1.35 / 1.35 | 487 / 439 | 3/3 / 3/3 |
| unicode | 0.99 / 1.11 | 1.35 / 1.46 | 375 / 327 | 3/3 / 3/3 |
| crlf | 0.94 / 0.97 | 1.30 / 1.33 | 374 / 326 | 3/3 / 3/3 |
| no-final-newline | 1.23 / 1.24 | 1.36 / 1.38 | 346 / 298 | 3/3 / 3/3 |
| disambiguated-block | 1.22 / 1.23 | 1.30 / 1.32 | 402 / 354 | 3/3 / 3/3 |
| global-replace | 1.21 / 1.23 | 1.32 / 1.32 | 704 / 656 | 3/3 / 3/3 |
| same-file-batch | 2.84 / 2.92 | 3.00 / 3.01 | 402 / 354 | 3/3 / 3/3 |
| multi-file-batch | 5.46 / 5.84 | 4.71 / 4.80 | 735 / 639 | 3/3 / 3/3 |
| long-line | 1.08 / 1.09 | 1.34 / 1.57 | 895 / 847 | 3/3 / 3/3 |
| insert-after-anchor | 1.09 / 1.11 | 1.32 / 1.34 | 356 / 308 | 3/3 / 3/3 |
| delete-block | 1.21 / 1.23 | 1.25 / 1.25 | 377 / 329 | 3/3 / 3/3 |
| bom-crlf | 1.09 / 1.16 | 1.12 / 1.22 | 349 / 301 | 3/3 / 3/3 |
| mixed-eol | 1.12 / 1.16 | 1.17 / 1.17 | 343 / 295 | 3/3 / 3/3 |
| noop | 0.35 / 0.36 | 0.46 / 0.48 | 317 / 269 | 3/3 / 3/3 |
| reject-missing | 0.27 / 0.28 | 0.38 / 0.39 | 96 / 172 | 3/3 / 3/3 |
| reject-ambiguous | 0.24 / 0.24 | 0.39 / 0.41 | 100 / 160 | 3/3 / 3/3 |
| reject-stale-write | 0.26 / 0.28 | 0.39 / 0.39 | 100 / 144 | 3/3 / 3/3 |
| preview | 0.37 / 0.39 | 0.49 / 0.49 | 416 / 368 | 3/3 / 3/3 |
| write-create | 1.03 / 1.06 | 1.16 / 1.17 | 202 / 178 | 3/3 / 3/3 |
| reject-late-batch | 0.28 / 0.41 | 0.39 / 0.49 | 188 / 313 | 3/3 / 3/3 |
| create-rename-delete | 2.67 / 2.67 | 2.58 / 2.68 | 277 / 253 | 3/3 / 3/3 |
| large-65536 | 1.25 / 1.27 | 1.34 / 1.36 | 891 / 843 | 3/3 / 3/3 |
| large-1048576 | 2.29 / 2.40 | 2.42 / 2.48 | 891 / 843 | 3/3 / 3/3 |
| large-5242880 | 7.82 / 7.95 | 8.43 / 8.66 | 891 / 843 | 3/3 / 3/3 |

## Reproduce

```sh
python benches/native/google_adk_setup.py
target/native-hosts/google-adk-2.13.0/.venv/bin/python benches/google_adk_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```

[SDK integration and lifecycle](../google-adk.md). The raw JSON retains actual arguments/projections/RPC errors, independent file goals, SDK source hashes and setup costs.
