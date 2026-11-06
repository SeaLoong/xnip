# CrewAI MCP tool benchmark

CrewAI 1.15.36; 26 fixtures, 52 groups and 156 measured workflows. Zero model calls.

The SDK lane uses the actual MCPToolResolver, MCPNativeTool, Agent registry and public BaseTool.run. The direct lane uses public MCP ClientSession. Each invocation starts and closes a new MCP connection in both lanes, matching CrewAI's own native tool lifecycle. Initial discovery/Agent registration is recorded separately. This integration supplies MCP tools through CrewAI's public resolver.

Every real MCP outcome is retained through an observer that delegates the original SDK method. Reads must succeed before their requested mutation. Rejected requests require the expected typed remote error and message; startup, transport or SDK failures fail the experiment. CrewAI's retry policy remains active: ambiguous edits may have several real remote attempts and their backoff remains inside timing. All resulting files, absences and byte hashes must match the independent fixture oracle. Direct and CrewAI final RPC outcomes must agree.

Timing includes complete public tool calls, fresh process creation, initialize, SDK argument/result processing, retry policy and connection closure. Fixture reset, byte accounting and file validation run outside it. Counts describe serialized JSON payloads and projected tool text rather than model billing. Each warm-up is retained; with three measured samples, nearest-rank p95 is the maximum.

Binary SHA-256 `144f76db0a340d7eede61b7fc2545c8c7c1d7d937300f378fd6c77c8da3e0688`; source `68f77db2dc03eca67993b4980dac9aec710982c1091ad047054a2750c5384248`. Python 3.12.14; MCP 1.30.0.

| Fixture | Direct mutation median / p95 ms | CrewAI mutation median / p95 ms | Direct / CrewAI RPC attempts | Direct / CrewAI correct |
|---|---:|---:|---:|---:|
| tiny-config | 4.20 / 4.36 | 6.39 / 6.46 | 2 / 2 | 3/3 / 3/3 |
| large-local-change | 4.31 / 4.35 | 6.74 / 7.32 | 2 / 2 | 3/3 / 3/3 |
| multiline-function | 4.08 / 4.11 | 6.40 / 6.71 | 2 / 2 | 3/3 / 3/3 |
| unicode | 4.23 / 4.24 | 5.98 / 6.69 | 2 / 2 | 3/3 / 3/3 |
| crlf | 4.23 / 4.26 | 6.04 / 6.46 | 2 / 2 | 3/3 / 3/3 |
| no-final-newline | 4.14 / 4.22 | 6.57 / 6.72 | 2 / 2 | 3/3 / 3/3 |
| disambiguated-block | 4.04 / 4.15 | 5.77 / 6.63 | 2 / 2 | 3/3 / 3/3 |
| global-replace | 4.18 / 4.22 | 6.80 / 7.11 | 2 / 2 | 3/3 / 3/3 |
| same-file-batch | 5.41 / 5.73 | 7.50 / 7.85 | 2 / 2 | 3/3 / 3/3 |
| multi-file-batch | 7.74 / 8.18 | 10.10 / 10.16 | 4 / 4 | 3/3 / 3/3 |
| long-line | 4.25 / 4.40 | 6.94 / 7.06 | 2 / 2 | 3/3 / 3/3 |
| insert-after-anchor | 4.26 / 4.28 | 6.81 / 6.81 | 2 / 2 | 3/3 / 3/3 |
| delete-block | 4.18 / 4.26 | 6.93 / 6.94 | 2 / 2 | 3/3 / 3/3 |
| noop | 3.29 / 3.38 | 6.15 / 6.17 | 2 / 2 | 3/3 / 3/3 |
| reject-stale | 2.83 / 2.83 | 5.50 / 6.03 | 2 / 2 | 3/3 / 3/3 |
| reject-ambiguous | 2.74 / 2.79 | 3012.42 / 3012.62 | 2 / 4 | 3/3 / 3/3 |
| write-create | 4.73 / 5.78 | 6.67 / 7.38 | 1 / 1 | 3/3 / 3/3 |
| delete | 3.41 / 3.43 | 5.99 / 6.01 | 2 / 2 | 3/3 / 3/3 |
| rename | 5.02 / 5.27 | 7.60 / 8.11 | 2 / 2 | 3/3 / 3/3 |
| reject-late-batch | 2.84 / 2.85 | 5.50 / 5.57 | 3 / 3 | 3/3 / 3/3 |
| bom-crlf | 4.00 / 4.34 | 6.11 / 6.67 | 2 / 2 | 3/3 / 3/3 |
| mixed-eol | 3.94 / 4.03 | 6.65 / 7.39 | 2 / 2 | 3/3 / 3/3 |
| preview | 3.47 / 3.55 | 5.91 / 6.27 | 2 / 2 | 3/3 / 3/3 |
| large-65536 | 4.21 / 4.52 | 6.88 / 6.92 | 2 / 2 | 3/3 / 3/3 |
| large-1048576 | 5.90 / 5.94 | 8.42 / 8.96 | 2 / 2 | 3/3 / 3/3 |
| large-5242880 | 13.73 / 14.16 | 17.24 / 17.51 | 2 / 2 | 3/3 / 3/3 |

## Reproduce

```sh
python benches/native/crewai_setup.py
target/native-hosts/crewai-1.15.36/.venv/bin/python benches/crewai_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```

[SDK integration, cache and lifecycle](../../../crewai.md). Raw JSON includes actual arguments, projections, typed errors, per-attempt results, SDK source hashes, setup and independent file goals.
