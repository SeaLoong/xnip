# Native bridge encoding control

Both cohorts execute the same official DSH tools, policies, fixtures, xnip executable and source. The control completes the ordinary capability handshake, then selects the supported legacy byte-array path. The normal cohort negotiates UTF-8. One persistent process serves each provider/version; every fixture has its own warm-up. The legacy cohort runs first.

This isolates the bridge representation choice within the measured local workflow. Returned file snapshots and exact guards retain their byte oracle. Tool and filesystem work remain part of the timed operation. Median and nearest-rank p95 use measured samples; three runs make p95 the maximum. Scheduling, GC and a small sample count affect the magnitude of latency differences.

Binary SHA-256: `5504d4f5ce31d9b04852bc61bba1d9bc7341e0bbd5fcf0fc6c24ab12d3e87ab1` (release). Source SHA-256: `945eb2cf93b09832b8a09e0520eefc69034fa9f8784d4a438bbeae7c3684e935`. 3 measured runs, 1 warm-up per fixture. Raw JSON retains both cohorts and their original-provider reference lanes.

| DSH | Fixture | Legacy mutation median / p95 ms | UTF-8 mutation median / p95 ms | Legacy / UTF-8 bridge bytes | Legacy / UTF-8 correct |
|---|---|---:|---:|---:|---:|
| 0.2.1-rc.2 | large-1048576 | 537.41 / 554.16 | 34.44 / 34.99 | 20,932,466 / 5,285,773 | 3/3 / 3/3 |
| 0.2.1-rc.2 | large-5242880 | 2839.94 / 2930.04 | 157.67 / 167.22 | 104,654,712 / 26,421,139 | 3/3 / 3/3 |
| 0.2.1-rc.5 | large-1048576 | 510.22 / 520.99 | 32.77 / 32.91 | 20,932,466 / 5,285,773 | 3/3 / 3/3 |
| 0.2.1-rc.5 | large-5242880 | 2692.92 / 3310.36 | 154.02 / 156.93 | 104,654,712 / 26,421,139 | 3/3 / 3/3 |

## Reproduce

```sh
node benches/native_provider.mjs --compare-encodings --executable target/release/xnip --repeats 3 --warmups 1
```

See the [full provider matrix](native-provider-results.md) for the 32-fixture cohort, methodology and setup. The JSON companion contains read timings, local tool bytes, native request/response bytes, error outcomes and per-file expected/actual hashes. Zero model calls.

The [earlier debug observations](native-provider-debug-history.json) retain optimization-before/after samples with their distinct executable hashes. The release control above holds the source and executable fixed; those early observations have a narrower provenance record.
