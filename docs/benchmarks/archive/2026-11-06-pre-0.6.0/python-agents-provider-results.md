# Python agent SDK tool benchmarks

DeepAgents 0.7.34, LangGraph 1.2.20 and PydanticAI 2.66.0. 23 fixtures; 3 measured runs and 1 warm-up per fixture/provider. Zero model calls.

DeepAgents compares its original FilesystemBackend and xnip regular-file persistence through the same official FilesystemMiddleware tools, permissions wrapper and compiled LangGraph ToolNode. Matching, schemas and rendered results remain official SDK behavior. Both lanes disable large-result eviction during this file-tool cohort; ordinary applications retain their middleware policy.

LangGraph provides a tool registry and graph executor rather than a builtin file editor. Its lane executes the registered core-backed StructuredTools through a real StateGraph and ToolNode. PydanticAI executes its official MCPToolset.call_tool path against a real xnip stdio server. Their lanes measure supported SDK tool pipelines without inventing an original editor baseline.

The original thirteen byte fixtures remain unchanged. DeepAgents' edit policy uses Python universal newlines and its official string-replacement helper, so its host-specific oracle normalizes edited text to LF; the original literal byte goal is retained independently. Other hosts use xnip literal byte semantics. Complete expected files, unexpected files and SDK receipts/errors have independent oracles. Raw JSON retains actual argument objects and mutation results alongside their hashes; missing/ambiguous errors must have their specific cause, and successful receipts distinguish writes, no-ops and replacement counts.

Warm timing includes the complete SDK tool call. Fixture setup, file hashing and JSON accounting occur outside timing. SDK startup is reported separately. Payload counts cover actual SDK tool arguments and ToolMessage.content or the PydanticAI tool result; thrown exception messages have a separate byte field. Identity/status envelopes and generated message IDs are separate. Raw samples also retain actual native JSONL byte counters where that client is owned by the adapter; MCP transport-envelope bytes remain unknown. These measurements describe tool execution and local payloads, with zero agent/model generation or billing estimates.

Binary SHA-256 `16cecdbad176790492338643e29480d2dc7793c5cda5629102e0dde434be8deb` (release); maintained source `4dc0cce6dfbdd1b30cc5d58347043662c59ba7fc300c6c95567a90b63ea5718f`. Raw JSON retains package versions, source fingerprints and all measured samples. With three samples nearest-rank p95 is the maximum.

| Framework | Provider | SDK/session setup ms |
|---|---|---:|
| deepagents | filesystem-original | 60.43 |
| deepagents | xnip-native | 3.77 |
| langgraph | xnip-tools | 7.54 |
| pydantic-ai | xnip-tools | 731.11 |

| Framework | Provider | Fixture | Mutation median / p95 ms | Read median ms | SDK tool bytes | Host goal+outcome / literal goal |
|---|---|---|---:|---:|---:|---:|
| deepagents | filesystem-original | tiny-config | 1.40 / 1.42 | 1.34 | 330 | 3/3 / 3/3 |
| deepagents | filesystem-original | large-local-change | 1.42 / 1.45 | 1.52 | 2,649 | 3/3 / 3/3 |
| deepagents | filesystem-original | multiline-function | 1.46 / 1.97 | 1.53 | 551 | 3/3 / 3/3 |
| deepagents | filesystem-original | unicode | 1.31 / 1.35 | 1.40 | 352 | 3/3 / 3/3 |
| deepagents | filesystem-original | crlf | 1.37 / 1.49 | 1.27 | 309 | 3/3 / 0/3 |
| deepagents | filesystem-original | no-final-newline | 1.26 / 1.31 | 1.34 | 331 | 3/3 / 3/3 |
| deepagents | filesystem-original | disambiguated-block | 1.27 / 1.31 | 1.26 | 408 | 3/3 / 3/3 |
| deepagents | filesystem-original | global-replace | 1.31 / 1.45 | 1.29 | 538 | 3/3 / 3/3 |
| deepagents | filesystem-original | same-file-batch | 2.66 / 2.83 | 1.30 | 537 | 3/3 / 3/3 |
| deepagents | filesystem-original | multi-file-batch | 3.98 / 4.14 | 3.90 | 857 | 3/3 / 3/3 |
| deepagents | filesystem-original | long-line | 1.35 / 1.38 | 1.51 | 40,297 | 3/3 / 3/3 |
| deepagents | filesystem-original | insert-after-anchor | 1.33 / 1.36 | 1.27 | 358 | 3/3 / 3/3 |
| deepagents | filesystem-original | delete-block | 1.26 / 1.31 | 1.32 | 346 | 3/3 / 3/3 |
| deepagents | filesystem-original | bom-crlf | 1.28 / 1.29 | 1.27 | 293 | 3/3 / 0/3 |
| deepagents | filesystem-original | mixed-eol | 1.31 / 1.32 | 1.24 | 300 | 3/3 / 0/3 |
| deepagents | filesystem-original | reject-missing | 1.23 / 1.32 | 1.24 | 235 | 3/3 / 3/3 |
| deepagents | filesystem-original | reject-ambiguous | 1.20 / 1.25 | 1.27 | 348 | 3/3 / 3/3 |
| deepagents | filesystem-original | noop | 1.27 / 1.38 | 1.26 | 252 | 3/3 / 3/3 |
| deepagents | filesystem-original | write-create | 1.27 / 1.31 | 0.00 | 119 | 3/3 / 3/3 |
| deepagents | filesystem-original | write-overwrite | 1.24 / 1.25 | 1.30 | 208 | 3/3 / 3/3 |
| deepagents | filesystem-original | large-65536 | 1.46 / 1.67 | 1.44 | 65,813 | 3/3 / 3/3 |
| deepagents | filesystem-original | large-1048576 | 3.32 / 3.36 | 4.21 | 1,048,859 | 3/3 / 3/3 |
| deepagents | filesystem-original | large-5242880 | 9.72 / 9.89 | 18.77 | 5,243,163 | 3/3 / 3/3 |
| deepagents | xnip-native | tiny-config | 4.24 / 4.26 | 1.40 | 330 | 3/3 / 3/3 |
| deepagents | xnip-native | large-local-change | 6.60 / 6.68 | 1.64 | 2,649 | 3/3 / 3/3 |
| deepagents | xnip-native | multiline-function | 3.97 / 4.74 | 1.32 | 551 | 3/3 / 3/3 |
| deepagents | xnip-native | unicode | 4.20 / 4.24 | 1.44 | 352 | 3/3 / 3/3 |
| deepagents | xnip-native | crlf | 4.04 / 4.06 | 1.41 | 309 | 3/3 / 0/3 |
| deepagents | xnip-native | no-final-newline | 3.82 / 4.23 | 1.32 | 331 | 3/3 / 3/3 |
| deepagents | xnip-native | disambiguated-block | 3.81 / 3.82 | 1.37 | 408 | 3/3 / 3/3 |
| deepagents | xnip-native | global-replace | 3.66 / 3.73 | 1.27 | 538 | 3/3 / 3/3 |
| deepagents | xnip-native | same-file-batch | 6.90 / 7.25 | 1.29 | 537 | 3/3 / 3/3 |
| deepagents | xnip-native | multi-file-batch | 10.31 / 10.48 | 3.79 | 857 | 3/3 / 3/3 |
| deepagents | xnip-native | long-line | 5.09 / 5.32 | 1.42 | 40,297 | 3/3 / 3/3 |
| deepagents | xnip-native | insert-after-anchor | 3.58 / 3.71 | 1.23 | 358 | 3/3 / 3/3 |
| deepagents | xnip-native | delete-block | 3.52 / 3.70 | 1.30 | 346 | 3/3 / 3/3 |
| deepagents | xnip-native | bom-crlf | 3.63 / 7.28 | 1.32 | 293 | 3/3 / 0/3 |
| deepagents | xnip-native | mixed-eol | 3.70 / 3.95 | 1.32 | 300 | 3/3 / 0/3 |
| deepagents | xnip-native | reject-missing | 1.32 / 1.62 | 1.28 | 235 | 3/3 / 3/3 |
| deepagents | xnip-native | reject-ambiguous | 1.39 / 1.45 | 1.29 | 348 | 3/3 / 3/3 |
| deepagents | xnip-native | noop | 1.74 / 1.74 | 1.31 | 252 | 3/3 / 3/3 |
| deepagents | xnip-native | write-create | 3.34 / 3.43 | 0.00 | 119 | 3/3 / 3/3 |
| deepagents | xnip-native | write-overwrite | 3.49 / 3.49 | 1.31 | 208 | 3/3 / 3/3 |
| deepagents | xnip-native | large-65536 | 6.74 / 6.75 | 1.60 | 65,813 | 3/3 / 3/3 |
| deepagents | xnip-native | large-1048576 | 56.91 / 58.00 | 4.73 | 1,048,859 | 3/3 / 3/3 |
| deepagents | xnip-native | large-5242880 | 261.66 / 263.31 | 17.18 | 5,243,163 | 3/3 / 3/3 |
| langgraph | xnip-tools | tiny-config | 3.46 / 3.50 | 1.33 | 211 | 3/3 / 3/3 |
| langgraph | xnip-tools | large-local-change | 4.97 / 5.12 | 1.36 | 12,787 | 3/3 / 3/3 |
| langgraph | xnip-tools | multiline-function | 3.42 / 3.65 | 1.34 | 425 | 3/3 / 3/3 |
| langgraph | xnip-tools | unicode | 3.44 / 3.62 | 1.38 | 237 | 3/3 / 3/3 |
| langgraph | xnip-tools | crlf | 3.32 / 3.38 | 1.27 | 203 | 3/3 / 3/3 |
| langgraph | xnip-tools | no-final-newline | 3.47 / 3.67 | 1.34 | 205 | 3/3 / 3/3 |
| langgraph | xnip-tools | disambiguated-block | 3.44 / 3.45 | 1.28 | 281 | 3/3 / 3/3 |
| langgraph | xnip-tools | global-replace | 3.59 / 3.59 | 1.31 | 413 | 3/3 / 3/3 |
| langgraph | xnip-tools | same-file-batch | 7.27 / 7.57 | 1.27 | 312 | 3/3 / 3/3 |
| langgraph | xnip-tools | multi-file-batch | 9.65 / 10.33 | 3.69 | 503 | 3/3 / 3/3 |
| langgraph | xnip-tools | long-line | 4.63 / 5.01 | 1.37 | 12,215 | 3/3 / 3/3 |
| langgraph | xnip-tools | insert-after-anchor | 3.47 / 3.58 | 1.27 | 231 | 3/3 / 3/3 |
| langgraph | xnip-tools | delete-block | 3.24 / 3.48 | 1.32 | 226 | 3/3 / 3/3 |
| langgraph | xnip-tools | bom-crlf | 3.16 / 3.23 | 1.36 | 183 | 3/3 / 3/3 |
| langgraph | xnip-tools | mixed-eol | 3.21 / 3.38 | 1.28 | 193 | 3/3 / 3/3 |
| langgraph | xnip-tools | reject-missing | 1.15 / 1.17 | 1.29 | 299 | 3/3 / 3/3 |
| langgraph | xnip-tools | reject-ambiguous | 1.16 / 1.20 | 1.23 | 287 | 3/3 / 3/3 |
| langgraph | xnip-tools | noop | 1.24 / 1.30 | 1.20 | 146 | 3/3 / 3/3 |
| langgraph | xnip-tools | write-create | 2.92 / 2.93 | 0.00 | 80 | 3/3 / 3/3 |
| langgraph | xnip-tools | write-overwrite | 3.18 / 3.23 | 1.27 | 143 | 3/3 / 3/3 |
| langgraph | xnip-tools | large-65536 | 4.78 / 4.85 | 1.35 | 12,211 | 3/3 / 3/3 |
| langgraph | xnip-tools | large-1048576 | 33.45 / 33.47 | 2.16 | 12,217 | 3/3 / 3/3 |
| langgraph | xnip-tools | large-5242880 | 166.70 / 168.40 | 5.41 | 12,217 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | tiny-config | 1.45 / 1.46 | 0.53 | 228 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | large-local-change | 1.21 / 1.21 | 0.52 | 6,573 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | multiline-function | 1.33 / 1.47 | 0.70 | 474 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | unicode | 0.98 / 1.00 | 0.47 | 246 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | crlf | 1.01 / 1.04 | 0.44 | 220 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | no-final-newline | 1.18 / 1.25 | 0.44 | 216 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | disambiguated-block | 1.11 / 1.44 | 0.45 | 314 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | global-replace | 1.10 / 1.13 | 0.54 | 566 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | same-file-batch | 2.01 / 2.26 | 0.48 | 316 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | multi-file-batch | 2.76 / 2.80 | 1.24 | 506 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | long-line | 1.08 / 1.14 | 0.55 | 12,237 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | insert-after-anchor | 0.99 / 1.01 | 0.44 | 248 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | delete-block | 0.98 / 0.98 | 0.44 | 251 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | bom-crlf | 0.97 / 0.99 | 0.46 | 202 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | mixed-eol | 0.91 / 0.95 | 0.46 | 218 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | reject-missing | 0.33 / 0.35 | 0.47 | 222 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | reject-ambiguous | 0.34 / 0.36 | 0.48 | 210 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | noop | 0.37 / 0.38 | 0.42 | 147 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | write-create | 1.98 / 2.04 | 0.00 | 95 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | write-overwrite | 2.15 / 2.24 | 0.50 | 172 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | large-65536 | 2.20 / 2.41 | 0.53 | 12,233 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | large-1048576 | 2.21 / 2.41 | 1.00 | 12,237 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | large-5242880 | 7.87 / 7.98 | 3.27 | 12,237 | 3/3 / 3/3 |

## Reproduce

```sh
python benches/native/python_agents_setup.py
python benches/native/python_agents_setup.py --check
target/native-hosts/python-agents/.venv/bin/python benches/python_agents_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```

On Windows use `.venv/Scripts/python.exe` and `xnip.exe`. `--scenarios` selects comma-separated fixtures; `--output-prefix` selects local output files. Finish builds and SDK preparation before collecting samples.
