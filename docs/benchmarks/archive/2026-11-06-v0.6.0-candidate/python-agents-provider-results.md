# Python agent SDK tool benchmarks

DeepAgents 0.7.34, LangGraph 1.2.20 and PydanticAI 2.66.0. 23 fixtures; 3 measured runs and 1 warm-up per fixture/provider. Zero model calls.

DeepAgents compares its original FilesystemBackend and xnip regular-file persistence through the same official FilesystemMiddleware tools, permissions wrapper and compiled LangGraph ToolNode. Matching, schemas and rendered results remain official SDK behavior. Both lanes disable large-result eviction during this file-tool cohort; ordinary applications retain their middleware policy.

LangGraph provides a tool registry and graph executor rather than a builtin file editor. Its lane executes the registered core-backed StructuredTools through a real StateGraph and ToolNode. PydanticAI executes its official MCPToolset.call_tool path against a real xnip stdio server. Their lanes measure supported SDK tool pipelines without inventing an original editor baseline.

The original thirteen byte fixtures remain unchanged. DeepAgents' edit policy uses Python universal newlines and its official string-replacement helper, so its host-specific oracle normalizes edited text to LF; the original literal byte goal is retained independently. Other hosts use xnip literal byte semantics. Complete expected files, unexpected files and SDK receipts/errors have independent oracles. Raw JSON retains actual argument objects and mutation results alongside their hashes; missing/ambiguous errors must have their specific cause, and successful receipts distinguish writes, no-ops and replacement counts.

Warm timing includes the complete SDK tool call. Fixture setup, file hashing and JSON accounting occur outside timing. SDK startup is reported separately. Payload counts cover actual SDK tool arguments and ToolMessage.content or the PydanticAI tool result; thrown exception messages have a separate byte field. Identity/status envelopes and generated message IDs are separate. Raw samples also retain actual native JSONL byte counters where that client is owned by the adapter; MCP transport-envelope bytes remain unknown. These measurements describe tool execution and local payloads, with zero agent/model generation or billing estimates.

Binary SHA-256 `144f76db0a340d7eede61b7fc2545c8c7c1d7d937300f378fd6c77c8da3e0688` (release); maintained source `19e5647f015a4c36f3f6598c27c809cec5b0ab3761bade3bc3a9d0b9fe06f248`. Raw JSON retains package versions, source fingerprints and all measured samples. With three samples nearest-rank p95 is the maximum.

| Framework | Provider | SDK/session setup ms |
|---|---|---:|
| deepagents | filesystem-original | 60.78 |
| deepagents | xnip-native | 4.32 |
| langgraph | xnip-tools | 7.73 |
| pydantic-ai | xnip-tools | 685.88 |

| Framework | Provider | Fixture | Mutation median / p95 ms | Read median ms | SDK tool bytes | Host goal+outcome / literal goal |
|---|---|---|---:|---:|---:|---:|
| deepagents | filesystem-original | tiny-config | 1.30 / 1.42 | 1.35 | 330 | 3/3 / 3/3 |
| deepagents | filesystem-original | large-local-change | 1.38 / 1.38 | 1.44 | 2,649 | 3/3 / 3/3 |
| deepagents | filesystem-original | multiline-function | 1.32 / 1.89 | 1.33 | 551 | 3/3 / 3/3 |
| deepagents | filesystem-original | unicode | 1.39 / 1.45 | 1.31 | 352 | 3/3 / 3/3 |
| deepagents | filesystem-original | crlf | 1.45 / 1.51 | 1.32 | 309 | 3/3 / 0/3 |
| deepagents | filesystem-original | no-final-newline | 1.25 / 1.28 | 1.24 | 331 | 3/3 / 3/3 |
| deepagents | filesystem-original | disambiguated-block | 1.30 / 1.37 | 1.28 | 408 | 3/3 / 3/3 |
| deepagents | filesystem-original | global-replace | 1.25 / 1.31 | 1.27 | 538 | 3/3 / 3/3 |
| deepagents | filesystem-original | same-file-batch | 2.59 / 2.68 | 1.24 | 537 | 3/3 / 3/3 |
| deepagents | filesystem-original | multi-file-batch | 3.78 / 3.87 | 3.66 | 857 | 3/3 / 3/3 |
| deepagents | filesystem-original | long-line | 1.43 / 1.43 | 1.37 | 40,297 | 3/3 / 3/3 |
| deepagents | filesystem-original | insert-after-anchor | 1.31 / 1.31 | 1.39 | 358 | 3/3 / 3/3 |
| deepagents | filesystem-original | delete-block | 1.29 / 1.35 | 1.25 | 346 | 3/3 / 3/3 |
| deepagents | filesystem-original | bom-crlf | 1.29 / 1.29 | 1.23 | 293 | 3/3 / 0/3 |
| deepagents | filesystem-original | mixed-eol | 1.29 / 1.35 | 1.24 | 300 | 3/3 / 0/3 |
| deepagents | filesystem-original | reject-missing | 1.15 / 1.15 | 1.19 | 235 | 3/3 / 3/3 |
| deepagents | filesystem-original | reject-ambiguous | 1.14 / 1.18 | 1.20 | 348 | 3/3 / 3/3 |
| deepagents | filesystem-original | noop | 1.28 / 1.49 | 1.21 | 252 | 3/3 / 3/3 |
| deepagents | filesystem-original | write-create | 1.31 / 1.34 | 0.00 | 119 | 3/3 / 3/3 |
| deepagents | filesystem-original | write-overwrite | 1.28 / 1.33 | 1.20 | 208 | 3/3 / 3/3 |
| deepagents | filesystem-original | large-65536 | 1.50 / 1.55 | 1.41 | 65,813 | 3/3 / 3/3 |
| deepagents | filesystem-original | large-1048576 | 2.72 / 2.73 | 4.24 | 1,048,859 | 3/3 / 3/3 |
| deepagents | filesystem-original | large-5242880 | 8.81 / 9.80 | 18.67 | 5,243,163 | 3/3 / 3/3 |
| deepagents | xnip-native | tiny-config | 4.74 / 4.95 | 1.80 | 330 | 3/3 / 3/3 |
| deepagents | xnip-native | large-local-change | 6.13 / 8.95 | 1.60 | 2,649 | 3/3 / 3/3 |
| deepagents | xnip-native | multiline-function | 3.87 / 3.87 | 1.35 | 551 | 3/3 / 3/3 |
| deepagents | xnip-native | unicode | 3.80 / 3.83 | 1.35 | 352 | 3/3 / 3/3 |
| deepagents | xnip-native | crlf | 4.05 / 4.75 | 1.37 | 309 | 3/3 / 0/3 |
| deepagents | xnip-native | no-final-newline | 3.82 / 3.92 | 1.29 | 331 | 3/3 / 3/3 |
| deepagents | xnip-native | disambiguated-block | 3.75 / 3.82 | 1.29 | 408 | 3/3 / 3/3 |
| deepagents | xnip-native | global-replace | 3.75 / 3.90 | 1.29 | 538 | 3/3 / 3/3 |
| deepagents | xnip-native | same-file-batch | 7.95 / 7.97 | 1.36 | 537 | 3/3 / 3/3 |
| deepagents | xnip-native | multi-file-batch | 11.06 / 12.09 | 3.82 | 857 | 3/3 / 3/3 |
| deepagents | xnip-native | long-line | 5.48 / 5.53 | 1.51 | 40,297 | 3/3 / 3/3 |
| deepagents | xnip-native | insert-after-anchor | 3.89 / 6.91 | 1.44 | 358 | 3/3 / 3/3 |
| deepagents | xnip-native | delete-block | 4.29 / 4.47 | 1.50 | 346 | 3/3 / 3/3 |
| deepagents | xnip-native | bom-crlf | 3.90 / 4.06 | 1.50 | 293 | 3/3 / 0/3 |
| deepagents | xnip-native | mixed-eol | 4.46 / 6.59 | 1.34 | 300 | 3/3 / 0/3 |
| deepagents | xnip-native | reject-missing | 1.35 / 1.35 | 1.31 | 235 | 3/3 / 3/3 |
| deepagents | xnip-native | reject-ambiguous | 1.38 / 1.44 | 1.26 | 348 | 3/3 / 3/3 |
| deepagents | xnip-native | noop | 1.62 / 1.64 | 1.33 | 252 | 3/3 / 3/3 |
| deepagents | xnip-native | write-create | 3.22 / 3.38 | 0.00 | 119 | 3/3 / 3/3 |
| deepagents | xnip-native | write-overwrite | 3.24 / 3.50 | 1.25 | 208 | 3/3 / 3/3 |
| deepagents | xnip-native | large-65536 | 5.81 / 6.05 | 1.47 | 65,813 | 3/3 / 3/3 |
| deepagents | xnip-native | large-1048576 | 49.77 / 50.88 | 4.84 | 1,048,859 | 3/3 / 3/3 |
| deepagents | xnip-native | large-5242880 | 262.57 / 279.66 | 16.82 | 5,243,163 | 3/3 / 3/3 |
| langgraph | xnip-tools | tiny-config | 3.43 / 3.50 | 1.45 | 211 | 3/3 / 3/3 |
| langgraph | xnip-tools | large-local-change | 5.68 / 5.88 | 1.50 | 12,787 | 3/3 / 3/3 |
| langgraph | xnip-tools | multiline-function | 3.42 / 3.43 | 1.38 | 425 | 3/3 / 3/3 |
| langgraph | xnip-tools | unicode | 3.42 / 3.56 | 1.35 | 237 | 3/3 / 3/3 |
| langgraph | xnip-tools | crlf | 3.51 / 3.55 | 1.45 | 203 | 3/3 / 3/3 |
| langgraph | xnip-tools | no-final-newline | 3.54 / 3.56 | 1.42 | 205 | 3/3 / 3/3 |
| langgraph | xnip-tools | disambiguated-block | 3.45 / 3.75 | 1.39 | 281 | 3/3 / 3/3 |
| langgraph | xnip-tools | global-replace | 3.74 / 6.74 | 1.56 | 413 | 3/3 / 3/3 |
| langgraph | xnip-tools | same-file-batch | 6.69 / 6.74 | 1.43 | 312 | 3/3 / 3/3 |
| langgraph | xnip-tools | multi-file-batch | 10.10 / 10.49 | 4.11 | 503 | 3/3 / 3/3 |
| langgraph | xnip-tools | long-line | 4.40 / 4.58 | 1.42 | 12,215 | 3/3 / 3/3 |
| langgraph | xnip-tools | insert-after-anchor | 3.52 / 3.54 | 1.31 | 231 | 3/3 / 3/3 |
| langgraph | xnip-tools | delete-block | 3.77 / 3.79 | 1.37 | 226 | 3/3 / 3/3 |
| langgraph | xnip-tools | bom-crlf | 3.43 / 3.49 | 1.35 | 183 | 3/3 / 3/3 |
| langgraph | xnip-tools | mixed-eol | 3.52 / 3.54 | 1.38 | 193 | 3/3 / 3/3 |
| langgraph | xnip-tools | reject-missing | 1.24 / 1.26 | 1.35 | 299 | 3/3 / 3/3 |
| langgraph | xnip-tools | reject-ambiguous | 1.23 / 1.27 | 1.30 | 287 | 3/3 / 3/3 |
| langgraph | xnip-tools | noop | 1.46 / 1.65 | 1.50 | 146 | 3/3 / 3/3 |
| langgraph | xnip-tools | write-create | 3.10 / 3.50 | 0.00 | 80 | 3/3 / 3/3 |
| langgraph | xnip-tools | write-overwrite | 3.29 / 3.29 | 1.37 | 143 | 3/3 / 3/3 |
| langgraph | xnip-tools | large-65536 | 4.61 / 4.94 | 1.42 | 12,211 | 3/3 / 3/3 |
| langgraph | xnip-tools | large-1048576 | 33.74 / 33.95 | 2.36 | 12,217 | 3/3 / 3/3 |
| langgraph | xnip-tools | large-5242880 | 166.03 / 167.66 | 4.23 | 12,217 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | tiny-config | 1.51 / 1.65 | 0.61 | 228 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | large-local-change | 1.46 / 1.64 | 0.65 | 6,573 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | multiline-function | 1.36 / 1.37 | 0.61 | 474 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | unicode | 1.50 / 1.52 | 0.60 | 246 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | crlf | 1.46 / 1.52 | 0.61 | 220 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | no-final-newline | 1.37 / 1.41 | 0.59 | 216 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | disambiguated-block | 1.42 / 1.43 | 0.61 | 314 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | global-replace | 1.38 / 1.38 | 0.62 | 566 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | same-file-batch | 2.56 / 2.65 | 0.61 | 316 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | multi-file-batch | 2.97 / 3.20 | 1.35 | 506 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | long-line | 1.21 / 1.25 | 0.58 | 12,237 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | insert-after-anchor | 1.16 / 1.19 | 0.51 | 248 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | delete-block | 1.14 / 1.25 | 0.52 | 251 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | bom-crlf | 1.10 / 1.15 | 0.49 | 202 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | mixed-eol | 1.06 / 1.12 | 0.53 | 218 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | reject-missing | 0.37 / 0.37 | 0.49 | 222 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | reject-ambiguous | 0.36 / 0.46 | 0.50 | 210 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | noop | 0.43 / 0.44 | 0.46 | 147 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | write-create | 1.91 / 1.93 | 0.00 | 95 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | write-overwrite | 2.31 / 2.37 | 0.53 | 172 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | large-65536 | 1.21 / 1.27 | 0.56 | 12,233 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | large-1048576 | 2.28 / 2.36 | 1.04 | 12,237 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | large-5242880 | 7.62 / 8.28 | 3.49 | 12,237 | 3/3 / 3/3 |

## Reproduce

```sh
python benches/native/python_agents_setup.py
python benches/native/python_agents_setup.py --check
target/native-hosts/python-agents/.venv/bin/python benches/python_agents_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```

On Windows use `.venv/Scripts/python.exe` and `xnip.exe`. `--scenarios` selects comma-separated fixtures; `--output-prefix` selects local output files. Finish builds and SDK preparation before collecting samples.
