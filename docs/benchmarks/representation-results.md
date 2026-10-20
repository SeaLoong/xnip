# Supplementary representation cost benchmark

Counter: {"encoding":"o200k_base","tokenizer":"tiktoken","unit":"tokens","version":"0.14.0"}. Revision: 0b7155cab1c647923ac37adb407d775af404bf4e; modified source: True.
Working-tree binary SHA-256: 713ff9a0bce7e7ff4b183f4bfaad3e8eb5297cb311da4e8ad9ca96204bbf2207 (unchanged across run).
Source tree SHA-256: cd250f818477f8a149f674a0c6dc13d61e43a5e869cb625d2bb5b26070bd80d1; includes tracked and untracked build/instruction inputs. Individual file hashes are in JSON.
Comparison scope: current xnip options and harness editing representations; no historical versions.
Background auto-update is disabled for every xnip subprocess with XNIP_AUTO_UPDATE=0.
All xnip execution rows check final bytes. Other harness formats are serialization examples; no external agent/model was run.
Token counts, when enabled, are exact for the named tokenizer and serialized text. They exclude provider billing, reasoning, cached input, and hidden harness prompts.

| MCP catalog | Tools | Cost |
|---|---:|---:|
| compact / structured | 5 | 2303 |
| full / structured | 10 | 4150 |
| compact / text | 5 | 1754 |

| Current instruction entry point | Cost |
|---|---:|
| docs/SKILL.md | 753 |
| integrations/generic/SKILL.md | 762 |
| integrations/claude-code/SKILL.md | 760 |
| integrations/codex/AGENTS.md | 74 |
| integrations/codex/guide.md | 730 |
| integrations/agents-md/AGENTS.md | 272 |
| integrations/aider/CONVENTIONS.md | 273 |
| integrations/copilot/xnip.md | 275 |
| integrations/cursor/xnip.mdc | 303 |

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

| Compact interface | MCP files guarded | MCP files snapshot | MCP write snapshot | CLI compact guarded | CLI compact snapshot |
|---|---:|---:|---:|---:|---:|
| tiny-config | 19 | 20 | 26 | 17 | 18 |
| large-local-change | 30 | 21007 | 21013 | 28 | 21005 |
| multiline-function | 43 | 32 | 38 | 41 | 30 |
| unicode | 21 | 25 | 31 | 19 | 23 |
| crlf | 19 | 23 | 29 | 17 | 21 |
| no-final-newline | 19 | 16 | 22 | 17 | 14 |
| disambiguated-block | 29 | 29 | 35 | 27 | 27 |
| global-replace | 174 | 88 | 94 | 171 | 86 |
| same-file-batch | 26 | 21 | 27 | 24 | 19 |
| multi-file-batch | 33 | 30 | 54 | 31 | 28 |
| long-line | 16 | 5014 | 5020 | 14 | 5012 |
| insert-after-anchor | 20 | 20 | 26 | 18 | 18 |
| delete-block | 18 | 12 | 18 | 16 | 10 |

Guarded maps retain old-content checks. Snapshot variants explicitly allow whole-file writes; these rows have different precondition contracts.


| Current output mode / budget | Cost |
|---|---:|
| peek-default | 2829 |
| peek-unlimited | 8012 |
| find-default | 735 |
| find-unlimited | 2812 |
| preview-default | 1587 |
| preview-unlimited | 5084 |
| cli-preview-compact | 1561 |
| cli-preview-human | 5069 |
| cli-preview-json | 5088 |

| Injected failure + read + retry | Total cost |
|---|---:|
| ambiguous-old | 177 |
| stale-old | 174 |
| overlapping-old | 164 |

| xnip batch accounting | Known context | Full context | Payload | Result |
|---|---:|---:|---:|---:|
| tiny-config | 4 | 12 | 26 | 46 |
| large-local-change | 10 | 19000 | 37 | 46 |
| multiline-function | 21 | 30 | 50 | 46 |
| unicode | 5 | 15 | 28 | 46 |
| crlf | 4 | 12 | 26 | 46 |
| no-final-newline | 4 | 8 | 26 | 46 |
| disambiguated-block | 7 | 16 | 36 | 46 |
| global-replace | 2 | 60 | 26 | 46 |
| same-file-batch | 6 | 12 | 42 | 46 |
| multi-file-batch | 3 | 12 | 48 | 66 |
| long-line | 3 | 5007 | 23 | 46 |
| insert-after-anchor | 3 | 7 | 27 | 46 |
| delete-block | 7 | 11 | 26 | 46 |

139 xnip execution cases passed per repetition; repeats=3.
3 injected rejection/read/retry cases and lifecycle bytes passed.

Timing includes local scheduling and storage; CLI startup and persistent MCP have different scopes.
The fixtures include cases where xnip loses on payload size. Format size does not predict model reliability.
JSON includes fixture bytes, generated payloads, actual responses, schema snapshots and retry transcripts.
Sessions use repository target/native-bench/sessions. Public display metadata replaces machine HOME prefixes with `<HOME>` after measurement; the disclosure_redaction ledger retains original-string hashes. Fixture bytes, serialized payloads and tool inputs are unchanged, and all costs use pre-redaction strings.
