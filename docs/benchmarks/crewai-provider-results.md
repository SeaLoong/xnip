# CrewAI MCP tool benchmark

CrewAI 1.15.36; 26 fixtures, 52 groups and 156 measured workflows. Zero model calls.

The SDK lane uses the actual MCPToolResolver, MCPNativeTool, Agent registry and public BaseTool.run. The direct lane uses public MCP ClientSession. Each invocation starts and closes a new MCP connection in both lanes, matching CrewAI's own native tool lifecycle. Initial discovery/Agent registration is recorded separately. This integration supplies MCP tools through CrewAI's public resolver.

Every real MCP outcome is retained through an observer that delegates the original SDK method. Reads must succeed before their requested mutation. Rejected requests require the expected typed remote error and message; startup, transport or SDK failures fail the experiment. CrewAI's retry policy remains active: ambiguous edits may have several real remote attempts and their backoff remains inside timing. All resulting files, absences and byte hashes must match the independent fixture oracle. Direct and CrewAI final RPC outcomes must agree.

Timing includes complete public tool calls, fresh process creation, initialize, SDK argument/result processing, retry policy and connection closure. Fixture reset, byte accounting and file validation run outside it. Counts describe serialized JSON payloads and projected tool text rather than model billing. Each warm-up is retained; with three measured samples, nearest-rank p95 is the maximum.

Binary SHA-256 `b01cc02a39864cb7f460cdd29fc84c8059855229acc6aec97c43d1070505d012`; source `1f2d5fc6690ee7f26e8f8d4ad1353c9d0a8697902322882d20f38f5fc87d97cc`. Python 3.12.14; MCP 1.30.0.

| Fixture | Direct mutation median / p95 ms | CrewAI mutation median / p95 ms | Direct / CrewAI RPC attempts | Direct / CrewAI correct |
|---|---:|---:|---:|---:|
| tiny-config | 4.67 / 4.76 | 6.46 / 8.00 | 2 / 2 | 3/3 / 3/3 |
| large-local-change | 4.77 / 4.86 | 7.36 / 7.64 | 2 / 2 | 3/3 / 3/3 |
| multiline-function | 4.42 / 4.64 | 7.07 / 7.22 | 2 / 2 | 3/3 / 3/3 |
| unicode | 4.42 / 4.44 | 7.35 / 7.60 | 2 / 2 | 3/3 / 3/3 |
| crlf | 4.36 / 4.56 | 7.21 / 7.26 | 2 / 2 | 3/3 / 3/3 |
| no-final-newline | 4.50 / 4.69 | 7.19 / 7.82 | 2 / 2 | 3/3 / 3/3 |
| disambiguated-block | 4.63 / 5.19 | 7.08 / 7.17 | 2 / 2 | 3/3 / 3/3 |
| global-replace | 4.60 / 4.75 | 7.10 / 7.32 | 2 / 2 | 3/3 / 3/3 |
| same-file-batch | 6.52 / 6.64 | 8.20 / 8.75 | 2 / 2 | 3/3 / 3/3 |
| multi-file-batch | 7.58 / 7.64 | 10.57 / 10.61 | 4 / 4 | 3/3 / 3/3 |
| long-line | 4.46 / 4.66 | 7.01 / 7.28 | 2 / 2 | 3/3 / 3/3 |
| insert-after-anchor | 4.22 / 4.28 | 7.48 / 7.75 | 2 / 2 | 3/3 / 3/3 |
| delete-block | 4.45 / 4.54 | 7.56 / 7.63 | 2 / 2 | 3/3 / 3/3 |
| noop | 3.59 / 3.67 | 6.67 / 7.78 | 2 / 2 | 3/3 / 3/3 |
| reject-stale | 2.94 / 2.98 | 6.19 / 6.70 | 2 / 2 | 3/3 / 3/3 |
| reject-ambiguous | 3.00 / 3.02 | 3014.73 / 3016.16 | 2 / 4 | 3/3 / 3/3 |
| write-create | 4.29 / 4.61 | 8.09 / 8.31 | 1 / 1 | 3/3 / 3/3 |
| delete | 3.64 / 3.66 | 6.55 / 6.64 | 2 / 2 | 3/3 / 3/3 |
| rename | 5.37 / 5.38 | 8.52 / 8.74 | 2 / 2 | 3/3 / 3/3 |
| reject-late-batch | 3.13 / 3.43 | 6.84 / 8.38 | 3 / 3 | 3/3 / 3/3 |
| bom-crlf | 4.62 / 5.67 | 7.12 / 7.60 | 2 / 2 | 3/3 / 3/3 |
| mixed-eol | 4.38 / 4.87 | 7.40 / 7.45 | 2 / 2 | 3/3 / 3/3 |
| preview | 3.71 / 4.03 | 5.90 / 6.26 | 2 / 2 | 3/3 / 3/3 |
| large-65536 | 4.99 / 5.35 | 7.30 / 7.42 | 2 / 2 | 3/3 / 3/3 |
| large-1048576 | 6.39 / 6.73 | 9.61 / 9.66 | 2 / 2 | 3/3 / 3/3 |
| large-5242880 | 14.39 / 14.72 | 15.73 / 15.89 | 2 / 2 | 3/3 / 3/3 |

## Reproduce

```sh
python benches/native/crewai_setup.py
target/native-hosts/crewai-1.15.36/.venv/bin/python benches/crewai_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```

[SDK integration, cache and lifecycle](../crewai.md). Raw JSON includes actual arguments, projections, typed errors, per-attempt results, SDK source hashes, setup and independent file goals.
