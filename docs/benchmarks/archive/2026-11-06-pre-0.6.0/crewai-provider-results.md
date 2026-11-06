# CrewAI MCP tool benchmark

CrewAI 1.15.36; 26 fixtures, 52 groups and 156 measured workflows. Zero model calls.

The SDK lane uses the actual MCPToolResolver, MCPNativeTool, Agent registry and public BaseTool.run. The direct lane uses public MCP ClientSession. Each invocation starts and closes a new MCP connection in both lanes, matching CrewAI's own native tool lifecycle. Initial discovery/Agent registration is recorded separately. This integration supplies MCP tools through CrewAI's public resolver.

Every real MCP outcome is retained through an observer that delegates the original SDK method. Reads must succeed before their requested mutation. Rejected requests require the expected typed remote error and message; startup, transport or SDK failures fail the experiment. CrewAI's retry policy remains active: ambiguous edits may have several real remote attempts and their backoff remains inside timing. All resulting files, absences and byte hashes must match the independent fixture oracle. Direct and CrewAI final RPC outcomes must agree.

Timing includes complete public tool calls, fresh process creation, initialize, SDK argument/result processing, retry policy and connection closure. Fixture reset, byte accounting and file validation run outside it. Counts describe serialized JSON payloads and projected tool text rather than model billing. Each warm-up is retained; with three measured samples, nearest-rank p95 is the maximum.

Binary SHA-256 `16cecdbad176790492338643e29480d2dc7793c5cda5629102e0dde434be8deb`; source `808679240d107cf51a5534bd10fb33d1ded6b3191438f5abb48f4216df4bd6a7`. Python 3.12.14; MCP 1.30.0.

| Fixture | Direct mutation median / p95 ms | CrewAI mutation median / p95 ms | Direct / CrewAI RPC attempts | Direct / CrewAI correct |
|---|---:|---:|---:|---:|
| tiny-config | 4.87 / 5.29 | 6.25 / 6.68 | 2 / 2 | 3/3 / 3/3 |
| large-local-change | 5.04 / 5.06 | 6.65 / 7.19 | 2 / 2 | 3/3 / 3/3 |
| multiline-function | 4.31 / 4.86 | 6.41 / 6.43 | 2 / 2 | 3/3 / 3/3 |
| unicode | 4.69 / 4.73 | 6.81 / 6.86 | 2 / 2 | 3/3 / 3/3 |
| crlf | 4.66 / 5.11 | 6.35 / 6.84 | 2 / 2 | 3/3 / 3/3 |
| no-final-newline | 5.00 / 5.36 | 6.74 / 7.02 | 2 / 2 | 3/3 / 3/3 |
| disambiguated-block | 5.00 / 5.04 | 6.43 / 6.53 | 2 / 2 | 3/3 / 3/3 |
| global-replace | 4.55 / 4.79 | 6.41 / 6.98 | 2 / 2 | 3/3 / 3/3 |
| same-file-batch | 6.42 / 6.68 | 7.76 / 7.81 | 2 / 2 | 3/3 / 3/3 |
| multi-file-batch | 8.57 / 8.76 | 10.28 / 11.16 | 4 / 4 | 3/3 / 3/3 |
| long-line | 4.84 / 5.14 | 6.51 / 6.90 | 2 / 2 | 3/3 / 3/3 |
| insert-after-anchor | 4.53 / 6.13 | 6.16 / 6.58 | 2 / 2 | 3/3 / 3/3 |
| delete-block | 4.54 / 4.66 | 6.22 / 6.87 | 2 / 2 | 3/3 / 3/3 |
| noop | 3.68 / 3.82 | 5.35 / 5.79 | 2 / 2 | 3/3 / 3/3 |
| reject-stale | 3.06 / 3.68 | 5.15 / 5.56 | 2 / 2 | 3/3 / 3/3 |
| reject-ambiguous | 3.41 / 3.42 | 3012.27 / 3013.95 | 2 / 4 | 3/3 / 3/3 |
| write-create | 4.99 / 5.13 | 6.24 / 6.76 | 1 / 1 | 3/3 / 3/3 |
| delete | 4.24 / 4.31 | 5.90 / 6.61 | 2 / 2 | 3/3 / 3/3 |
| rename | 5.58 / 7.52 | 8.02 / 8.46 | 2 / 2 | 3/3 / 3/3 |
| reject-late-batch | 3.18 / 3.42 | 5.31 / 5.58 | 3 / 3 | 3/3 / 3/3 |
| bom-crlf | 4.19 / 4.93 | 6.44 / 6.56 | 2 / 2 | 3/3 / 3/3 |
| mixed-eol | 4.25 / 6.73 | 6.29 / 6.39 | 2 / 2 | 3/3 / 3/3 |
| preview | 3.76 / 3.79 | 5.66 / 6.06 | 2 / 2 | 3/3 / 3/3 |
| large-65536 | 4.49 / 4.56 | 7.43 / 7.51 | 2 / 2 | 3/3 / 3/3 |
| large-1048576 | 6.56 / 6.67 | 8.85 / 9.22 | 2 / 2 | 3/3 / 3/3 |
| large-5242880 | 14.11 / 16.30 | 16.98 / 17.50 | 2 / 2 | 3/3 / 3/3 |

## Reproduce

```sh
python benches/native/crewai_setup.py
target/native-hosts/crewai-1.15.36/.venv/bin/python benches/crewai_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```

[SDK integration, cache and lifecycle](../../../crewai.md). Raw JSON includes actual arguments, projections, typed errors, per-attempt results, SDK source hashes, setup and independent file goals.
