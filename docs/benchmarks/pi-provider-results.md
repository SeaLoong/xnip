# Pi native provider benchmark

Pi 1.3.0; 20 fixtures; 3 measured runs and 1 warm-up per fixture/provider. Measured 2026-11-05T22:43:36.339Z. Zero model calls, checked with the real runtime counter and idle session.

Both lanes execute the official SDK read/edit tools and mutation queue through runToolCall and its before/after hooks. The native lane retains the host schemas, argument preparation, matching and renderers; xnip supplies IO. The catalog and metadata are checked equal. This Pi cohort retains its own governance and public result boundary.

Each file is read before editing. Edits in one file are grouped in edits[] using original-file coordinates. Global replacement uses a unique full-file block. Fixture creation, byte verification and serialization counting are outside tool timing. One persistent session serves each lane. Setup includes session creation and native handshake; SDK bootstrap is separate. Medians and nearest-rank p95 use the raw samples; with three runs p95 is the maximum.

Binary release, SHA-256 `5504d4f5ce31d9b04852bc61bba1d9bc7341e0bbd5fcf0fc6c24ab12d3e87ab1`; source `f7d762e855bff45a521838762324a78c05d8b550ee05b345e1b7fe063ef96f0c`. Node v24.22.1; linux/x64; AMD EPYC 9V74 80-Core Processor. SDK bootstrap 641.31 ms.

| Provider | Session setup ms |
|---|---:|
| pi-original | 14.12 |
| xnip-native | 25.57 |

| Fixture | Original edit median / p95 ms | xnip edit median / p95 ms | Original / xnip read median ms | Original / xnip tool bytes | Original / xnip correct |
|---|---:|---:|---:|---:|---:|
| tiny-config | 0.91 / 1.05 | 3.35 / 6.29 | 0.58 / 0.48 | 914 / 914 | 3/3 / 3/3 |
| large-local-change | 1.93 / 1.93 | 6.04 / 6.34 | 0.59 / 0.75 | 99,140 / 99,140 | 3/3 / 3/3 |
| multiline-function | 0.53 / 0.59 | 2.75 / 3.20 | 0.46 / 0.34 | 1,496 / 1,496 | 3/3 / 3/3 |
| unicode | 0.48 / 2.40 | 2.78 / 3.36 | 0.35 / 0.32 | 1,009 / 1,009 | 3/3 / 3/3 |
| crlf | 0.71 / 1.66 | 2.76 / 2.96 | 0.48 / 0.29 | 891 / 891 | 3/3 / 3/3 |
| no-final-newline | 0.57 / 1.04 | 2.31 / 2.44 | 0.55 / 0.29 | 935 / 935 | 3/3 / 3/3 |
| disambiguated-block | 0.60 / 0.71 | 2.20 / 2.26 | 0.45 / 0.32 | 1,076 / 1,076 | 3/3 / 3/3 |
| global-replace | 0.77 / 0.97 | 2.42 / 2.62 | 0.48 / 0.31 | 2,831 / 2,831 | 3/3 / 3/3 |
| same-file-batch | 0.60 / 0.92 | 2.24 / 2.78 | 0.44 / 0.27 | 916 / 916 | 3/3 / 3/3 |
| multi-file-batch | 1.69 / 1.76 | 6.72 / 7.29 | 1.08 / 0.91 | 2,332 / 2,332 | 3/3 / 3/3 |
| long-line | 1.02 / 1.88 | 4.00 / 4.10 | 0.59 / 0.53 | 240,804 / 240,804 | 3/3 / 3/3 |
| insert-after-anchor | 0.51 / 0.54 | 2.36 / 2.36 | 0.47 / 0.32 | 924 / 924 | 3/3 / 3/3 |
| delete-block | 0.59 / 0.63 | 2.25 / 2.46 | 0.46 / 0.31 | 913 / 913 | 3/3 / 3/3 |
| large-65536 | 1.15 / 1.43 | 4.82 / 5.18 | 0.58 / 0.70 | 156,982 / 156,982 | 3/3 / 3/3 |
| large-1048576 | 13.06 / 17.51 | 57.35 / 58.85 | 2.22 / 5.79 | 157,007 / 157,007 | 3/3 / 3/3 |
| large-5242880 | 70.22 / 71.86 | 277.99 / 286.41 | 8.18 / 26.31 | 157,020 / 157,020 | 3/3 / 3/3 |
| pi-fuzzy-quote | 0.70 / 0.79 | 3.17 / 3.77 | 0.46 / 0.42 | 961 / 961 | 3/3 / 3/3 |
| pi-reject-missing | 0.40 / 0.47 | 0.36 / 0.41 | 0.45 / 0.40 | 554 / 554 | 3/3 / 3/3 |
| pi-reject-ambiguous | 0.37 / 0.38 | 0.25 / 0.30 | 0.45 / 0.36 | 572 / 572 | 3/3 / 3/3 |
| pi-noop | 0.44 / 0.57 | 0.24 / 0.28 | 0.48 / 0.35 | 574 / 574 | 3/3 / 3/3 |

The independent oracle checks final bytes, expected host error text and the complete fixture file set. Raw JSON retains each call, local argument/result bytes, file hashes, model counter and source/package fingerprints. The historical 13/41 matrix remains its original cohort. Scheduling, GC, page cache and a small run count affect observed latency.

## Reproduce

```sh
node benches/pi_provider.mjs --executable target/release/xnip --repeats 3 --warmups 1
node --test-reporter=tap tests/pi_provider_bench_contract.mjs
```

The runtime fixture prepares pinned official Pi 1.3.0 packages in target/native-bench/pi-v1.3.0. See the [DSH provider cohort](native-provider-results.md) for its separately measured tool, governance and native-IPC boundary.
