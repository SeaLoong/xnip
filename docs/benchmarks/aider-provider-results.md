# Aider scripting IO provider benchmark

Pinned commit `5dc9490bb35f9729ef2c95d00a19ccd30c26339c`; 24 fixtures across diff/whole/udiff, 142 groups and 426 measured workflows. Zero model completions.

The real Coder is created through its scripting IO parameter. Every measured reply enters its original apply_updates pipeline: native parser, dry-run matching, file authorization and final format executor. The original lane writes through InputOutput; the native lane delegates its reading/presentation/confirmation and persists text through xnip core. Both lanes retain identical format classes, raw prompt templates, supplied reply bytes and actual apply_updates receipts/diagnostics.

Aider describes this Python scripting API as version-sensitive. This cohort pins its source revision and full hashed Python dependencies; it measures this local scripting seam. SDK construction, source/model metadata setup, fixture reset, reply generation and file validation stay outside edit timing. Complete SEARCH/REPLACE or whole-file representations and three-line unified diffs are supplied from known fixture contents. Interactive user/model latency and final agent-facing message assembly have their own boundaries.

The host-selected LF policy and each format's explicit trailing-newline representation are declared separately from the original literal byte goal. Both independent file oracles and original/native result parity are retained. Aider's initial empty-file creation stays in its authorization pipeline. Git commit scheduling remains under Aider; benchmarks select auto_commits=False and use_git=False. Model.send_completion and the actual LiteLLM completion method are trapped and counted at zero.

Executable SHA-256 `144f76db0a340d7eede61b7fc2545c8c7c1d7d937300f378fd6c77c8da3e0688`; source `7553f05ad2ee7459b362acbb41eb6a824fcec088e53c6abd8a3986b02612d464`. One warm-up and three measured workflows per group by default; nearest-rank p95 with three samples is the maximum.

| Fixture | Format | Original median / p95 ms | Native median / p95 ms | Input bytes | Native IPC bytes | Original / native host-goal exact |
|---|---|---:|---:|---:|---:|---:|
| tiny-config | diff | 0.31 / 0.31 | 2.46 / 2.81 | 135 | 1,171 | 3/3 / 3/3 |
| tiny-config | whole | 0.21 / 0.23 | 2.31 / 2.35 | 58 | 1,171 | 3/3 / 3/3 |
| tiny-config | udiff | 0.27 / 0.28 | 2.81 / 2.90 | 112 | 1,171 | 3/3 / 3/3 |
| large-local-change | diff | 2.37 / 2.40 | 6.09 / 6.33 | 93,837 | 294,410 | 3/3 / 3/3 |
| large-local-change | whole | 0.52 / 0.61 | 4.35 / 6.85 | 46,908 | 294,410 | 3/3 / 3/3 |
| large-local-change | udiff | 0.31 / 0.33 | 4.48 / 4.65 | 259 | 294,410 | 3/3 / 3/3 |
| multiline-function | diff | 0.30 / 0.32 | 2.44 / 2.76 | 261 | 1,690 | 3/3 / 3/3 |
| multiline-function | whole | 0.20 / 0.21 | 2.31 / 2.41 | 101 | 1,690 | 3/3 / 3/3 |
| multiline-function | udiff | 0.28 / 0.28 | 2.32 / 2.41 | 234 | 1,690 | 3/3 / 3/3 |
| unicode | diff | 0.30 / 0.32 | 2.40 / 2.41 | 164 | 1,286 | 3/3 / 3/3 |
| unicode | whole | 0.20 / 0.26 | 2.35 / 2.43 | 71 | 1,286 | 3/3 / 3/3 |
| unicode | udiff | 0.27 / 0.28 | 2.35 / 2.37 | 147 | 1,286 | 3/3 / 3/3 |
| crlf | diff | 0.29 / 0.31 | 2.48 / 2.62 | 135 | 1,227 | 3/3 / 3/3 |
| crlf | whole | 0.19 / 0.22 | 2.27 / 2.36 | 58 | 1,227 | 3/3 / 3/3 |
| crlf | udiff | 0.27 / 0.28 | 2.51 / 2.53 | 112 | 1,227 | 3/3 / 3/3 |
| no-final-newline | diff | 0.29 / 0.30 | 2.84 / 2.85 | 109 | 1,096 | 3/3 / 3/3 |
| no-final-newline | whole | 0.21 / 0.25 | 2.38 / 2.42 | 45 | 1,096 | 3/3 / 3/3 |
| no-final-newline | udiff | 0.28 / 0.30 | 4.40 / 9.25 | 98 | 1,096 | 3/3 / 3/3 |
| disambiguated-block | diff | 0.30 / 0.32 | 2.43 / 2.79 | 167 | 1,287 | 3/3 / 3/3 |
| disambiguated-block | whole | 0.20 / 0.23 | 2.39 / 2.60 | 74 | 1,287 | 3/3 / 3/3 |
| disambiguated-block | udiff | 0.26 / 0.31 | 2.31 / 2.33 | 119 | 1,287 | 3/3 / 3/3 |
| global-replace | diff | 0.32 / 0.33 | 2.78 / 2.95 | 499 | 2,831 | 3/3 / 3/3 |
| global-replace | whole | 0.21 / 0.24 | 2.49 / 2.82 | 240 | 2,831 | 3/3 / 3/3 |
| global-replace | udiff | 0.29 / 0.36 | 2.48 / 2.50 | 542 | 2,831 | 3/3 / 3/3 |
| same-file-batch | diff | 0.29 / 0.32 | 2.53 / 2.56 | 101 | 1,059 | 3/3 / 3/3 |
| same-file-batch | whole | 0.20 / 0.22 | 1.99 / 2.02 | 42 | 1,059 | 3/3 / 3/3 |
| same-file-batch | udiff | 0.26 / 0.35 | 2.50 / 2.54 | 101 | 1,059 | 3/3 / 3/3 |
| multi-file-batch | diff | 0.71 / 0.73 | 7.33 / 7.69 | 230 | 2,805 | 3/3 / 3/3 |
| multi-file-batch | whole | 0.40 / 0.40 | 7.18 / 7.21 | 78 | 2,805 | 3/3 / 3/3 |
| multi-file-batch | udiff | 0.57 / 0.58 | 8.03 / 8.74 | 209 | 2,805 | 3/3 / 3/3 |
| long-line | diff | 0.70 / 0.78 | 4.17 / 4.22 | 80,089 | 320,995 | 3/3 / 3/3 |
| long-line | whole | 0.27 / 0.28 | 3.35 / 3.53 | 40,035 | 320,995 | 3/3 / 3/3 |
| long-line | udiff | 0.44 / 0.45 | 3.86 / 3.95 | 80,088 | 320,995 | 3/3 / 3/3 |
| insert-after-anchor | diff | 0.31 / 0.34 | 2.19 / 2.51 | 122 | 1,124 | 3/3 / 3/3 |
| insert-after-anchor | whole | 0.20 / 0.21 | 2.53 / 2.53 | 57 | 1,124 | 3/3 / 3/3 |
| insert-after-anchor | udiff | 0.29 / 0.31 | 2.50 / 2.54 | 101 | 1,124 | 3/3 / 3/3 |
| delete-block | diff | 0.30 / 0.32 | 2.50 / 2.53 | 107 | 1,077 | 3/3 / 3/3 |
| delete-block | whole | 0.21 / 0.26 | 2.41 / 2.58 | 30 | 1,077 | 3/3 / 3/3 |
| delete-block | udiff | 0.26 / 0.29 | 2.54 / 2.62 | 102 | 1,077 | 3/3 / 3/3 |
| aider-create | diff | 0.55 / 0.63 | 2.48 / 2.61 | 67 | 880 | 3/3 / 3/3 |
| aider-create | whole | 0.47 / 0.52 | 2.61 / 2.68 | 28 | 880 | 3/3 / 3/3 |
| aider-create | udiff | 0.59 / 0.59 | 2.47 / 2.60 | 67 | 880 | 3/3 / 3/3 |
| aider-whole-overwrite | diff | 0.33 / 0.40 | 2.53 / 2.60 | 67 | 883 | 3/3 / 3/3 |
| aider-whole-overwrite | whole | 0.20 / 0.27 | 2.29 / 2.40 | 24 | 883 | 3/3 / 3/3 |
| aider-whole-overwrite | udiff | 0.25 / 0.26 | 2.32 / 2.65 | 66 | 883 | 3/3 / 3/3 |
| aider-noop | diff | 0.29 / 0.30 | 0.62 / 0.77 | 69 | 649 | 3/3 / 3/3 |
| aider-noop | whole | 0.20 / 0.20 | 0.46 / 0.49 | 25 | 649 | 3/3 / 3/3 |
| aider-noop | udiff | 0.01 / 0.01 | 0.02 / 0.02 | 12 | 0 | 3/3 / 3/3 |
| aider-reject-missing | diff | 0.67 / 0.71 | 0.77 / 0.89 | 71 | 0 | 3/3 / 3/3 |
| aider-reject-missing | udiff | 0.57 / 0.80 | 0.57 / 0.65 | 61 | 0 | 3/3 / 3/3 |
| aider-approval-denied | diff | 0.30 / 0.30 | 0.33 / 0.33 | 65 | 0 | 3/3 / 3/3 |
| aider-approval-denied | whole | 0.27 / 0.33 | 0.23 / 0.23 | 22 | 0 | 3/3 / 3/3 |
| aider-approval-denied | udiff | 0.24 / 0.27 | 0.21 / 0.22 | 62 | 0 | 3/3 / 3/3 |
| aider-dry-run | diff | 0.21 / 0.21 | 0.30 / 0.33 | 67 | 0 | 3/3 / 3/3 |
| aider-dry-run | whole | 0.11 / 0.12 | 0.13 / 0.13 | 24 | 0 | 3/3 / 3/3 |
| aider-dry-run | udiff | 0.19 / 0.23 | 0.21 / 0.24 | 66 | 0 | 3/3 / 3/3 |
| aider-latin1 | diff | 0.31 / 0.39 | 3.33 / 4.15 | 79 | 1,052 | 3/3 / 3/3 |
| aider-latin1 | whole | 0.20 / 0.22 | 1.99 / 2.06 | 30 | 1,052 | 3/3 / 3/3 |
| aider-latin1 | udiff | 0.25 / 0.30 | 2.46 / 2.58 | 77 | 1,052 | 3/3 / 3/3 |
| aider-crlf-output | diff | 0.37 / 0.62 | 2.32 / 2.46 | 85 | 1,003 | 3/3 / 3/3 |
| aider-crlf-output | whole | 0.23 / 0.53 | 2.04 / 2.17 | 33 | 1,003 | 3/3 / 3/3 |
| aider-crlf-output | udiff | 0.27 / 0.28 | 2.51 / 2.71 | 81 | 1,003 | 3/3 / 3/3 |
| aider-large-65536 | diff | 0.87 / 0.89 | 4.89 / 5.02 | 131,131 | 459,624 | 3/3 / 3/3 |
| aider-large-65536 | whole | 0.25 / 0.26 | 4.20 / 4.21 | 65,556 | 459,624 | 3/3 / 3/3 |
| aider-large-65536 | udiff | 0.53 / 0.58 | 4.18 / 4.21 | 65,608 | 459,624 | 3/3 / 3/3 |
| aider-large-1048576 | diff | 8.93 / 9.05 | 46.26 / 46.95 | 2,097,211 | 7,340,904 | 3/3 / 3/3 |
| aider-large-1048576 | whole | 1.03 / 1.04 | 37.36 / 37.48 | 1,048,596 | 7,340,904 | 3/3 / 3/3 |
| aider-large-1048576 | udiff | 4.51 / 7.21 | 40.88 / 42.14 | 1,048,648 | 7,340,904 | 3/3 / 3/3 |
| aider-large-5242880 | diff | 64.15 / 64.97 | 248.74 / 264.05 | 10,485,819 | 36,701,032 | 3/3 / 3/3 |
| aider-large-5242880 | whole | 7.14 / 7.25 | 189.44 / 200.36 | 5,242,900 | 36,701,032 | 3/3 / 3/3 |
| aider-large-5242880 | udiff | 38.67 / 39.16 | 208.84 / 211.78 | 5,242,952 | 36,701,032 | 3/3 / 3/3 |

## Reproduce

```sh
python3.13 benches/native/aider_native_setup.py
target/native-hosts/aider-5dc9490/aider-venv/bin/python benches/aider_provider.py --executable target/release/xnip
```

See [SDK setup and restoration](../aider-native.md) and the JSON companion for per-sample receipts, exact input/output/file hashes, SDK sources, dependencies, initialization costs and bridge counters.
