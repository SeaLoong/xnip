# Native filesystem provider benchmark

Measured 2026-11-05T22:42:04.676Z. 32 fixtures, 3 measured runs and 1 warm-up per provider/host/policy. Zero model calls.

Native bridge selection: provider selects UTF-8 when advertised, otherwise legacy byte arrays.

Both lanes execute the official DSH `read`, `write` and `edit` tools, observation policy and sandbox policy. The original lane uses its packaged filesystem; the xnip lane replaces that provider. Tools, schemas and prompts are checked equal. This cohort measures local execution and storage bytes.

The original 13 fixture names are retained. Batch fixtures run one official tool call per edit, matching the native tool API; their results represent a sequence of edits.

Fixtures supply known replacement text. Prerequisite reads retain the official bounded output window, including for edits near the end of large files.

Fixture reset and byte verification run outside timing. Read and mutation times are summed tool execution durations on a persistent host and native process. Setup includes provider construction and the native handshake; the first official module import is cold. Median and nearest-rank p95 use the measured samples; with three runs, p95 is the maximum.

The byte oracle preserves original UTF-8 BOM, CRLF, mixed line endings and EOF bytes outside the requested edit. A host-normalized result appears as a byte mismatch even when the tool reports success. Rejection cases must return the expected host error and preserve the expected storage bytes. Timing comparisons should be read alongside correctness.

Binary: xnip 0.5.7, release, SHA-256 `5504d4f5ce31d9b04852bc61bba1d9bc7341e0bbd5fcf0fc6c24ab12d3e87ab1`. Node v24.22.1; linux/x64; AMD EPYC 9V74 80-Core Processor. Source digest `945eb2cf93b09832b8a09e0520eefc69034fa9f8784d4a438bbeae7c3684e935`. Complete file and package fingerprints are in the JSON companion.

## Host setup

| DSH | Policy | Provider | Setup ms |
|---|---|---|---:|
| 0.2.1-rc.2 | workspace-write | dsh-original | 125.37 |
| 0.2.1-rc.2 | workspace-write | xnip-native | 16.23 |
| 0.2.1-rc.2 | read-only | dsh-original | 5.08 |
| 0.2.1-rc.2 | read-only | xnip-native | 15.32 |
| 0.2.1-rc.5 | workspace-write | dsh-original | 94.78 |
| 0.2.1-rc.5 | workspace-write | xnip-native | 16.00 |
| 0.2.1-rc.5 | read-only | dsh-original | 5.26 |
| 0.2.1-rc.5 | read-only | xnip-native | 14.54 |

## DSH 0.2.1-rc.2

Times are mutation median / p95 in milliseconds; read median is shown separately. Bridge bytes count actual native stdin + stdout during tool execution. Tool bytes count serialized arguments + complete local results; they describe this API boundary.

| Fixture | Original mutation ms | xnip mutation ms | Original / xnip read ms | Original / xnip correct | xnip bridge bytes | Original / xnip tool bytes |
|---|---:|---:|---:|---:|---:|---:|
| tiny-config | 4.02 / 5.77 | 5.19 / 5.56 | 1.97 / 1.36 | 3/3 / 3/3 | 1,791 | 1,931 / 1,931 |
| large-local-change | 4.50 / 10.48 | 8.75 / 12.84 | 33.03 / 33.79 | 3/3 / 3/3 | 246,265 | 351,076 / 351,076 |
| multiline-function | 3.29 / 5.12 | 6.46 / 11.54 | 1.10 / 1.09 | 3/3 / 3/3 | 2,435 | 2,810 / 2,810 |
| unicode | 2.53 / 2.58 | 4.19 / 4.21 | 1.31 / 1.11 | 3/3 / 3/3 | 1,891 | 1,968 / 1,968 |
| crlf | 2.45 / 2.73 | 5.67 / 9.05 | 0.95 / 1.25 | 3/3 / 3/3 | 1,786 | 1,868 / 1,868 |
| no-final-newline | 2.31 / 2.41 | 5.70 / 9.33 | 0.92 / 1.36 | 3/3 / 3/3 | 1,797 | 1,829 / 1,829 |
| disambiguated-block | 2.37 / 2.38 | 11.70 / 13.88 | 1.00 / 1.25 | 3/3 / 3/3 | 1,987 | 2,227 / 2,227 |
| global-replace | 2.65 / 5.60 | 19.63 / 20.92 | 1.56 / 1.54 | 3/3 / 3/3 | 3,277 | 4,200 / 4,200 |
| same-file-batch | 5.00 / 5.32 | 16.82 / 31.06 | 1.15 / 1.26 | 3/3 / 3/3 | 3,442 | 2,758 / 2,758 |
| multi-file-batch | 7.63 / 8.12 | 24.55 / 35.01 | 3.40 / 5.08 | 3/3 / 3/3 | 4,823 | 4,860 / 4,860 |
| long-line | 2.36 / 2.42 | 10.75 / 19.14 | 1.02 / 1.17 | 3/3 / 3/3 | 281,639 | 167,706 / 167,706 |
| insert-after-anchor | 3.47 / 4.88 | 4.48 / 5.45 | 1.05 / 1.36 | 3/3 / 3/3 | 1,828 | 1,959 / 1,959 |
| delete-block | 2.41 / 2.58 | 7.30 / 8.63 | 1.02 / 1.23 | 3/3 / 3/3 | 1,757 | 1,944 / 1,944 |
| bom-lf | 2.10 / 2.47 | 5.82 / 6.17 | 1.00 / 1.12 | 0/3 / 3/3 | 1,673 | 1,725 / 1,725 |
| bom-crlf | 2.47 / 3.29 | 3.82 / 5.71 | 0.95 / 1.40 | 0/3 / 3/3 | 1,713 | 1,743 / 1,743 |
| mixed-eol-span | 3.23 / 3.64 | 3.86 / 3.90 | 1.02 / 1.21 | 0/3 / 3/3 | 1,719 | 1,843 / 1,843 |
| mixed-eol-all | 3.12 / 3.63 | 4.23 / 4.89 | 1.00 / 1.05 | 0/3 / 3/3 | 1,727 | 1,903 / 1,903 |
| adjacent-edits | 4.97 / 9.78 | 9.20 / 9.88 | 1.06 / 1.04 | 3/3 / 3/3 | 3,301 | 2,579 / 2,579 |
| disjoint-three-edits | 6.43 / 8.91 | 15.15 / 17.72 | 0.97 / 1.18 | 3/3 / 3/3 | 5,231 | 3,811 / 3,811 |
| noop-lf | 0.29 / 0.30 | 0.27 / 0.27 | 0.93 / 1.20 | 3/3 / 3/3 | 0 | 1,130 / 1,130 |
| reject-missing | 0.90 / 0.91 | 1.30 / 1.31 | 0.96 / 1.13 | 3/3 / 3/3 | 702 | 1,424 / 1,328 |
| reject-ambiguous | 0.92 / 1.43 | 1.10 / 1.88 | 0.99 / 1.02 | 3/3 / 3/3 | 671 | 1,538 / 1,276 |
| reject-empty-search | 0.28 / 0.31 | 0.26 / 0.28 | 0.95 / 0.93 | 3/3 / 3/3 | 0 | 1,102 / 1,102 |
| reject-unobserved | 0.66 / 0.71 | 0.65 / 0.77 | 0.00 / 0.00 | 3/3 / 3/3 | 0 | 663 / 663 |
| reject-stale | 1.09 / 1.16 | 0.89 / 0.91 | 1.15 / 0.95 | 3/3 / 3/3 | 0 | 1,422 / 1,176 |
| reject-readonly | 0.67 / 0.78 | 0.69 / 0.79 | 1.00 / 1.01 | 3/3 / 3/3 | 0 | 1,503 / 1,503 |
| write-create | 4.92 / 5.40 | 5.88 / 6.94 | 0.00 / 0.00 | 3/3 / 3/3 | 1,322 | 722 / 722 |
| write-overwrite-observed | 3.86 / 5.53 | 8.05 / 9.24 | 1.27 / 1.25 | 3/3 / 3/3 | 1,393 | 1,731 / 1,731 |
| write-noop-observed | 2.83 / 2.84 | 2.06 / 2.18 | 1.42 / 0.94 | 3/3 / 3/3 | 933 | 1,604 / 1,604 |
| large-65536 | 3.13 / 3.74 | 11.84 / 13.92 | 8.99 / 7.02 | 3/3 / 3/3 | 332,169 | 309,261 / 309,261 |
| large-1048576 | 9.01 / 10.93 | 50.80 / 51.00 | 8.06 / 10.64 | 3/3 / 3/3 | 5,285,787 | 2,290,721 / 2,290,721 |
| large-5242880 | 39.96 / 40.46 | 203.80 / 223.04 | 10.28 / 11.30 | 3/3 / 3/3 | 26,421,149 | 10,744,867 / 10,744,867 |

Storage/outcome differences: `dsh-original/bom-lf` (0/3 byte exact; 3/3 expected outcome), `dsh-original/bom-crlf` (0/3 byte exact; 3/3 expected outcome), `dsh-original/mixed-eol-span` (0/3 byte exact; 3/3 expected outcome), `dsh-original/mixed-eol-all` (0/3 byte exact; 3/3 expected outcome).

## DSH 0.2.1-rc.5

Times are mutation median / p95 in milliseconds; read median is shown separately. Bridge bytes count actual native stdin + stdout during tool execution. Tool bytes count serialized arguments + complete local results; they describe this API boundary.

| Fixture | Original mutation ms | xnip mutation ms | Original / xnip read ms | Original / xnip correct | xnip bridge bytes | Original / xnip tool bytes |
|---|---:|---:|---:|---:|---:|---:|
| tiny-config | 3.13 / 3.85 | 3.89 / 3.93 | 1.68 / 1.00 | 3/3 / 3/3 | 1,791 | 1,931 / 1,931 |
| large-local-change | 4.37 / 5.22 | 7.43 / 9.07 | 33.94 / 35.96 | 3/3 / 3/3 | 246,265 | 351,076 / 351,076 |
| multiline-function | 2.65 / 2.72 | 4.37 / 4.40 | 1.05 / 1.08 | 3/3 / 3/3 | 2,435 | 2,810 / 2,810 |
| unicode | 2.60 / 2.70 | 4.09 / 4.26 | 1.06 / 0.97 | 3/3 / 3/3 | 1,891 | 1,968 / 1,968 |
| crlf | 2.38 / 2.56 | 4.15 / 4.22 | 0.97 / 1.41 | 3/3 / 3/3 | 1,786 | 1,868 / 1,868 |
| no-final-newline | 3.12 / 3.33 | 4.35 / 4.52 | 1.18 / 1.14 | 3/3 / 3/3 | 1,797 | 1,829 / 1,829 |
| disambiguated-block | 2.68 / 2.96 | 3.79 / 4.06 | 1.17 / 1.05 | 3/3 / 3/3 | 1,987 | 2,227 / 2,227 |
| global-replace | 2.89 / 5.86 | 3.64 / 4.10 | 1.53 / 1.18 | 3/3 / 3/3 | 3,277 | 4,200 / 4,200 |
| same-file-batch | 4.94 / 5.09 | 6.50 / 6.61 | 1.02 / 0.89 | 3/3 / 3/3 | 3,442 | 2,758 / 2,758 |
| multi-file-batch | 6.34 / 7.07 | 10.11 / 10.94 | 2.79 / 2.51 | 3/3 / 3/3 | 4,823 | 4,860 / 4,860 |
| long-line | 2.43 / 2.49 | 4.47 / 4.81 | 1.01 / 0.88 | 3/3 / 3/3 | 281,639 | 167,706 / 167,706 |
| insert-after-anchor | 2.13 / 2.36 | 3.50 / 3.55 | 0.94 / 0.91 | 3/3 / 3/3 | 1,828 | 1,959 / 1,959 |
| delete-block | 2.29 / 2.53 | 3.56 / 3.75 | 0.92 / 0.89 | 3/3 / 3/3 | 1,757 | 1,944 / 1,944 |
| bom-lf | 2.01 / 2.04 | 3.79 / 4.21 | 0.97 / 0.97 | 0/3 / 3/3 | 1,673 | 1,725 / 1,725 |
| bom-crlf | 2.09 / 2.18 | 4.17 / 5.55 | 0.95 / 0.95 | 0/3 / 3/3 | 1,713 | 1,743 / 1,743 |
| mixed-eol-span | 2.01 / 2.07 | 4.03 / 4.23 | 0.97 / 0.96 | 0/3 / 3/3 | 1,719 | 1,843 / 1,843 |
| mixed-eol-all | 2.26 / 2.29 | 4.30 / 4.35 | 1.00 / 0.96 | 0/3 / 3/3 | 1,727 | 1,903 / 1,903 |
| adjacent-edits | 4.15 / 4.18 | 6.46 / 6.49 | 0.95 / 0.94 | 3/3 / 3/3 | 3,301 | 2,579 / 2,579 |
| disjoint-three-edits | 5.90 / 6.01 | 12.42 / 12.45 | 0.93 / 0.97 | 3/3 / 3/3 | 5,231 | 3,811 / 3,811 |
| noop-lf | 0.26 / 0.30 | 0.25 / 0.25 | 0.84 / 0.91 | 3/3 / 3/3 | 0 | 1,130 / 1,130 |
| reject-missing | 0.82 / 0.82 | 0.98 / 1.10 | 0.88 / 0.92 | 3/3 / 3/3 | 702 | 1,424 / 1,328 |
| reject-ambiguous | 0.85 / 0.86 | 0.96 / 0.96 | 0.88 / 0.91 | 3/3 / 3/3 | 671 | 1,538 / 1,276 |
| reject-empty-search | 0.25 / 0.32 | 0.25 / 0.25 | 0.87 / 0.90 | 3/3 / 3/3 | 0 | 1,102 / 1,102 |
| reject-unobserved | 0.59 / 0.60 | 0.62 / 0.65 | 0.00 / 0.00 | 3/3 / 3/3 | 0 | 663 / 663 |
| reject-stale | 0.73 / 0.77 | 0.82 / 0.90 | 0.88 / 0.95 | 3/3 / 3/3 | 0 | 1,422 / 1,176 |
| reject-readonly | 0.66 / 0.86 | 0.72 / 0.74 | 1.53 / 0.95 | 3/3 / 3/3 | 0 | 1,503 / 1,503 |
| write-create | 2.23 / 2.60 | 3.00 / 3.28 | 0.00 / 0.00 | 3/3 / 3/3 | 1,322 | 722 / 722 |
| write-overwrite-observed | 2.33 / 2.52 | 3.35 / 3.36 | 0.90 / 0.95 | 3/3 / 3/3 | 1,393 | 1,731 / 1,731 |
| write-noop-observed | 2.16 / 2.45 | 1.23 / 1.31 | 0.93 / 0.92 | 3/3 / 3/3 | 933 | 1,604 / 1,604 |
| large-65536 | 2.55 / 2.64 | 5.52 / 6.89 | 6.92 / 6.69 | 3/3 / 3/3 | 332,169 | 309,261 / 309,261 |
| large-1048576 | 7.28 / 9.26 | 38.29 / 41.40 | 7.89 / 7.35 | 3/3 / 3/3 | 5,285,787 | 2,290,721 / 2,290,721 |
| large-5242880 | 32.66 / 45.01 | 177.65 / 201.04 | 13.26 / 9.41 | 3/3 / 3/3 | 26,421,149 | 10,744,867 / 10,744,867 |

Storage/outcome differences: `dsh-original/bom-lf` (0/3 byte exact; 3/3 expected outcome), `dsh-original/bom-crlf` (0/3 byte exact; 3/3 expected outcome), `dsh-original/mixed-eol-span` (0/3 byte exact; 3/3 expected outcome), `dsh-original/mixed-eol-all` (0/3 byte exact; 3/3 expected outcome).

## Reproduce

```sh
node benches/native/dsh_native_setup.mjs
CARGO_BUILD_JOBS=4 cargo build --release --locked --offline
node benches/native_provider.mjs --executable target/release/xnip --repeats 3 --warmups 1
node --test-reporter=tap tests/native_provider_bench_contract.mjs
```

The setup helper installs official host packages under `target/native-hosts/dsh-0.2.1-rc.2` and `dsh-0.2.1-rc.5`. The benchmark uses the Python standard library to load the unchanged original 13 fixtures. The runner verifies that measured source and executable hashes stay fixed throughout execution.

The separate [bridge encoding control](native-provider-encoding.md) compares legacy arrays with negotiated UTF-8 using the same executable and source.

Historical representation, 13-fixture and 41-profile reports retain their own cohorts and measurements. This report adds actual native-provider execution. Raw samples include per-call timings and errors, complete byte counts, file hashes, package versions and fingerprints. Scheduling, page cache, warm-up order and a small sample count affect latency; fresh-machine repetition is useful for wider performance conclusions.
