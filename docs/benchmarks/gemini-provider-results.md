# Gemini SDK filesystem provider benchmark

Gemini SDK 0.27.2; 30 fixtures, 3 measured runs and 1 warm-up per lane. Zero model calls. Measured 2026-11-06T02:34:23.270Z.

Both lanes use official Config and ToolRegistry services with read_file, replace and write_file. Each call builds the official invocation, checks the policy bus, performs any requested confirmation callback and executes the tool. The original service and xnip service retain identical registered schemas, matching, occurrence validation and rendering.

This cohort covers local SDK filesystem configurations for the pinned version. Config uses its supported disableLLMCorrection=true option in both lanes. The official empty fake-response generator counts attempted model calls; every measured workflow must keep that counter at zero. CLI profiles, OS sandbox sessions, interactive user delay and model/billing measurements have their own boundaries.

Fixtures supply known inputs. Global edits set expected_replacements to the fixture count; sequential edits use separate replace calls. Every existing file is read first except the policy-denied fixture. SDK bootstrap is timed separately, followed by per-policy provider setup. Warm timing includes the complete tool-call pipeline; fixture reset, JSON byte accounting and independent file verification run outside timing. Three samples make nearest-rank p95 the maximum.

The byte oracle preserves input bytes outside the requested change. The host newline policy can produce a different stored representation in both lanes; those differences remain visible. Provider compatibility separately requires identical resulting file hashes and actual SDK tool-result hashes. Local tool bytes count arguments, actual ToolResult JSON and separately recorded exception messages. Native bridge bytes count actual child stdin/stdout and describe an additional local boundary.

Binary SHA-256 `33bf6effc23db6f10fe8d5d1881e98003db95cb59a8af144a2a523fbc97f3884` (release); maintained source `01bbcff41d819fbcd533a7d3309eacb5d956bed283be04f73056f367e047dbab`. Node v24.22.1; linux/x64; AMD EPYC 9V74 80-Core Processor. SDK bootstrap 5344.01 ms.

| Policy | Provider | Setup ms | Setup bridge bytes |
|---|---|---:|---:|
| allow | gemini-original | 36.54 | 0 |
| allow | xnip-native | 8.29 | 330 |
| ask_user | gemini-original | 1.00 | 0 |
| ask_user | xnip-native | 6.69 | 330 |
| deny | gemini-original | 0.81 | 0 |
| deny | xnip-native | 6.50 | 330 |

| Fixture | Original mutation median / p95 ms | xnip mutation median / p95 ms | Original / xnip read median ms | Original / xnip tool bytes | xnip bridge bytes | Original / xnip byte+outcome exact |
|---|---:|---:|---:|---:|---:|---:|
| tiny-config | 0.94 / 0.99 | 3.08 / 3.82 | 1.00 / 0.69 | 1,204 / 1,204 | 1,561 | 3/3 / 3/3 |
| large-local-change | 4.24 / 4.59 | 6.68 / 7.00 | 1.38 / 0.76 | 148,361 / 148,361 | 343,705 | 3/3 / 3/3 |
| multiline-function | 0.77 / 0.95 | 3.60 / 3.84 | 0.79 / 0.58 | 1,703 / 1,703 | 2,223 | 3/3 / 3/3 |
| unicode | 0.69 / 0.71 | 2.87 / 2.95 | 0.73 / 0.52 | 1,293 / 1,293 | 1,669 | 3/3 / 3/3 |
| crlf | 0.74 / 0.78 | 2.64 / 2.93 | 0.74 / 0.55 | 1,185 / 1,185 | 1,568 | 3/3 / 3/3 |
| no-final-newline | 0.70 / 0.70 | 2.57 / 3.23 | 0.81 / 0.54 | 1,227 / 1,227 | 1,535 | 3/3 / 3/3 |
| disambiguated-block | 0.66 / 0.88 | 2.97 / 2.97 | 0.67 / 0.69 | 1,358 / 1,358 | 1,757 | 3/3 / 3/3 |
| global-replace | 0.91 / 0.93 | 3.16 / 3.18 | 0.79 / 0.54 | 2,329 / 2,329 | 3,448 | 3/3 / 3/3 |
| same-file-batch | 1.25 / 1.49 | 4.96 / 5.32 | 0.64 / 0.53 | 2,191 / 2,191 | 2,924 | 3/3 / 3/3 |
| multi-file-batch | 2.15 / 2.16 | 9.11 / 10.43 | 2.28 / 1.89 | 3,221 / 3,221 | 4,001 | 3/3 / 3/3 |
| long-line | 1.02 / 1.04 | 3.89 / 4.17 | 0.86 / 0.68 | 163,507 / 163,507 | 361,361 | 3/3 / 3/3 |
| insert-after-anchor | 0.70 / 0.72 | 2.65 / 3.09 | 0.74 / 0.68 | 1,229 / 1,229 | 1,570 | 3/3 / 3/3 |
| delete-block | 0.72 / 0.73 | 2.47 / 2.49 | 0.74 / 0.62 | 1,179 / 1,179 | 1,487 | 3/3 / 3/3 |
| large-65536 | 1.64 / 1.78 | 5.18 / 9.87 | 0.89 / 0.71 | 199,579 / 199,579 | 463,955 | 3/3 / 3/3 |
| large-1048576 | 11.88 / 12.83 | 43.40 / 45.93 | 1.78 / 1.35 | 2,373,365 / 2,373,365 | 7,399,013 | 3/3 / 3/3 |
| large-5242880 | 71.89 / 75.98 | 230.62 / 243.80 | 3.66 / 3.23 | 10,827,514 / 10,827,514 | 36,988,519 | 3/3 / 3/3 |
| gemini-regex-spacing | 0.73 / 0.74 | 2.55 / 2.67 | 0.70 / 0.51 | 1,215 / 1,215 | 1,510 | 3/3 / 3/3 |
| gemini-reject-missing | 0.38 / 0.40 | 0.31 / 0.39 | 0.71 / 0.51 | 1,040 / 1,040 | 247 | 3/3 / 3/3 |
| gemini-reject-count | 0.41 / 0.42 | 0.30 / 0.32 | 0.76 / 0.47 | 830 / 830 | 250 | 3/3 / 3/3 |
| gemini-reject-explicit-count | 0.37 / 0.63 | 0.28 / 0.34 | 0.69 / 0.46 | 888 / 888 | 254 | 3/3 / 3/3 |
| gemini-noop | 0.36 / 0.39 | 0.32 / 0.32 | 0.67 / 0.43 | 790 / 790 | 237 | 3/3 / 3/3 |
| gemini-edit-create | 0.62 / 0.67 | 1.95 / 2.25 | 0.00 / 0.00 | 418 / 418 | 1,109 | 3/3 / 3/3 |
| gemini-reject-existing-create | 0.38 / 0.39 | 0.29 / 0.29 | 0.68 / 0.48 | 813 / 813 | 260 | 3/3 / 3/3 |
| gemini-write-create | 0.71 / 0.71 | 2.00 / 2.06 | 0.00 / 0.00 | 909 / 909 | 1,115 | 3/3 / 3/3 |
| gemini-write-overwrite | 0.58 / 0.59 | 2.17 / 2.18 | 0.54 / 0.51 | 1,053 / 1,053 | 1,398 | 3/3 / 3/3 |
| gemini-write-noop | 0.57 / 0.59 | 0.55 / 0.63 | 0.57 / 0.46 | 977 / 977 | 1,035 | 3/3 / 3/3 |
| gemini-bom-crlf | 0.60 / 0.76 | 2.54 / 2.68 | 0.57 / 0.51 | 1,174 / 1,174 | 1,510 | 3/3 / 3/3 |
| gemini-mixed-eol | 0.54 / 0.69 | 2.44 / 2.49 | 0.57 / 0.51 | 1,173 / 1,173 | 1,499 | 0/3 / 0/3 |
| gemini-confirmed-edit | 0.68 / 0.69 | 2.59 / 2.67 | 0.47 / 0.54 | 1,111 / 1,111 | 1,551 | 3/3 / 3/3 |
| gemini-policy-denied | 0.12 / 0.13 | 0.13 / 0.15 | 0.00 / 0.00 | 243 / 243 | 0 | 3/3 / 3/3 |

Storage oracle differences: `gemini-original/gemini-mixed-eol` (0/3 exact), `xnip-native/gemini-mixed-eol` (0/3 exact).

Every native workflow matches the original provider’s actual SDK input/result fingerprints and final stored bytes. These local measurements preserve the observed execution costs; process IO and transaction work remain part of the native lane.

## Reproduce

```sh
node benches/native/gemini_native_setup.mjs
CARGO_BUILD_JOBS=4 cargo build --release --locked --offline
node benches/gemini_provider.mjs --executable target/release/xnip --repeats 3 --warmups 1
node --test-reporter=tap tests/gemini_provider_bench_contract.mjs
```

Prepared packages live under target/native-hosts/gemini-0.27.2. The JSON companion records per-call timings, tool and bridge byte counts, exceptions, policy events, expected/actual file hashes, full maintained-source fingerprints and the dependency-tree digest. Existing host and historical representation cohorts retain their own data.
