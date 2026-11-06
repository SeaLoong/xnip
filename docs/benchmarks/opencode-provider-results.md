# OpenCode native plugin benchmark

OpenCode 1.19.3; 23 scenarios, 3 measured runs and 1 warm-up per provider/scenario. Zero actual model endpoint requests.

Full CLI invocation through runtime.call, including argument serialization, process startup/shutdown, stdout file capture/read and JSON decoding; each call launches a fresh OpenCode process. Fixture setup, benchmark payload archival/byte accounting and file-goal verification are outside timing.

Independent exact byte goals for both providers, known success receipts and public guard/permission error reasons; custom xnip tools retain exact matching while original edit retains fuzzy/EOL behavior. Formatters/LSP disabled in both lanes.

One configured local inert provider; HTTP trap counts actual model endpoint requests. Official debug source executes tools without a model loop; interactive ASK and session before/after hooks lie outside this entry.

Binary SHA-256 `144f76db0a340d7eede61b7fc2545c8c7c1d7d937300f378fd6c77c8da3e0688`; source `632ce552959782c341f572aed9522e83f160f683c27d7a691ab76a66cc36f31d`. Raw JSON retains source/package fingerprints, actual arguments/results, recomputable digests, public errors and complete file oracles. Large result payloads use lossless gzip+base64 encoding.

The plugin adds explicit xnip tools. OpenCode builtins retain their own result/UI, formatting and fuzzy matching behavior. The multi-file patch fixture uses one xnip_apply call versus separate original edit calls; tool counts remain visible. This is tool-only CLI execution, not agent generation or billed tokens. Three measured runs make nearest-rank p95 the maximum.

| Scenario | Original median / p95 ms | xnip median / p95 ms | Original / xnip calls | Original / xnip goal+outcome |
|---|---:|---:|---:|---:|
| tiny-config | 937.61 / 992.64 | 998.29 / 1007.03 | 1 / 1 | 3/3 / 3/3 |
| large-local-change | 939.06 / 955.43 | 965.65 / 1022.75 | 1 / 1 | 3/3 / 3/3 |
| multiline-function | 956.42 / 957.70 | 977.65 / 1050.47 | 1 / 1 | 3/3 / 3/3 |
| unicode | 950.57 / 998.30 | 967.63 / 1028.03 | 1 / 1 | 3/3 / 3/3 |
| crlf | 978.14 / 979.09 | 973.20 / 997.26 | 1 / 1 | 3/3 / 3/3 |
| no-final-newline | 933.67 / 949.32 | 979.15 / 981.97 | 1 / 1 | 3/3 / 3/3 |
| disambiguated-block | 953.35 / 984.80 | 998.20 / 1011.21 | 1 / 1 | 3/3 / 3/3 |
| global-replace | 993.61 / 1022.86 | 983.43 / 1035.86 | 1 / 1 | 3/3 / 3/3 |
| same-file-batch | 1958.92 / 1985.58 | 1953.08 / 2358.49 | 2 / 2 | 3/3 / 3/3 |
| multi-file-batch | 2860.62 / 2869.17 | 3031.10 / 3072.69 | 3 / 3 | 3/3 / 3/3 |
| long-line | 980.12 / 1005.96 | 1011.17 / 1053.28 | 1 / 1 | 3/3 / 3/3 |
| insert-after-anchor | 988.91 / 1007.95 | 1007.74 / 1045.23 | 1 / 1 | 3/3 / 3/3 |
| delete-block | 945.01 / 1101.51 | 988.53 / 1003.12 | 1 / 1 | 3/3 / 3/3 |
| large-65536 | 961.93 / 997.74 | 999.08 / 1025.21 | 1 / 1 | 3/3 / 3/3 |
| large-1048576 | 997.21 / 1000.58 | 1021.95 / 1088.47 | 1 / 1 | 3/3 / 3/3 |
| large-5242880 | 1018.67 / 1055.72 | 1174.48 / 1193.50 | 1 / 1 | 3/3 / 3/3 |
| bom-crlf | 915.59 / 969.60 | 995.59 / 1034.25 | 1 / 1 | 3/3 / 3/3 |
| missing-guard | 964.62 / 1012.40 | 959.57 / 968.89 | 1 / 1 | 3/3 / 3/3 |
| ambiguous-guard | 960.61 / 964.45 | 972.26 / 988.37 | 1 / 1 | 3/3 / 3/3 |
| identical-edit | 980.69 / 984.10 | 1003.28 / 1017.83 | 1 / 1 | 3/3 / 3/3 |
| policy-denied | 915.56 / 962.57 | 989.68 / 1037.70 | 1 / 1 | 3/3 / 3/3 |
| write-create | 971.32 / 975.68 | 1019.47 / 1046.13 | 1 / 1 | 3/3 / 3/3 |
| patch-multifile | 1957.65 / 1993.72 | 989.81 / 1002.55 | 2 / 1 | 3/3 / 3/3 |

## Reproduce

```sh
node benches/native/opencode_native_setup.mjs
node benches/opencode_provider.mjs --executable target/release/xnip --repeats 3 --warmups 1
```

[Integration and verification scope](../opencode.md).
