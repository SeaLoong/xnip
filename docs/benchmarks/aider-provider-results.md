# Aider scripting IO provider benchmark

Pinned commit `5dc9490bb35f9729ef2c95d00a19ccd30c26339c`; 24 fixtures across diff/whole/udiff, 142 groups and 426 measured workflows. Zero model completions.

The real Coder is created through its scripting IO parameter. Every measured reply enters its original apply_updates pipeline: native parser, dry-run matching, file authorization and final format executor. The original lane writes through InputOutput; the native lane delegates its reading/presentation/confirmation and persists text through xnip core. Both lanes retain identical format classes, raw prompt templates, supplied reply bytes and actual apply_updates receipts/diagnostics.

Aider describes this Python scripting API as version-sensitive. This cohort pins its source revision and full hashed Python dependencies; it measures this local scripting seam. SDK construction, source/model metadata setup, fixture reset, reply generation and file validation stay outside edit timing. Complete SEARCH/REPLACE or whole-file representations and three-line unified diffs are supplied from known fixture contents. Interactive user/model latency and final agent-facing message assembly have their own boundaries.

The host-selected LF policy and each format's explicit trailing-newline representation are declared separately from the original literal byte goal. Both independent file oracles and original/native result parity are retained. Aider's initial empty-file creation stays in its authorization pipeline. Git commit scheduling remains under Aider; benchmarks select auto_commits=False and use_git=False. Model.send_completion and the actual LiteLLM completion method are trapped and counted at zero.

Executable SHA-256 `b01cc02a39864cb7f460cdd29fc84c8059855229acc6aec97c43d1070505d012`; source `a8750a1be027f63663ae0c4ce7abfa3f1a57fb9f0ae376f077ffd13dc2a14f9f`. One warm-up and three measured workflows per group by default; nearest-rank p95 with three samples is the maximum.

| Fixture | Format | Original median / p95 ms | Native median / p95 ms | Input bytes | Native IPC bytes | Original / native host-goal exact |
|---|---|---:|---:|---:|---:|---:|
| tiny-config | diff | 0.32 / 0.38 | 2.22 / 2.29 | 135 | 1,171 | 3/3 / 3/3 |
| tiny-config | whole | 0.24 / 0.25 | 1.81 / 1.86 | 58 | 1,171 | 3/3 / 3/3 |
| tiny-config | udiff | 0.26 / 0.28 | 2.14 / 2.34 | 112 | 1,171 | 3/3 / 3/3 |
| large-local-change | diff | 2.20 / 2.27 | 6.13 / 6.18 | 93,837 | 294,410 | 3/3 / 3/3 |
| large-local-change | whole | 0.48 / 0.54 | 4.29 / 4.36 | 46,908 | 294,410 | 3/3 / 3/3 |
| large-local-change | udiff | 0.33 / 0.37 | 4.72 / 5.00 | 259 | 294,410 | 3/3 / 3/3 |
| multiline-function | diff | 0.31 / 0.37 | 2.40 / 2.41 | 261 | 1,690 | 3/3 / 3/3 |
| multiline-function | whole | 0.26 / 0.27 | 1.83 / 1.84 | 101 | 1,690 | 3/3 / 3/3 |
| multiline-function | udiff | 0.29 / 0.33 | 2.04 / 2.33 | 234 | 1,690 | 3/3 / 3/3 |
| unicode | diff | 0.31 / 0.32 | 2.30 / 2.37 | 164 | 1,286 | 3/3 / 3/3 |
| unicode | whole | 0.21 / 0.22 | 1.93 / 2.03 | 71 | 1,286 | 3/3 / 3/3 |
| unicode | udiff | 0.31 / 0.36 | 2.16 / 2.21 | 147 | 1,286 | 3/3 / 3/3 |
| crlf | diff | 0.33 / 0.35 | 2.41 / 2.67 | 135 | 1,227 | 3/3 / 3/3 |
| crlf | whole | 0.22 / 0.27 | 2.00 / 2.06 | 58 | 1,227 | 3/3 / 3/3 |
| crlf | udiff | 0.30 / 0.31 | 2.25 / 2.31 | 112 | 1,227 | 3/3 / 3/3 |
| no-final-newline | diff | 0.28 / 0.34 | 2.17 / 2.21 | 109 | 1,096 | 3/3 / 3/3 |
| no-final-newline | whole | 0.20 / 0.25 | 2.22 / 4.66 | 45 | 1,096 | 3/3 / 3/3 |
| no-final-newline | udiff | 0.25 / 0.26 | 2.35 / 2.38 | 98 | 1,096 | 3/3 / 3/3 |
| disambiguated-block | diff | 0.34 / 0.37 | 2.10 / 2.15 | 167 | 1,287 | 3/3 / 3/3 |
| disambiguated-block | whole | 0.21 / 0.22 | 2.26 / 2.36 | 74 | 1,287 | 3/3 / 3/3 |
| disambiguated-block | udiff | 0.26 / 0.28 | 2.48 / 2.66 | 119 | 1,287 | 3/3 / 3/3 |
| global-replace | diff | 0.29 / 0.33 | 2.15 / 2.45 | 499 | 2,831 | 3/3 / 3/3 |
| global-replace | whole | 0.20 / 0.20 | 2.05 / 2.11 | 240 | 2,831 | 3/3 / 3/3 |
| global-replace | udiff | 0.28 / 0.32 | 2.07 / 2.14 | 542 | 2,831 | 3/3 / 3/3 |
| same-file-batch | diff | 0.28 / 0.34 | 2.46 / 2.49 | 101 | 1,059 | 3/3 / 3/3 |
| same-file-batch | whole | 0.19 / 0.20 | 1.90 / 1.97 | 42 | 1,059 | 3/3 / 3/3 |
| same-file-batch | udiff | 0.27 / 0.27 | 2.08 / 2.21 | 101 | 1,059 | 3/3 / 3/3 |
| multi-file-batch | diff | 0.82 / 0.88 | 5.66 / 5.80 | 230 | 2,805 | 3/3 / 3/3 |
| multi-file-batch | whole | 0.37 / 0.37 | 5.97 / 6.57 | 78 | 2,805 | 3/3 / 3/3 |
| multi-file-batch | udiff | 0.62 / 0.72 | 6.11 / 6.22 | 209 | 2,805 | 3/3 / 3/3 |
| long-line | diff | 0.67 / 0.70 | 3.73 / 3.88 | 80,089 | 320,995 | 3/3 / 3/3 |
| long-line | whole | 0.24 / 0.25 | 3.10 / 3.27 | 40,035 | 320,995 | 3/3 / 3/3 |
| long-line | udiff | 0.46 / 0.48 | 3.56 / 4.19 | 80,088 | 320,995 | 3/3 / 3/3 |
| insert-after-anchor | diff | 0.30 / 0.32 | 2.26 / 2.27 | 122 | 1,124 | 3/3 / 3/3 |
| insert-after-anchor | whole | 0.19 / 0.22 | 2.61 / 2.72 | 57 | 1,124 | 3/3 / 3/3 |
| insert-after-anchor | udiff | 0.51 / 1.07 | 2.14 / 2.19 | 101 | 1,124 | 3/3 / 3/3 |
| delete-block | diff | 0.30 / 0.31 | 2.24 / 2.28 | 107 | 1,077 | 3/3 / 3/3 |
| delete-block | whole | 0.17 / 0.19 | 2.04 / 2.14 | 30 | 1,077 | 3/3 / 3/3 |
| delete-block | udiff | 0.26 / 0.34 | 2.13 / 2.17 | 102 | 1,077 | 3/3 / 3/3 |
| aider-create | diff | 0.43 / 0.45 | 1.95 / 1.96 | 67 | 880 | 3/3 / 3/3 |
| aider-create | whole | 0.38 / 0.38 | 2.23 / 2.26 | 28 | 880 | 3/3 / 3/3 |
| aider-create | udiff | 0.40 / 0.45 | 2.26 / 2.29 | 67 | 880 | 3/3 / 3/3 |
| aider-whole-overwrite | diff | 0.29 / 0.31 | 2.20 / 2.42 | 67 | 883 | 3/3 / 3/3 |
| aider-whole-overwrite | whole | 0.19 / 0.20 | 1.97 / 2.00 | 24 | 883 | 3/3 / 3/3 |
| aider-whole-overwrite | udiff | 0.24 / 0.26 | 2.10 / 2.13 | 66 | 883 | 3/3 / 3/3 |
| aider-noop | diff | 0.28 / 0.40 | 0.52 / 0.55 | 69 | 649 | 3/3 / 3/3 |
| aider-noop | whole | 0.19 / 0.20 | 0.35 / 0.38 | 25 | 649 | 3/3 / 3/3 |
| aider-noop | udiff | 0.01 / 0.01 | 0.02 / 0.02 | 12 | 0 | 3/3 / 3/3 |
| aider-reject-missing | diff | 0.62 / 0.70 | 0.75 / 0.78 | 71 | 0 | 3/3 / 3/3 |
| aider-reject-missing | udiff | 0.54 / 0.55 | 0.56 / 0.65 | 61 | 0 | 3/3 / 3/3 |
| aider-approval-denied | diff | 0.28 / 0.30 | 0.31 / 0.35 | 65 | 0 | 3/3 / 3/3 |
| aider-approval-denied | whole | 0.20 / 0.21 | 0.21 / 0.26 | 22 | 0 | 3/3 / 3/3 |
| aider-approval-denied | udiff | 0.20 / 0.22 | 0.20 / 0.24 | 62 | 0 | 3/3 / 3/3 |
| aider-dry-run | diff | 0.22 / 0.24 | 0.26 / 0.27 | 67 | 0 | 3/3 / 3/3 |
| aider-dry-run | whole | 0.11 / 0.14 | 0.12 / 0.12 | 24 | 0 | 3/3 / 3/3 |
| aider-dry-run | udiff | 0.16 / 0.18 | 0.20 / 0.23 | 66 | 0 | 3/3 / 3/3 |
| aider-latin1 | diff | 0.28 / 0.33 | 2.21 / 2.23 | 79 | 1,052 | 3/3 / 3/3 |
| aider-latin1 | whole | 0.19 / 0.20 | 1.91 / 1.92 | 30 | 1,052 | 3/3 / 3/3 |
| aider-latin1 | udiff | 0.25 / 0.39 | 2.07 / 2.33 | 77 | 1,052 | 3/3 / 3/3 |
| aider-crlf-output | diff | 0.29 / 0.29 | 1.87 / 1.90 | 85 | 1,003 | 3/3 / 3/3 |
| aider-crlf-output | whole | 0.24 / 0.78 | 1.92 / 2.02 | 33 | 1,003 | 3/3 / 3/3 |
| aider-crlf-output | udiff | 0.28 / 0.28 | 1.85 / 1.87 | 81 | 1,003 | 3/3 / 3/3 |
| aider-large-65536 | diff | 0.86 / 0.96 | 4.13 / 4.17 | 131,131 | 459,624 | 3/3 / 3/3 |
| aider-large-65536 | whole | 0.25 / 0.26 | 3.42 / 3.43 | 65,556 | 459,624 | 3/3 / 3/3 |
| aider-large-65536 | udiff | 0.53 / 0.65 | 4.04 / 4.10 | 65,608 | 459,624 | 3/3 / 3/3 |
| aider-large-1048576 | diff | 8.79 / 10.18 | 42.00 / 42.06 | 2,097,211 | 7,340,904 | 3/3 / 3/3 |
| aider-large-1048576 | whole | 1.04 / 1.07 | 33.12 / 34.04 | 1,048,596 | 7,340,904 | 3/3 / 3/3 |
| aider-large-1048576 | udiff | 4.67 / 6.68 | 37.36 / 38.28 | 1,048,648 | 7,340,904 | 3/3 / 3/3 |
| aider-large-5242880 | diff | 65.50 / 65.52 | 232.81 / 236.79 | 10,485,819 | 36,701,032 | 3/3 / 3/3 |
| aider-large-5242880 | whole | 7.49 / 9.25 | 183.09 / 199.54 | 5,242,900 | 36,701,032 | 3/3 / 3/3 |
| aider-large-5242880 | udiff | 36.99 / 37.56 | 220.90 / 240.91 | 5,242,952 | 36,701,032 | 3/3 / 3/3 |

## Reproduce

```sh
python3.13 benches/native/aider_native_setup.py
target/native-hosts/aider-5dc9490/aider-venv/bin/python benches/aider_provider.py --executable target/release/xnip
```

See [SDK setup and restoration](../aider-native.md) and the JSON companion for per-sample receipts, exact input/output/file hashes, SDK sources, dependencies, initialization costs and bridge counters.
