# MCP host profiles and Goose client benchmark

14 generated host profiles; 20 fixtures, 40 lane/fixture groups and 120 measured workflows. Zero model calls.

The shared stdio lane executes the generated command, negotiates real MCP, discovers the actual xnip catalog and calls its file tools. GUI profiles bind to that shared transport lane; these measurements describe the configured local bridge. Codex, Claude Code and Copilot CLI additionally read isolated configurations through their actual management commands. Continue uses its published configuration schema. Goose additionally executes its pinned official McpClient through the existing source-verified fixture. Agent/model loops and GUI rendering remain separate execution layers.

Every sample starts a new MCP connection. Workflow time includes local spawn/connection, discovery, tool calls and graceful disconnect; fixture reset and byte checks are outside timing. Actual client timing is retained for every tool call, including typed remote JSON-RPC rejections. Goose's fixture captures the official client's remote error while retaining every completed prefix call; startup, transport and process failures fail the run. Rejected fixtures require the specific remote error code and guard message. These fields describe different transport boundaries. Warm-up counts come from the raw methodology; with three measured samples nearest-rank p95 is the maximum.

Tool JSON bytes count canonical UTF-8 JSON for the actual public arguments and client result/error objects. The shared lane also retains measured JSONL request/response frame bytes. Goose client DTOs and any GUI/model-visible projection retain separate boundaries; these byte counts describe local calls, rather than billing.

Binary SHA-256 `144f76db0a340d7eede61b7fc2545c8c7c1d7d937300f378fd6c77c8da3e0688`; maintained source `41019f387dcee5d797baa93cbc127575dda339d9f4cf5b683c75ebb1c25ee06a`. Python 3.12.14; Goose 1.55.0.

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
| tiny-config | 6.38 / 6.60 | 5.17 / 5.26 | 304 / 352 | 3/3 / 3/3 |
| large-local-change | 7.99 / 8.15 | 5.49 / 5.55 | 6,649 / 6,697 | 3/3 / 3/3 |
| multiline-function | 6.15 / 6.27 | 4.93 / 4.98 | 550 / 598 | 3/3 / 3/3 |
| unicode | 6.14 / 6.18 | 5.09 / 5.52 | 322 / 370 | 3/3 / 3/3 |
| crlf | 6.14 / 6.14 | 5.18 / 5.58 | 296 / 344 | 3/3 / 3/3 |
| no-final-newline | 6.22 / 6.47 | 5.27 / 5.73 | 292 / 340 | 3/3 / 3/3 |
| disambiguated-block | 6.18 / 6.31 | 5.21 / 5.52 | 390 / 438 | 3/3 / 3/3 |
| global-replace | 6.29 / 6.37 | 5.44 / 5.61 | 654 / 702 | 3/3 / 3/3 |
| same-file-batch | 7.19 / 7.92 | 6.77 / 6.92 | 384 / 432 | 3/3 / 3/3 |
| multi-file-batch | 10.06 / 10.61 | 8.80 / 9.33 | 656 / 752 | 3/3 / 3/3 |
| long-line | 16.14 / 17.50 | 6.44 / 6.78 | 40,257 / 40,305 | 3/3 / 3/3 |
| insert-after-anchor | 6.21 / 6.29 | 5.10 / 7.31 | 324 / 372 | 3/3 / 3/3 |
| delete-block | 6.21 / 6.30 | 5.17 / 5.31 | 327 / 375 | 3/3 / 3/3 |
| noop | 5.37 / 5.42 | 4.15 / 4.16 | 213 / 261 | 3/3 / 3/3 |
| reject-stale | 5.38 / 5.38 | 4.25 / 4.32 | 276 / 300 | 3/3 / 3/3 |
| reject-ambiguous | 5.34 / 5.58 | 4.30 / 4.61 | 265 / 289 | 3/3 / 3/3 |
| write-create | 6.25 / 6.38 | 5.05 / 5.34 | 142 / 166 | 3/3 / 3/3 |
| delete | 5.38 / 5.50 | 4.46 / 4.57 | 208 / 256 | 3/3 / 3/3 |
| rename | 7.11 / 7.29 | 6.34 / 6.60 | 250 / 298 | 3/3 / 3/3 |
| reject-late-batch | 5.82 / 5.88 | 4.66 / 5.20 | 581 / 629 | 3/3 / 3/3 |

## Reproduce

```sh
python benches/native/host_profiles_setup.py
python benches/native/goose_adapter.py --setup
python tests/host_profiles_contract.py
python benches/host_profiles.py --executable target/release/xnip --repeats 3 --warmups 1
```

[Host setup and restoration](../../../host-profiles.md). The JSON companion retains actual call inputs/results, schema and binary/source hashes, independent file outcomes, CLI parser evidence and warm-up samples.
