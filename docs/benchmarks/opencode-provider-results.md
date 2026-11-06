# OpenCode native plugin benchmark

OpenCode 1.19.3; 23 scenarios, 3 measured runs and 1 warm-up per provider/scenario. Zero actual model endpoint requests.

Full CLI invocation through runtime.call, including argument serialization, process startup/shutdown, stdout file capture/read and JSON decoding; each call launches a fresh OpenCode process. Fixture setup, benchmark payload archival/byte accounting and file-goal verification are outside timing.

Independent exact byte goals for both providers, known success receipts and public guard/permission error reasons; custom xnip tools retain exact matching while original edit retains fuzzy/EOL behavior. Formatters/LSP disabled in both lanes.

One configured local inert provider; HTTP trap counts actual model endpoint requests. Official debug source executes tools without a model loop; interactive ASK and session before/after hooks lie outside this entry.

Binary SHA-256 `b01cc02a39864cb7f460cdd29fc84c8059855229acc6aec97c43d1070505d012`; source `57b24836902e70d53cf0fd0b0cf673662f649b133c53fa12fce3a2f0543d9dbe`. Raw JSON retains source/package fingerprints, actual arguments/results, recomputable digests, public errors and complete file oracles. Large result payloads use lossless gzip+base64 encoding.

The plugin adds explicit xnip tools. OpenCode builtins retain their own result/UI, formatting and fuzzy matching behavior. The multi-file patch fixture uses one xnip_apply call versus separate original edit calls; tool counts remain visible. This is tool-only CLI execution, not agent generation or billed tokens. Three measured runs make nearest-rank p95 the maximum.

| Scenario | Original median / p95 ms | xnip median / p95 ms | Original / xnip calls | Original / xnip goal+outcome |
|---|---:|---:|---:|---:|
| tiny-config | 953.84 / 973.58 | 1044.25 / 1060.08 | 1 / 1 | 3/3 / 3/3 |
| large-local-change | 900.17 / 961.65 | 983.05 / 991.15 | 1 / 1 | 3/3 / 3/3 |
| multiline-function | 945.26 / 966.15 | 993.32 / 1005.22 | 1 / 1 | 3/3 / 3/3 |
| unicode | 898.44 / 935.28 | 972.39 / 984.12 | 1 / 1 | 3/3 / 3/3 |
| crlf | 898.57 / 942.64 | 1008.76 / 1009.66 | 1 / 1 | 3/3 / 3/3 |
| no-final-newline | 1025.42 / 1038.56 | 978.37 / 983.54 | 1 / 1 | 3/3 / 3/3 |
| disambiguated-block | 921.58 / 926.41 | 966.33 / 990.71 | 1 / 1 | 3/3 / 3/3 |
| global-replace | 963.35 / 965.03 | 980.64 / 1005.31 | 1 / 1 | 3/3 / 3/3 |
| same-file-batch | 1877.25 / 1891.49 | 2016.23 / 2034.63 | 2 / 2 | 3/3 / 3/3 |
| multi-file-batch | 2815.59 / 2916.48 | 2955.24 / 3038.18 | 3 / 3 | 3/3 / 3/3 |
| long-line | 946.60 / 976.60 | 942.47 / 1035.79 | 1 / 1 | 3/3 / 3/3 |
| insert-after-anchor | 962.76 / 964.25 | 970.77 / 989.27 | 1 / 1 | 3/3 / 3/3 |
| delete-block | 962.95 / 1124.64 | 1000.47 / 1034.93 | 1 / 1 | 3/3 / 3/3 |
| large-65536 | 975.65 / 988.83 | 1019.82 / 1032.65 | 1 / 1 | 3/3 / 3/3 |
| large-1048576 | 993.70 / 1016.14 | 974.48 / 1059.01 | 1 / 1 | 3/3 / 3/3 |
| large-5242880 | 1027.08 / 1069.88 | 1090.05 / 1137.54 | 1 / 1 | 3/3 / 3/3 |
| bom-crlf | 941.37 / 962.14 | 979.65 / 990.38 | 1 / 1 | 3/3 / 3/3 |
| missing-guard | 943.15 / 1013.30 | 1000.05 / 1056.19 | 1 / 1 | 3/3 / 3/3 |
| ambiguous-guard | 971.58 / 974.01 | 1001.90 / 1007.43 | 1 / 1 | 3/3 / 3/3 |
| identical-edit | 966.63 / 1037.72 | 965.14 / 978.38 | 1 / 1 | 3/3 / 3/3 |
| policy-denied | 929.91 / 933.19 | 980.46 / 984.87 | 1 / 1 | 3/3 / 3/3 |
| write-create | 939.62 / 990.00 | 988.39 / 998.14 | 1 / 1 | 3/3 / 3/3 |
| patch-multifile | 1886.04 / 1951.32 | 963.61 / 1012.30 | 2 / 1 | 3/3 / 3/3 |

## Reproduce

```sh
node benches/native/opencode_native_setup.mjs
node benches/opencode_provider.mjs --executable target/release/xnip --repeats 3 --warmups 1
```

[Integration and verification scope](../opencode.md).
