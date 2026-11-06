# Aider scripting IO provider benchmark

Pinned commit `5dc9490bb35f9729ef2c95d00a19ccd30c26339c`; 24 fixtures across diff/whole/udiff, 142 groups and 426 measured workflows. Zero model completions.

The real Coder is created through its scripting IO parameter. Every measured reply enters its original apply_updates pipeline: native parser, dry-run matching, file authorization and final format executor. The original lane writes through InputOutput; the native lane delegates its reading/presentation/confirmation and persists text through xnip core. Both lanes retain identical format classes, raw prompt templates, supplied reply bytes and actual apply_updates receipts/diagnostics.

Aider describes this Python scripting API as version-sensitive. This cohort pins its source revision and full hashed Python dependencies; it measures this local scripting seam. SDK construction, source/model metadata setup, fixture reset, reply generation and file validation stay outside edit timing. Complete SEARCH/REPLACE or whole-file representations and three-line unified diffs are supplied from known fixture contents. Interactive user/model latency and final agent-facing message assembly have their own boundaries.

The host-selected LF policy and each format's explicit trailing-newline representation are declared separately from the original literal byte goal. Both independent file oracles and original/native result parity are retained. Aider's initial empty-file creation stays in its authorization pipeline. Git commit scheduling remains under Aider; benchmarks select auto_commits=False and use_git=False. Model.send_completion and the actual LiteLLM completion method are trapped and counted at zero.

Executable SHA-256 `16cecdbad176790492338643e29480d2dc7793c5cda5629102e0dde434be8deb`; source `7950cd7dfd4cdf11066c70cb7d70d2406f36af5d6fd7ce5b6fd184bf559016bd`. One warm-up and three measured workflows per group by default; nearest-rank p95 with three samples is the maximum.

| Fixture | Format | Original median / p95 ms | Native median / p95 ms | Input bytes | Native IPC bytes | Original / native host-goal exact |
|---|---|---:|---:|---:|---:|---:|
| tiny-config | diff | 0.38 / 0.70 | 2.35 / 2.37 | 135 | 1,171 | 3/3 / 3/3 |
| tiny-config | whole | 0.28 / 0.31 | 2.13 / 2.13 | 58 | 1,171 | 3/3 / 3/3 |
| tiny-config | udiff | 0.32 / 0.36 | 2.19 / 2.20 | 112 | 1,171 | 3/3 / 3/3 |
| large-local-change | diff | 2.27 / 2.33 | 6.64 / 6.83 | 93,837 | 294,410 | 3/3 / 3/3 |
| large-local-change | whole | 0.56 / 0.61 | 4.33 / 4.40 | 46,908 | 294,410 | 3/3 / 3/3 |
| large-local-change | udiff | 0.38 / 0.39 | 4.07 / 4.29 | 259 | 294,410 | 3/3 / 3/3 |
| multiline-function | diff | 0.36 / 0.36 | 2.47 / 2.83 | 261 | 1,690 | 3/3 / 3/3 |
| multiline-function | whole | 0.26 / 0.27 | 2.09 / 2.18 | 101 | 1,690 | 3/3 / 3/3 |
| multiline-function | udiff | 0.32 / 0.33 | 2.09 / 2.38 | 234 | 1,690 | 3/3 / 3/3 |
| unicode | diff | 0.36 / 0.40 | 2.37 / 2.74 | 164 | 1,286 | 3/3 / 3/3 |
| unicode | whole | 0.26 / 0.27 | 3.03 / 3.06 | 71 | 1,286 | 3/3 / 3/3 |
| unicode | udiff | 0.33 / 0.35 | 2.35 / 2.43 | 147 | 1,286 | 3/3 / 3/3 |
| crlf | diff | 0.36 / 0.38 | 2.41 / 2.62 | 135 | 1,227 | 3/3 / 3/3 |
| crlf | whole | 0.26 / 0.29 | 2.47 / 2.57 | 58 | 1,227 | 3/3 / 3/3 |
| crlf | udiff | 0.32 / 0.33 | 2.70 / 2.80 | 112 | 1,227 | 3/3 / 3/3 |
| no-final-newline | diff | 0.35 / 0.37 | 2.36 / 2.56 | 109 | 1,096 | 3/3 / 3/3 |
| no-final-newline | whole | 0.26 / 0.26 | 2.45 / 2.46 | 45 | 1,096 | 3/3 / 3/3 |
| no-final-newline | udiff | 0.33 / 0.37 | 2.45 / 2.46 | 98 | 1,096 | 3/3 / 3/3 |
| disambiguated-block | diff | 0.38 / 0.40 | 2.52 / 4.42 | 167 | 1,287 | 3/3 / 3/3 |
| disambiguated-block | whole | 0.26 / 0.26 | 2.07 / 2.12 | 74 | 1,287 | 3/3 / 3/3 |
| disambiguated-block | udiff | 0.32 / 0.36 | 2.05 / 2.07 | 119 | 1,287 | 3/3 / 3/3 |
| global-replace | diff | 0.36 / 0.37 | 2.66 / 2.73 | 499 | 2,831 | 3/3 / 3/3 |
| global-replace | whole | 0.28 / 0.28 | 2.83 / 2.95 | 240 | 2,831 | 3/3 / 3/3 |
| global-replace | udiff | 0.34 / 0.40 | 2.88 / 3.03 | 542 | 2,831 | 3/3 / 3/3 |
| same-file-batch | diff | 0.32 / 0.34 | 2.23 / 2.24 | 101 | 1,059 | 3/3 / 3/3 |
| same-file-batch | whole | 0.23 / 0.24 | 2.71 / 2.78 | 42 | 1,059 | 3/3 / 3/3 |
| same-file-batch | udiff | 0.28 / 0.38 | 2.29 / 3.07 | 101 | 1,059 | 3/3 / 3/3 |
| multi-file-batch | diff | 0.74 / 0.77 | 6.36 / 7.44 | 230 | 2,805 | 3/3 / 3/3 |
| multi-file-batch | whole | 0.47 / 0.48 | 7.99 / 8.08 | 78 | 2,805 | 3/3 / 3/3 |
| multi-file-batch | udiff | 0.61 / 0.69 | 7.00 / 7.27 | 209 | 2,805 | 3/3 / 3/3 |
| long-line | diff | 0.60 / 0.63 | 4.29 / 5.24 | 80,089 | 320,995 | 3/3 / 3/3 |
| long-line | whole | 0.24 / 0.28 | 4.28 / 4.32 | 40,035 | 320,995 | 3/3 / 3/3 |
| long-line | udiff | 0.47 / 0.48 | 4.03 / 4.15 | 80,088 | 320,995 | 3/3 / 3/3 |
| insert-after-anchor | diff | 0.31 / 0.34 | 2.58 / 3.09 | 122 | 1,124 | 3/3 / 3/3 |
| insert-after-anchor | whole | 0.19 / 0.23 | 2.55 / 2.57 | 57 | 1,124 | 3/3 / 3/3 |
| insert-after-anchor | udiff | 0.26 / 0.30 | 2.29 / 2.36 | 101 | 1,124 | 3/3 / 3/3 |
| delete-block | diff | 0.33 / 0.38 | 2.58 / 2.59 | 107 | 1,077 | 3/3 / 3/3 |
| delete-block | whole | 0.23 / 0.24 | 2.58 / 2.61 | 30 | 1,077 | 3/3 / 3/3 |
| delete-block | udiff | 0.27 / 0.32 | 2.47 / 2.49 | 102 | 1,077 | 3/3 / 3/3 |
| aider-create | diff | 0.47 / 0.51 | 2.61 / 2.64 | 67 | 880 | 3/3 / 3/3 |
| aider-create | whole | 0.41 / 0.42 | 2.46 / 2.52 | 28 | 880 | 3/3 / 3/3 |
| aider-create | udiff | 0.44 / 0.47 | 2.97 / 3.77 | 67 | 880 | 3/3 / 3/3 |
| aider-whole-overwrite | diff | 0.30 / 0.34 | 2.40 / 4.59 | 67 | 883 | 3/3 / 3/3 |
| aider-whole-overwrite | whole | 0.22 / 0.22 | 2.19 / 2.24 | 24 | 883 | 3/3 / 3/3 |
| aider-whole-overwrite | udiff | 0.27 / 0.27 | 2.61 / 2.74 | 66 | 883 | 3/3 / 3/3 |
| aider-noop | diff | 0.31 / 0.34 | 0.56 / 0.67 | 69 | 649 | 3/3 / 3/3 |
| aider-noop | whole | 0.22 / 0.22 | 0.45 / 0.51 | 25 | 649 | 3/3 / 3/3 |
| aider-noop | udiff | 0.01 / 0.01 | 0.02 / 0.02 | 12 | 0 | 3/3 / 3/3 |
| aider-reject-missing | diff | 0.64 / 0.64 | 0.73 / 0.76 | 71 | 0 | 3/3 / 3/3 |
| aider-reject-missing | udiff | 0.52 / 0.55 | 0.57 / 0.58 | 61 | 0 | 3/3 / 3/3 |
| aider-approval-denied | diff | 0.29 / 0.36 | 0.33 / 0.35 | 65 | 0 | 3/3 / 3/3 |
| aider-approval-denied | whole | 0.23 / 0.32 | 0.22 / 0.23 | 22 | 0 | 3/3 / 3/3 |
| aider-approval-denied | udiff | 0.20 / 0.20 | 0.20 / 0.24 | 62 | 0 | 3/3 / 3/3 |
| aider-dry-run | diff | 0.20 / 0.22 | 0.28 / 0.33 | 67 | 0 | 3/3 / 3/3 |
| aider-dry-run | whole | 0.14 / 0.15 | 0.12 / 0.13 | 24 | 0 | 3/3 / 3/3 |
| aider-dry-run | udiff | 0.16 / 0.17 | 0.20 / 0.21 | 66 | 0 | 3/3 / 3/3 |
| aider-latin1 | diff | 0.32 / 0.35 | 2.60 / 2.65 | 79 | 1,052 | 3/3 / 3/3 |
| aider-latin1 | whole | 0.22 / 0.22 | 2.71 / 2.71 | 30 | 1,052 | 3/3 / 3/3 |
| aider-latin1 | udiff | 0.29 / 0.29 | 2.32 / 2.35 | 77 | 1,052 | 3/3 / 3/3 |
| aider-crlf-output | diff | 0.33 / 0.35 | 2.38 / 2.42 | 85 | 1,003 | 3/3 / 3/3 |
| aider-crlf-output | whole | 0.23 / 0.47 | 2.39 / 2.50 | 33 | 1,003 | 3/3 / 3/3 |
| aider-crlf-output | udiff | 0.29 / 0.32 | 2.45 / 2.59 | 81 | 1,003 | 3/3 / 3/3 |
| aider-large-65536 | diff | 0.89 / 0.91 | 5.20 / 5.43 | 131,131 | 459,624 | 3/3 / 3/3 |
| aider-large-65536 | whole | 0.29 / 0.29 | 4.30 / 5.46 | 65,556 | 459,624 | 3/3 / 3/3 |
| aider-large-65536 | udiff | 0.53 / 0.58 | 4.79 / 5.21 | 65,608 | 459,624 | 3/3 / 3/3 |
| aider-large-1048576 | diff | 8.87 / 9.20 | 47.72 / 48.15 | 2,097,211 | 7,340,904 | 3/3 / 3/3 |
| aider-large-1048576 | whole | 1.03 / 1.07 | 38.34 / 40.74 | 1,048,596 | 7,340,904 | 3/3 / 3/3 |
| aider-large-1048576 | udiff | 4.43 / 6.89 | 43.76 / 44.45 | 1,048,648 | 7,340,904 | 3/3 / 3/3 |
| aider-large-5242880 | diff | 64.77 / 65.77 | 242.98 / 245.95 | 10,485,819 | 36,701,032 | 3/3 / 3/3 |
| aider-large-5242880 | whole | 7.53 / 7.68 | 184.85 / 187.30 | 5,242,900 | 36,701,032 | 3/3 / 3/3 |
| aider-large-5242880 | udiff | 39.23 / 39.33 | 211.49 / 222.91 | 5,242,952 | 36,701,032 | 3/3 / 3/3 |

## Reproduce

```sh
python3.13 benches/native/aider_native_setup.py
target/native-hosts/aider-5dc9490/aider-venv/bin/python benches/aider_provider.py --executable target/release/xnip
```

See [SDK setup and restoration](../../../aider-native.md) and the JSON companion for per-sample receipts, exact input/output/file hashes, SDK sources, dependencies, initialization costs and bridge counters.
