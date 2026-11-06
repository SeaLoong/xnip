# MCP host profiles and Goose client benchmark

14 generated host profiles; 20 fixtures, 40 lane/fixture groups and 120 measured workflows. Zero model calls.

The shared stdio lane executes the generated command, negotiates real MCP, discovers the actual xnip catalog and calls its file tools. GUI profiles bind to that shared transport lane; these measurements describe the configured local bridge. Codex, Claude Code and Copilot CLI additionally read isolated configurations through their actual management commands. Continue uses its published configuration schema. Goose additionally executes its pinned official McpClient through the existing source-verified fixture. Agent/model loops and GUI rendering remain separate execution layers.

Every sample starts a new MCP connection. Workflow time includes local spawn/connection, discovery, tool calls and graceful disconnect; fixture reset and byte checks are outside timing. Actual client timing is retained for every tool call, including typed remote JSON-RPC rejections. Goose's fixture captures the official client's remote error while retaining every completed prefix call; startup, transport and process failures fail the run. Rejected fixtures require the specific remote error code and guard message. These fields describe different transport boundaries. Warm-up counts come from the raw methodology; with three measured samples nearest-rank p95 is the maximum.

Tool JSON bytes count canonical UTF-8 JSON for the actual public arguments and client result/error objects. The shared lane also retains measured JSONL request/response frame bytes. Goose client DTOs and any GUI/model-visible projection retain separate boundaries; these byte counts describe local calls, rather than billing.

Binary SHA-256 `16cecdbad176790492338643e29480d2dc7793c5cda5629102e0dde434be8deb`; maintained source `41a6a553aa6f3ecbbe3def85088b394d4cbe16c772747fb88418eb1511937ffd`. Python 3.12.14; Goose 1.55.0.

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
| tiny-config | 6.34 / 6.74 | 5.86 / 6.04 | 304 / 352 | 3/3 / 3/3 |
| large-local-change | 8.04 / 9.45 | 7.00 / 7.95 | 6,649 / 6,697 | 3/3 / 3/3 |
| multiline-function | 6.27 / 6.31 | 5.80 / 6.00 | 550 / 598 | 3/3 / 3/3 |
| unicode | 6.20 / 6.50 | 6.02 / 8.75 | 322 / 370 | 3/3 / 3/3 |
| crlf | 6.09 / 6.34 | 5.68 / 5.89 | 296 / 344 | 3/3 / 3/3 |
| no-final-newline | 6.58 / 6.73 | 5.07 / 5.60 | 292 / 340 | 3/3 / 3/3 |
| disambiguated-block | 6.57 / 7.10 | 5.03 / 5.57 | 390 / 438 | 3/3 / 3/3 |
| global-replace | 6.48 / 6.70 | 5.46 / 5.51 | 654 / 702 | 3/3 / 3/3 |
| same-file-batch | 8.27 / 8.74 | 7.47 / 7.54 | 384 / 432 | 3/3 / 3/3 |
| multi-file-batch | 11.31 / 12.54 | 11.62 / 15.24 | 656 / 752 | 3/3 / 3/3 |
| long-line | 16.74 / 17.05 | 6.68 / 6.85 | 40,257 / 40,305 | 3/3 / 3/3 |
| insert-after-anchor | 6.29 / 6.39 | 5.71 / 6.47 | 324 / 372 | 3/3 / 3/3 |
| delete-block | 6.44 / 6.99 | 5.21 / 10.57 | 327 / 375 | 3/3 / 3/3 |
| noop | 5.51 / 5.64 | 4.39 / 4.55 | 213 / 261 | 3/3 / 3/3 |
| reject-stale | 5.49 / 5.52 | 6.89 / 9.99 | 276 / 300 | 3/3 / 3/3 |
| reject-ambiguous | 5.68 / 6.00 | 4.28 / 4.33 | 265 / 289 | 3/3 / 3/3 |
| write-create | 6.21 / 6.32 | 4.77 / 6.11 | 142 / 166 | 3/3 / 3/3 |
| delete | 5.44 / 5.63 | 4.27 / 4.35 | 208 / 256 | 3/3 / 3/3 |
| rename | 7.18 / 8.03 | 6.35 / 7.49 | 250 / 298 | 3/3 / 3/3 |
| reject-late-batch | 5.74 / 5.92 | 6.85 / 6.89 | 581 / 629 | 3/3 / 3/3 |

## Reproduce

```sh
python benches/native/host_profiles_setup.py
python benches/native/goose_adapter.py --setup
python tests/host_profiles_contract.py
python benches/host_profiles.py --executable target/release/xnip --repeats 3 --warmups 1
```

[Host setup and restoration](../../../host-profiles.md). The JSON companion retains actual call inputs/results, schema and binary/source hashes, independent file outcomes, CLI parser evidence and warm-up samples.
