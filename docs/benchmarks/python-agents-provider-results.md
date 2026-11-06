# Python agent SDK tool benchmarks

DeepAgents 0.7.34, LangGraph 1.2.20 and PydanticAI 2.66.0. 23 fixtures; 3 measured runs and 1 warm-up per fixture/provider. Zero model calls.

DeepAgents compares its original FilesystemBackend and xnip regular-file persistence through the same official FilesystemMiddleware tools, permissions wrapper and compiled LangGraph ToolNode. Matching, schemas and rendered results remain official SDK behavior. Both lanes disable large-result eviction during this file-tool cohort; ordinary applications retain their middleware policy.

LangGraph provides a tool registry and graph executor rather than a builtin file editor. Its lane executes the registered core-backed StructuredTools through a real StateGraph and ToolNode. PydanticAI executes its official MCPToolset.call_tool path against a real xnip stdio server. Their lanes measure supported SDK tool pipelines without inventing an original editor baseline.

The original thirteen byte fixtures remain unchanged. DeepAgents' edit policy uses Python universal newlines and its official string-replacement helper, so its host-specific oracle normalizes edited text to LF; the original literal byte goal is retained independently. Other hosts use xnip literal byte semantics. Complete expected files, unexpected files and SDK receipts/errors have independent oracles. Raw JSON retains actual argument objects and mutation results alongside their hashes; missing/ambiguous errors must have their specific cause, and successful receipts distinguish writes, no-ops and replacement counts.

Warm timing includes the complete SDK tool call. Fixture setup, file hashing and JSON accounting occur outside timing. SDK startup is reported separately. Payload counts cover actual SDK tool arguments and ToolMessage.content or the PydanticAI tool result; thrown exception messages have a separate byte field. Identity/status envelopes and generated message IDs are separate. Raw samples also retain actual native JSONL byte counters where that client is owned by the adapter; MCP transport-envelope bytes remain unknown. These measurements describe tool execution and local payloads, with zero agent/model generation or billing estimates.

Binary SHA-256 `b01cc02a39864cb7f460cdd29fc84c8059855229acc6aec97c43d1070505d012` (release); maintained source `c04c4932d5497d0d08d9598d2839e8a28c0cc5966ce25904c352f87f6666fb71`. Raw JSON retains package versions, source fingerprints and all measured samples. With three samples nearest-rank p95 is the maximum.

| Framework | Provider | SDK/session setup ms |
|---|---|---:|
| deepagents | filesystem-original | 62.23 |
| deepagents | xnip-native | 4.15 |
| langgraph | xnip-tools | 7.66 |
| pydantic-ai | xnip-tools | 728.87 |

| Framework | Provider | Fixture | Mutation median / p95 ms | Read median ms | SDK tool bytes | Host goal+outcome / literal goal |
|---|---|---|---:|---:|---:|---:|
| deepagents | filesystem-original | tiny-config | 1.36 / 1.37 | 1.44 | 330 | 3/3 / 3/3 |
| deepagents | filesystem-original | large-local-change | 1.33 / 1.33 | 1.38 | 2,649 | 3/3 / 3/3 |
| deepagents | filesystem-original | multiline-function | 1.30 / 1.31 | 1.40 | 551 | 3/3 / 3/3 |
| deepagents | filesystem-original | unicode | 1.23 / 1.41 | 1.26 | 352 | 3/3 / 3/3 |
| deepagents | filesystem-original | crlf | 1.23 / 1.30 | 1.21 | 309 | 3/3 / 0/3 |
| deepagents | filesystem-original | no-final-newline | 1.25 / 1.27 | 1.21 | 331 | 3/3 / 3/3 |
| deepagents | filesystem-original | disambiguated-block | 1.27 / 1.29 | 1.24 | 408 | 3/3 / 3/3 |
| deepagents | filesystem-original | global-replace | 1.24 / 1.24 | 1.24 | 538 | 3/3 / 3/3 |
| deepagents | filesystem-original | same-file-batch | 3.44 / 4.49 | 1.77 | 537 | 3/3 / 3/3 |
| deepagents | filesystem-original | multi-file-batch | 3.87 / 3.90 | 3.51 | 857 | 3/3 / 3/3 |
| deepagents | filesystem-original | long-line | 1.42 / 1.42 | 1.37 | 40,297 | 3/3 / 3/3 |
| deepagents | filesystem-original | insert-after-anchor | 1.30 / 1.30 | 1.23 | 358 | 3/3 / 3/3 |
| deepagents | filesystem-original | delete-block | 1.23 / 1.31 | 1.29 | 346 | 3/3 / 3/3 |
| deepagents | filesystem-original | bom-crlf | 1.33 / 1.36 | 1.22 | 293 | 3/3 / 0/3 |
| deepagents | filesystem-original | mixed-eol | 1.24 / 1.25 | 1.22 | 300 | 3/3 / 0/3 |
| deepagents | filesystem-original | reject-missing | 1.11 / 1.14 | 1.25 | 235 | 3/3 / 3/3 |
| deepagents | filesystem-original | reject-ambiguous | 1.13 / 1.19 | 1.22 | 348 | 3/3 / 3/3 |
| deepagents | filesystem-original | noop | 1.25 / 1.26 | 1.24 | 252 | 3/3 / 3/3 |
| deepagents | filesystem-original | write-create | 1.36 / 1.41 | 0.00 | 119 | 3/3 / 3/3 |
| deepagents | filesystem-original | write-overwrite | 1.23 / 1.26 | 1.23 | 208 | 3/3 / 3/3 |
| deepagents | filesystem-original | large-65536 | 1.37 / 1.45 | 1.55 | 65,813 | 3/3 / 3/3 |
| deepagents | filesystem-original | large-1048576 | 2.86 / 3.03 | 4.03 | 1,048,859 | 3/3 / 3/3 |
| deepagents | filesystem-original | large-5242880 | 9.07 / 9.68 | 19.33 | 5,243,163 | 3/3 / 3/3 |
| deepagents | xnip-native | tiny-config | 4.04 / 4.38 | 1.26 | 330 | 3/3 / 3/3 |
| deepagents | xnip-native | large-local-change | 6.35 / 6.39 | 1.45 | 2,649 | 3/3 / 3/3 |
| deepagents | xnip-native | multiline-function | 4.10 / 4.10 | 1.30 | 551 | 3/3 / 3/3 |
| deepagents | xnip-native | unicode | 4.10 / 4.15 | 1.38 | 352 | 3/3 / 3/3 |
| deepagents | xnip-native | crlf | 4.04 / 4.16 | 1.28 | 309 | 3/3 / 0/3 |
| deepagents | xnip-native | no-final-newline | 3.90 / 4.22 | 1.23 | 331 | 3/3 / 3/3 |
| deepagents | xnip-native | disambiguated-block | 3.99 / 4.05 | 1.24 | 408 | 3/3 / 3/3 |
| deepagents | xnip-native | global-replace | 4.03 / 4.06 | 1.43 | 538 | 3/3 / 3/3 |
| deepagents | xnip-native | same-file-batch | 7.89 / 8.05 | 1.39 | 537 | 3/3 / 3/3 |
| deepagents | xnip-native | multi-file-batch | 11.65 / 11.88 | 4.04 | 857 | 3/3 / 3/3 |
| deepagents | xnip-native | long-line | 5.80 / 5.94 | 1.42 | 40,297 | 3/3 / 3/3 |
| deepagents | xnip-native | insert-after-anchor | 4.09 / 4.13 | 1.36 | 358 | 3/3 / 3/3 |
| deepagents | xnip-native | delete-block | 4.27 / 4.39 | 1.39 | 346 | 3/3 / 3/3 |
| deepagents | xnip-native | bom-crlf | 4.06 / 4.48 | 1.37 | 293 | 3/3 / 0/3 |
| deepagents | xnip-native | mixed-eol | 4.24 / 4.29 | 1.40 | 300 | 3/3 / 0/3 |
| deepagents | xnip-native | reject-missing | 1.45 / 1.47 | 1.33 | 235 | 3/3 / 3/3 |
| deepagents | xnip-native | reject-ambiguous | 2.97 / 3.11 | 1.49 | 348 | 3/3 / 3/3 |
| deepagents | xnip-native | noop | 1.67 / 1.71 | 1.31 | 252 | 3/3 / 3/3 |
| deepagents | xnip-native | write-create | 3.72 / 4.10 | 0.00 | 119 | 3/3 / 3/3 |
| deepagents | xnip-native | write-overwrite | 4.13 / 5.02 | 1.30 | 208 | 3/3 / 3/3 |
| deepagents | xnip-native | large-65536 | 6.68 / 7.29 | 1.52 | 65,813 | 3/3 / 3/3 |
| deepagents | xnip-native | large-1048576 | 49.23 / 51.58 | 4.92 | 1,048,859 | 3/3 / 3/3 |
| deepagents | xnip-native | large-5242880 | 249.07 / 253.50 | 16.38 | 5,243,163 | 3/3 / 3/3 |
| langgraph | xnip-tools | tiny-config | 4.08 / 4.42 | 1.34 | 211 | 3/3 / 3/3 |
| langgraph | xnip-tools | large-local-change | 5.71 / 5.89 | 1.36 | 12,787 | 3/3 / 3/3 |
| langgraph | xnip-tools | multiline-function | 4.13 / 4.33 | 1.31 | 425 | 3/3 / 3/3 |
| langgraph | xnip-tools | unicode | 3.97 / 4.19 | 1.32 | 237 | 3/3 / 3/3 |
| langgraph | xnip-tools | crlf | 4.08 / 4.11 | 1.29 | 203 | 3/3 / 3/3 |
| langgraph | xnip-tools | no-final-newline | 4.33 / 4.42 | 1.39 | 205 | 3/3 / 3/3 |
| langgraph | xnip-tools | disambiguated-block | 3.93 / 4.15 | 1.33 | 281 | 3/3 / 3/3 |
| langgraph | xnip-tools | global-replace | 3.86 / 3.91 | 1.27 | 413 | 3/3 / 3/3 |
| langgraph | xnip-tools | same-file-batch | 7.53 / 7.75 | 1.31 | 312 | 3/3 / 3/3 |
| langgraph | xnip-tools | multi-file-batch | 11.17 / 11.37 | 3.54 | 503 | 3/3 / 3/3 |
| langgraph | xnip-tools | long-line | 4.99 / 5.20 | 1.41 | 12,215 | 3/3 / 3/3 |
| langgraph | xnip-tools | insert-after-anchor | 3.71 / 4.07 | 1.43 | 231 | 3/3 / 3/3 |
| langgraph | xnip-tools | delete-block | 3.97 / 7.13 | 1.44 | 226 | 3/3 / 3/3 |
| langgraph | xnip-tools | bom-crlf | 4.14 / 5.09 | 1.46 | 183 | 3/3 / 3/3 |
| langgraph | xnip-tools | mixed-eol | 3.66 / 4.64 | 1.32 | 193 | 3/3 / 3/3 |
| langgraph | xnip-tools | reject-missing | 1.39 / 1.40 | 1.40 | 299 | 3/3 / 3/3 |
| langgraph | xnip-tools | reject-ambiguous | 1.25 / 1.30 | 1.33 | 287 | 3/3 / 3/3 |
| langgraph | xnip-tools | noop | 1.37 / 1.39 | 1.23 | 146 | 3/3 / 3/3 |
| langgraph | xnip-tools | write-create | 3.28 / 3.65 | 0.00 | 80 | 3/3 / 3/3 |
| langgraph | xnip-tools | write-overwrite | 3.63 / 3.68 | 1.30 | 143 | 3/3 / 3/3 |
| langgraph | xnip-tools | large-65536 | 5.19 / 5.20 | 1.40 | 12,211 | 3/3 / 3/3 |
| langgraph | xnip-tools | large-1048576 | 34.14 / 34.69 | 2.34 | 12,217 | 3/3 / 3/3 |
| langgraph | xnip-tools | large-5242880 | 155.93 / 161.30 | 4.68 | 12,217 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | tiny-config | 1.56 / 1.66 | 0.60 | 228 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | large-local-change | 1.38 / 1.39 | 0.66 | 6,573 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | multiline-function | 1.26 / 1.36 | 0.58 | 474 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | unicode | 1.27 / 1.45 | 0.57 | 246 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | crlf | 1.28 / 1.33 | 0.60 | 220 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | no-final-newline | 1.25 / 1.28 | 0.58 | 216 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | disambiguated-block | 1.28 / 1.30 | 0.55 | 314 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | global-replace | 1.12 / 1.18 | 0.48 | 566 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | same-file-batch | 1.99 / 1.99 | 0.51 | 316 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | multi-file-batch | 3.13 / 3.19 | 1.36 | 506 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | long-line | 1.10 / 1.13 | 0.55 | 12,237 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | insert-after-anchor | 1.10 / 1.19 | 0.54 | 248 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | delete-block | 1.09 / 1.09 | 0.50 | 251 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | bom-crlf | 1.13 / 1.35 | 0.49 | 202 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | mixed-eol | 1.18 / 1.21 | 0.49 | 218 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | reject-missing | 0.35 / 0.37 | 0.54 | 222 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | reject-ambiguous | 0.36 / 0.38 | 0.50 | 210 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | noop | 0.42 / 0.47 | 0.47 | 147 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | write-create | 2.40 / 2.48 | 0.00 | 95 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | write-overwrite | 2.84 / 3.10 | 0.52 | 172 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | large-65536 | 1.24 / 1.25 | 0.57 | 12,233 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | large-1048576 | 2.32 / 2.42 | 0.96 | 12,237 | 3/3 / 3/3 |
| pydantic-ai | xnip-tools | large-5242880 | 7.57 / 7.60 | 3.02 | 12,237 | 3/3 / 3/3 |

## Reproduce

```sh
python benches/native/python_agents_setup.py
python benches/native/python_agents_setup.py --check
target/native-hosts/python-agents/.venv/bin/python benches/python_agents_provider.py --executable target/release/xnip --repeats 3 --warmups 1
```

On Windows use `.venv/Scripts/python.exe` and `xnip.exe`. `--scenarios` selects comma-separated fixtures; `--output-prefix` selects local output files. Finish builds and SDK preparation before collecting samples.
