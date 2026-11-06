# OpenCode native plugin benchmark

OpenCode 1.19.3; 23 scenarios, 3 measured runs and 1 warm-up per provider/scenario. Zero actual model endpoint requests.

Full CLI invocation through runtime.call, including argument serialization, process startup/shutdown, stdout file capture/read and JSON decoding; each call launches a fresh OpenCode process. Fixture setup, benchmark payload archival/byte accounting and file-goal verification are outside timing.

Independent exact byte goals for both providers, known success receipts and public guard/permission error reasons; custom xnip tools retain exact matching while original edit retains fuzzy/EOL behavior. Formatters/LSP disabled in both lanes.

One configured local inert provider; HTTP trap counts actual model endpoint requests. Official debug source executes tools without a model loop; interactive ASK and session before/after hooks lie outside this entry.

Binary SHA-256 `16cecdbad176790492338643e29480d2dc7793c5cda5629102e0dde434be8deb`; source `df3295f9e61ce3eb454f33e1eddfdc8cb39e7ec0efe30d6d0f3f9a3f2d948985`. Raw JSON retains source/package fingerprints, actual arguments/results, recomputable digests, public errors and complete file oracles. Large result payloads use lossless gzip+base64 encoding.

The plugin adds explicit xnip tools. OpenCode builtins retain their own result/UI, formatting and fuzzy matching behavior. The multi-file patch fixture uses one xnip_apply call versus separate original edit calls; tool counts remain visible. This is tool-only CLI execution, not agent generation or billed tokens. Three measured runs make nearest-rank p95 the maximum.

| Scenario | Original median / p95 ms | xnip median / p95 ms | Original / xnip calls | Original / xnip goal+outcome |
|---|---:|---:|---:|---:|
| tiny-config | 997.69 / 1036.26 | 1014.46 / 1017.65 | 1 / 1 | 3/3 / 3/3 |
| large-local-change | 1047.06 / 1074.46 | 981.60 / 986.52 | 1 / 1 | 3/3 / 3/3 |
| multiline-function | 939.67 / 1142.01 | 999.91 / 1023.50 | 1 / 1 | 3/3 / 3/3 |
| unicode | 915.09 / 1033.63 | 1021.63 / 1027.57 | 1 / 1 | 3/3 / 3/3 |
| crlf | 940.37 / 1000.49 | 1014.72 / 1039.44 | 1 / 1 | 3/3 / 3/3 |
| no-final-newline | 916.52 / 957.86 | 979.66 / 1014.01 | 1 / 1 | 3/3 / 3/3 |
| disambiguated-block | 958.98 / 1002.54 | 1002.08 / 1002.91 | 1 / 1 | 3/3 / 3/3 |
| global-replace | 940.61 / 941.50 | 1033.81 / 1045.63 | 1 / 1 | 3/3 / 3/3 |
| same-file-batch | 1922.35 / 2004.86 | 1993.46 / 2063.38 | 2 / 2 | 3/3 / 3/3 |
| multi-file-batch | 2833.73 / 2891.26 | 3091.51 / 3158.61 | 3 / 3 | 3/3 / 3/3 |
| long-line | 949.28 / 996.91 | 1054.58 / 1059.36 | 1 / 1 | 3/3 / 3/3 |
| insert-after-anchor | 940.96 / 1037.00 | 1016.45 / 1046.11 | 1 / 1 | 3/3 / 3/3 |
| delete-block | 996.79 / 1109.12 | 1011.84 / 1027.66 | 1 / 1 | 3/3 / 3/3 |
| large-65536 | 975.18 / 1015.38 | 987.47 / 1020.83 | 1 / 1 | 3/3 / 3/3 |
| large-1048576 | 924.96 / 929.75 | 1070.75 / 1151.43 | 1 / 1 | 3/3 / 3/3 |
| large-5242880 | 1037.26 / 1060.33 | 1147.60 / 1159.83 | 1 / 1 | 3/3 / 3/3 |
| bom-crlf | 979.12 / 990.44 | 996.99 / 1002.67 | 1 / 1 | 3/3 / 3/3 |
| missing-guard | 938.47 / 967.74 | 973.32 / 995.30 | 1 / 1 | 3/3 / 3/3 |
| ambiguous-guard | 989.16 / 993.51 | 1010.31 / 1019.77 | 1 / 1 | 3/3 / 3/3 |
| identical-edit | 949.79 / 965.77 | 1055.69 / 1094.13 | 1 / 1 | 3/3 / 3/3 |
| policy-denied | 982.53 / 1035.76 | 1004.87 / 1056.09 | 1 / 1 | 3/3 / 3/3 |
| write-create | 941.79 / 942.78 | 1008.00 / 1030.78 | 1 / 1 | 3/3 / 3/3 |
| patch-multifile | 1966.82 / 2023.86 | 1018.10 / 1032.69 | 2 / 1 | 3/3 / 3/3 |

## Reproduce

```sh
node benches/native/opencode_native_setup.mjs
node benches/opencode_provider.mjs --executable target/release/xnip --repeats 3 --warmups 1
```

[Integration and verification scope](../../../opencode.md).
