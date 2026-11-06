# Cline apply_patch provider benchmark

Cline SDK 0.2.0; 31 fixtures; 3 measured runs and 1 warm-up per fixture/provider. Measured 2026-11-06T02:44:31.341Z. Zero actual model stream calls.

Both lanes use the published SDK registry, read_files and apply_patch tools, schema validation, tool policy and before/after hooks. Production uses the public createBuiltinTools executor hook under the normal Agent lifecycle. This fixture calls Agent.executeToolCalls, a TypeScript-private method exposed in published JavaScript, to enter deterministic tool dispatch with zero model stream calls. The original lane uses the packaged patch executor; xnip replaces that executor and retains official computePatchChanges matching and output wrapping. Catalogs and actual operation outputs are checked equal.

The native backend covers apply_patch; read_files retains its original reader. This SDK cohort measures local tool execution. Interactive IDE flows, model generation and billing have their own boundaries.

The original 13 fixtures retain independent expected bytes. Native patches use three-line context from their known before/after text; patch generation runs before timing. Multiple files and chunks share one patch call and one grouped preparatory read. Added cases exercise official whitespace matching, first-match context, no-op success, batches, BOM/CRLF, rejection outcomes and path aliases. Alias marker spellings remain intact when cases are placed in their fixture directories; the independent oracle records final content or final absence for the official update/add/delete/move order.

Large-file changes sit at EOF. Three explicitly named EOF-hint cases preserve the original repeated-line bytes and goal while supplying the official End of File marker and trailing empty context. A separate unhinted repeated 64 KiB case retains context-scan pressure. The [official parser component probe](cline-matcher-probe.json) compares the same 64 KiB input/goal: 459-byte unhinted patch versus 477-byte hinted patch. The [earlier partial diagnostic](cline-provider-partial-diagnostic.json) preserves unhinted 1 MiB progress around 13.9 seconds; sampling stopped during 5 MiB before a complete report. These diagnostics have separate component/partial boundaries from this complete SDK matrix.

Fixture reset, byte verification and serialization counting are outside timing. One persistent Agent serves each lane. Setup covers factory, SDK verification and Agent initialization; the first original lane includes cold SDK import. The native process initializes lazily when a warm-up patch first reaches persistence. Raw warm-up results retain that initialization cost separately. Official SDK wrapper timeout timers keep Node alive after tool results return; this idle process lifetime is outside warm tool execution timing. Median and nearest-rank p95 use measured samples; with three runs p95 is the maximum.

Binary release, SHA-256 `33bf6effc23db6f10fe8d5d1881e98003db95cb59a8af144a2a523fbc97f3884`; source `70fc80656e8a63da30d338a72cceb7fbac440de28a62f625070a454a5615351c`. Node v24.22.1; linux/x64; AMD EPYC 9V74 80-Core Processor. Helper import 2.40 ms. Package entry and lock fingerprints are in the JSON companion.

| Provider | Factory / Agent setup ms | First warm-up patch ms |
|---|---:|---:|
| cline-original | 777.99 | 3.01 |
| xnip-native | 8.79 | 11.43 |

| Fixture | Original patch median / p95 ms | xnip patch median / p95 ms | Original / xnip read median ms | Original / xnip SDK tool bytes | Original / xnip correct |
|---|---:|---:|---:|---:|---:|
| tiny-config | 0.61 / 0.64 | 3.55 / 3.59 | 0.61 / 0.35 | 741 / 741 | 3/3 / 3/3 |
| large-local-change | 282.94 / 290.18 | 260.77 / 266.33 | 1.61 / 0.93 | 50,526 / 50,526 | 3/3 / 3/3 |
| multiline-function | 0.51 / 0.52 | 3.22 / 3.52 | 0.44 / 0.24 | 1,000 / 1,000 | 3/3 / 3/3 |
| unicode | 0.37 / 0.43 | 3.07 / 3.25 | 0.30 / 0.29 | 772 / 772 | 3/3 / 3/3 |
| crlf | 0.36 / 0.38 | 3.04 / 3.12 | 0.28 / 0.30 | 715 / 715 | 3/3 / 3/3 |
| no-final-newline | 0.40 / 0.47 | 3.31 / 3.31 | 0.34 / 0.30 | 730 / 730 | 3/3 / 3/3 |
| disambiguated-block | 0.39 / 0.53 | 3.45 / 3.51 | 0.28 / 0.27 | 809 / 809 | 3/3 / 3/3 |
| global-replace | 0.43 / 0.43 | 3.23 / 3.28 | 0.34 / 0.29 | 1,506 / 1,506 | 3/3 / 3/3 |
| same-file-batch | 0.39 / 0.42 | 3.27 / 3.33 | 0.28 / 0.27 | 731 / 731 | 3/3 / 3/3 |
| multi-file-batch | 0.87 / 0.99 | 6.43 / 7.20 | 0.63 / 0.41 | 1,387 / 1,387 | 3/3 / 3/3 |
| long-line | 4.63 / 4.71 | 10.33 / 11.32 | 0.33 / 0.33 | 82,683 / 82,683 | 3/3 / 3/3 |
| insert-after-anchor | 0.42 / 0.43 | 3.65 / 3.93 | 0.32 / 0.29 | 752 / 752 | 3/3 / 3/3 |
| delete-block | 0.39 / 0.39 | 3.63 / 3.67 | 0.27 / 0.27 | 744 / 744 | 3/3 / 3/3 |
| large-65536 | 868.52 / 877.37 | 789.50 / 792.44 | 0.75 / 0.60 | 49,029 / 49,029 | 3/3 / 3/3 |
| cline-eof-hint-large-65536 | 0.79 / 1.48 | 5.94 / 6.35 | 0.60 / 0.40 | 49,109 / 49,109 | 3/3 / 3/3 |
| cline-eof-hint-large-1048576 | 4.51 / 5.29 | 47.30 / 88.10 | 2.61 / 1.98 | 49,118 / 49,118 | 3/3 / 3/3 |
| cline-eof-hint-large-5242880 | 36.21 / 51.71 | 270.02 / 310.27 | 10.09 / 18.79 | 49,119 / 49,119 | 3/3 / 3/3 |
| cline-fuzzy-whitespace | 0.45 / 0.48 | 4.25 / 4.33 | 0.32 / 0.30 | 750 / 750 | 3/3 / 3/3 |
| cline-first-match | 0.40 / 0.45 | 3.31 / 3.75 | 0.37 / 0.27 | 677 / 677 | 3/3 / 3/3 |
| cline-noop | 0.41 / 0.43 | 0.59 / 0.66 | 0.31 / 0.28 | 640 / 640 | 3/3 / 3/3 |
| cline-batch-adjacent | 0.40 / 0.42 | 3.56 / 4.03 | 0.29 / 0.29 | 731 / 731 | 3/3 / 3/3 |
| cline-batch-disjoint | 0.39 / 0.59 | 3.14 / 3.19 | 0.29 / 0.24 | 765 / 765 | 3/3 / 3/3 |
| cline-bom-crlf | 0.35 / 0.40 | 3.11 / 3.22 | 0.27 / 0.23 | 676 / 676 | 3/3 / 3/3 |
| cline-reject-context | 0.29 / 0.31 | 0.33 / 0.88 | 0.31 / 0.22 | 839 / 839 | 3/3 / 3/3 |
| cline-reject-existing-add | 0.26 / 0.31 | 0.19 / 0.21 | 0.27 / 0.27 | 701 / 701 | 3/3 / 3/3 |
| cline-reject-sentinels | 0.08 / 0.08 | 0.06 / 0.11 | 0.29 / 0.26 | 688 / 688 | 3/3 / 3/3 |
| cline-alias-update-update | 0.71 / 0.71 | 3.18 / 3.22 | 0.28 / 0.25 | 833 / 833 | 3/3 / 3/3 |
| cline-alias-add-add | 0.69 / 0.74 | 2.43 / 2.46 | 0.00 / 0.00 | 438 / 438 | 3/3 / 3/3 |
| cline-alias-delete-delete | 0.82 / 1.95 | 2.31 / 2.41 | 0.34 / 0.31 | 818 / 818 | 3/3 / 3/3 |
| cline-alias-move-update | 1.00 / 1.01 | 4.42 / 5.54 | 0.49 / 0.32 | 1,204 / 1,204 | 3/3 / 3/3 |
| cline-alias-same-path-move | 0.61 / 0.70 | 1.97 / 2.13 | 0.33 / 0.30 | 818 / 818 | 3/3 / 3/3 |

SDK tool bytes sum public arguments and the actual returned tool-result part, including its call ID. Raw output_bytes separately counts the operation output. An operation success:false is an error outcome even when dispatch isError is absent. The byte oracle verifies every expected file, its independent expected hash and the complete fixture file set. Per-call hashes verify original/native arguments and outputs match.

The JSON companion retains warm-up and measured samples, per-call timings/bytes/errors, actual Agent started/finished event counts, model stream counters and source/package fingerprints. The historical 13/41 matrix remains its original cohort. Scheduling, GC, page cache and a small run count affect observed latency.

## Reproduce

```sh
node benches/native/cline_native_setup.mjs
node benches/cline_provider.mjs --executable target/release/xnip --repeats 3 --warmups 1
node --test-reporter=tap tests/cline_provider_bench_contract.mjs
```

The fixed SDK packages are prepared under target/native-hosts/cline-0.2.0. See the independently measured [DSH](native-provider-results.md) and [Pi](pi-provider-results.md) cohorts for their respective tool and governance boundaries.
