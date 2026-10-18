# Supplementary representation cost benchmark

Counter: {"encoding":"o200k_base","tokenizer":"tiktoken","unit":"tokens","version":"0.14.0"}. Revision: 5991ae6d0d929437f05f1d31da6a01e6b2cd75c3; modified source: True.
Working-tree binary SHA-256: d8f7d29715a0d8479aa0bd90f40ae318e511d6c2a066aa95ffd5ead403c7a08c (unchanged across run).
Source tree SHA-256: 75720daac698851ff07c2d19f356e4289f2ccdae648864eda04443177adba7eb; includes tracked and untracked build/instruction inputs. Individual file hashes are in JSON.
Comparison scope: current xnip options and harness editing representations; no historical versions.
Background auto-update is disabled for every xnip subprocess with XNIP_AUTO_UPDATE=0.
All xnip execution rows check final bytes. Other harness formats are serialization examples; no external agent/model was run.
Token counts, when enabled, are exact for the named tokenizer and serialized text. They exclude provider billing, reasoning, cached input, and hidden harness prompts.

| MCP catalog | Tools | Cost |
|---|---:|---:|
| compact | 5 | 2097 |
| full | 10 | 4160 |

| Current instruction entry point | Cost |
|---|---:|
| docs/SKILL.md | 613 |
| integrations/generic/SKILL.md | 621 |
| integrations/claude-code/SKILL.md | 619 |
| integrations/codex/AGENTS.md | 74 |
| integrations/codex/guide.md | 588 |
| integrations/agents-md/AGENTS.md | 214 |
| integrations/aider/CONVENTIONS.md | 215 |
| integrations/copilot/xnip.md | 217 |
| integrations/cursor/xnip.mdc | 240 |

Instruction entry points have different installation roles; references loaded on demand are excluded.

Payload costs exclude context and responses; JSON records all three separately.

| Scenario | Whole file | Exact JSON | Aider blocks | Unified | apply_patch | Lines | xnip single | xnip batch | Native |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| tiny-config | 24 | 26 | 40 | 41 | 36 | 33 | 21 | 26 | 18 |
| large-local-change | 21011 | 37 | 149 | 101 | 96 | 45 | 32 | 37 | 27 |
| multiline-function | 36 | 50 | 64 | 69 | 64 | 59 | 45 | 50 | 41 |
| unicode | 29 | 28 | 46 | 49 | 44 | 43 | 23 | 28 | 20 |
| crlf | 27 | 26 | unsupported | unsupported | unsupported | 35 | 21 | 26 | 18 |
| no-final-newline | 20 | 26 | unsupported | unsupported | unsupported | 31 | 21 | 26 | 18 |
| disambiguated-block | 33 | 36 | 42 | 43 | 38 | 33 | 31 | 36 | 28 |
| global-replace | 92 | 26 | 136 | 163 | 158 | 185 | 21 | 26 | 16 |
| same-file-batch | 25 | 45 | 40 | 45 | 40 | 59 | 38 | 42 | 32 |
| multi-file-batch | 42 | 54 | 72 | 85 | 66 | 87 | 45 | 48 | 36 |
| long-line | 5018 | 23 | 10029 | 10033 | 10032 | 10036 | 18 | 23 | 15 |
| insert-after-anchor | 24 | 27 | 33 | 34 | 29 | 27 | 22 | 27 | 19 |
| delete-block | 16 | 24 | 31 | 35 | 30 | 31 | 21 | 26 | 15 |

| Current output mode / budget | Cost |
|---|---:|
| peek-default | 2834 |
| peek-unlimited | 8017 |
| find-default | 740 |
| find-unlimited | 2817 |
| preview-default | 1592 |
| preview-unlimited | 5089 |
| cli-preview-compact | 1561 |
| cli-preview-human | 5069 |
| cli-preview-json | 5088 |

| Injected failure + read + retry | Total cost |
|---|---:|
| ambiguous-old | 190 |
| stale-old | 183 |
| overlapping-old | 177 |

| xnip batch accounting | Known context | Full context | Payload | Result |
|---|---:|---:|---:|---:|
| tiny-config | 4 | 12 | 26 | 55 |
| large-local-change | 10 | 19000 | 37 | 55 |
| multiline-function | 21 | 30 | 50 | 55 |
| unicode | 5 | 15 | 28 | 55 |
| crlf | 4 | 12 | 26 | 55 |
| no-final-newline | 4 | 8 | 26 | 55 |
| disambiguated-block | 7 | 16 | 36 | 55 |
| global-replace | 2 | 60 | 26 | 55 |
| same-file-batch | 6 | 12 | 42 | 55 |
| multi-file-batch | 3 | 12 | 48 | 75 |
| long-line | 3 | 5007 | 23 | 55 |
| insert-after-anchor | 3 | 7 | 27 | 55 |
| delete-block | 7 | 11 | 26 | 55 |

74 xnip execution cases passed per repetition; repeats=3.
3 injected rejection/read/retry cases and lifecycle bytes passed.

Timing includes local scheduling and storage; CLI startup and persistent MCP have different scopes.
The fixtures include cases where xnip loses on payload size. Format size does not predict model reliability.
JSON includes fixture bytes, generated payloads, actual responses, schema snapshots and retry transcripts.
