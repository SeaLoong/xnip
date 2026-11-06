# MCP host profiles and Goose client benchmark

14 generated host profiles; 20 fixtures, 40 lane/fixture groups and 120 measured workflows. Zero model calls.

The shared stdio lane executes the generated command, negotiates real MCP, discovers the actual xnip catalog and calls its file tools. GUI profiles bind to that shared transport lane; these measurements describe the configured local bridge. Codex, Claude Code and Copilot CLI additionally read isolated configurations through their actual management commands. Continue uses its published configuration schema. Goose additionally executes its pinned official McpClient through the existing source-verified fixture. Agent/model loops and GUI rendering remain separate execution layers.

Every sample starts a new MCP connection. Workflow time includes local spawn/connection, discovery, tool calls and graceful disconnect; fixture reset and byte checks are outside timing. Actual client timing is retained for every tool call, including typed remote JSON-RPC rejections. Goose's fixture captures the official client's remote error while retaining every completed prefix call; startup, transport and process failures fail the run. Rejected fixtures require the specific remote error code and guard message. These fields describe different transport boundaries. Warm-up counts come from the raw methodology; with three measured samples nearest-rank p95 is the maximum.

Tool JSON bytes count canonical UTF-8 JSON for the actual public arguments and client result/error objects. The shared lane also retains measured JSONL request/response frame bytes. Goose client DTOs and any GUI/model-visible projection retain separate boundaries; these byte counts describe local calls, rather than billing.

Binary SHA-256 `b01cc02a39864cb7f460cdd29fc84c8059855229acc6aec97c43d1070505d012`; maintained source `6589f1698c59e9710c5f5d64114feb56901564cc52c6c036a067e8a9e313717a`. Python 3.12.14; Goose 1.55.0.

| Profile | Configuration evidence | Measured lane |
|---|---|---|
| codex | official CLI config reader | shared-stdio-mcp |
| claude-code | official CLI project-config reader | shared-stdio-mcp |
| cursor | generated configuration + actual stdio discovery/edit | shared-stdio-mcp |
| vscode | generated configuration + actual stdio discovery/edit | shared-stdio-mcp |
| copilot | generated configuration + actual stdio discovery/edit | shared-stdio-mcp |
| copilot-cli | official CLI config reader | shared-stdio-mcp |
| gemini-cli | generated configuration + actual stdio discovery/edit | shared-stdio-mcp |
| qwen-code | generated configuration + actual stdio discovery/edit | shared-stdio-mcp |
| roo-code | generated configuration + actual stdio discovery/edit | shared-stdio-mcp |
| cline-gui | generated configuration + actual stdio discovery/edit | shared-stdio-mcp |
| windsurf | generated configuration + actual stdio discovery/edit | shared-stdio-mcp |
| goose | generated configuration + actual stdio discovery/edit | shared-stdio-mcp; goose-official-client |
| continue | official published configuration schema | shared-stdio-mcp |
| zed | generated configuration + actual stdio discovery/edit | shared-stdio-mcp |

| Fixture | Shared workflow median / p95 ms | Goose workflow median / p95 ms | Shared / Goose tool JSON bytes | Shared / Goose correct |
|---|---:|---:|---:|---:|
| tiny-config | 6.60 / 6.61 | 5.20 / 5.43 | 304 / 352 | 3/3 / 3/3 |
| large-local-change | 8.35 / 8.70 | 6.03 / 6.08 | 6,649 / 6,697 | 3/3 / 3/3 |
| multiline-function | 6.86 / 6.90 | 5.61 / 5.72 | 550 / 598 | 3/3 / 3/3 |
| unicode | 6.55 / 6.73 | 5.69 / 5.80 | 322 / 370 | 3/3 / 3/3 |
| crlf | 6.48 / 6.51 | 4.89 / 5.09 | 296 / 344 | 3/3 / 3/3 |
| no-final-newline | 6.57 / 6.65 | 5.23 / 5.47 | 292 / 340 | 3/3 / 3/3 |
| disambiguated-block | 6.61 / 6.82 | 5.18 / 5.22 | 390 / 438 | 3/3 / 3/3 |
| global-replace | 6.89 / 6.94 | 5.07 / 5.22 | 654 / 702 | 3/3 / 3/3 |
| same-file-batch | 8.49 / 8.61 | 7.24 / 7.66 | 384 / 432 | 3/3 / 3/3 |
| multi-file-batch | 11.19 / 11.74 | 10.88 / 12.20 | 656 / 752 | 3/3 / 3/3 |
| long-line | 16.70 / 16.92 | 6.50 / 6.54 | 40,257 / 40,305 | 3/3 / 3/3 |
| insert-after-anchor | 6.58 / 6.97 | 5.79 / 5.83 | 324 / 372 | 3/3 / 3/3 |
| delete-block | 6.93 / 7.02 | 5.26 / 5.66 | 327 / 375 | 3/3 / 3/3 |
| noop | 5.47 / 5.61 | 4.42 / 4.72 | 213 / 261 | 3/3 / 3/3 |
| reject-stale | 5.76 / 6.19 | 4.19 / 4.64 | 276 / 300 | 3/3 / 3/3 |
| reject-ambiguous | 5.50 / 5.54 | 4.21 / 4.23 | 265 / 289 | 3/3 / 3/3 |
| write-create | 6.72 / 7.05 | 4.75 / 5.16 | 142 / 166 | 3/3 / 3/3 |
| delete | 5.52 / 6.28 | 4.70 / 4.75 | 208 / 256 | 3/3 / 3/3 |
| rename | 8.60 / 9.56 | 7.50 / 8.43 | 250 / 298 | 3/3 / 3/3 |
| reject-late-batch | 8.08 / 8.28 | 4.86 / 5.20 | 581 / 629 | 3/3 / 3/3 |

## Reproduce

```sh
python benches/native/host_profiles_setup.py
python benches/native/goose_adapter.py --setup
python tests/host_profiles_contract.py
python benches/host_profiles.py --executable target/release/xnip --repeats 3 --warmups 1
```

[Host setup and restoration](../host-profiles.md). The JSON companion retains actual call inputs/results, schema and binary/source hashes, independent file outcomes, CLI parser evidence and warm-up samples.
