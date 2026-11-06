# OpenHands SDK storage provider benchmark

OpenHands SDK/tools 1.60.1; 23 fixtures, 3 measured runs and 1 warm-up per provider. Zero model calls.

Both providers execute the original file_editor schema, action validation, matching, snippets and history through Conversation.execute_tool. The public tool-only API skips interactive confirmation; the separate Agent.step E2E covers confirmation, hooks, resource scheduling and interruption.

Warm execution includes action construction and SDK dispatch. Setup, fixture resets, byte hashing, result serialization accounting and waiting for asynchronous cleanup receipts are separate. Native IPC counters include every actual request/response. Three samples make nearest-rank p95 the maximum.

The independent byte goal preserves untouched storage. Both providers retain OpenHands universal-newline behavior; normalization differences remain visible. Compatibility separately requires identical actual SDK observations and stored bytes.

Binary SHA-256 `b01cc02a39864cb7f460cdd29fc84c8059855229acc6aec97c43d1070505d012` (release); maintained source `cf0f143da7b49904b19d8f168b6970e2a55b50a0ae72cbeba773c94349b02095`.

| Fixture | Original mutation median / p95 ms | Native mutation median / p95 ms | Original / native byte-goal runs | Native IPC bytes |
|---|---:|---:|---:|---:|
| tiny-config | 0.96 / 0.99 | 3.90 / 3.97 | 3/3 / 3/3 | 2,383 |
| large-local-change | 1.83 / 1.84 | 7.47 / 7.63 | 3/3 / 3/3 | 491,098 |
| multiline-function | 1.02 / 1.06 | 3.70 / 3.73 | 3/3 / 3/3 | 3,282 |
| unicode | 1.04 / 1.05 | 4.40 / 4.44 | 3/3 / 3/3 | 2,517 |
| crlf | 1.01 / 1.06 | 3.91 / 3.96 | 0/3 / 0/3 | 2,401 |
| no-final-newline | 1.02 / 1.07 | 3.81 / 3.89 | 3/3 / 3/3 | 2,332 |
| disambiguated-block | 1.08 / 1.32 | 3.93 / 4.13 | 3/3 / 3/3 | 2,665 |
| global-replace | 1.20 / 1.23 | 4.31 / 4.48 | 3/3 / 3/3 | 4,887 |
| same-file-batch | 2.11 / 3.29 | 7.94 / 8.05 | 3/3 / 3/3 | 4,229 |
| multi-file-batch | 3.04 / 3.08 | 11.80 / 12.15 | 3/3 / 3/3 | 6,208 |
| long-line | 1.73 / 1.77 | 6.16 / 6.31 | 3/3 / 3/3 | 482,113 |
| insert-after-anchor | 1.07 / 1.08 | 3.72 / 3.92 | 3/3 / 3/3 | 2,394 |
| delete-block | 1.08 / 1.10 | 3.83 / 3.94 | 3/3 / 3/3 | 2,287 |
| openhands-missing | 0.28 / 0.28 | 0.57 / 0.59 | 3/3 / 3/3 | 726 |
| openhands-ambiguous | 0.33 / 0.38 | 0.58 / 0.61 | 3/3 / 3/3 | 753 |
| openhands-bom-crlf | 1.10 / 1.16 | 3.96 / 4.21 | 0/3 / 0/3 | 2,139 |
| openhands-mixed-eol | 1.18 / 1.22 | 3.76 / 3.87 | 0/3 / 0/3 | 2,197 |
| openhands-create | 0.66 / 0.68 | 2.84 / 2.98 | 3/3 / 3/3 | 1,091 |
| openhands-insert | 1.36 / 1.36 | 4.09 / 4.17 | 3/3 / 3/3 | 2,135 |
| openhands-undo | 2.00 / 2.02 | 6.81 / 7.74 | 3/3 / 3/3 | 3,472 |
| openhands-large-65536 | 2.12 / 2.12 | 7.80 / 7.94 | 3/3 / 3/3 | 722,934 |
| openhands-large-1048576 | 8.93 / 8.99 | 65.01 / 70.57 | 3/3 / 3/3 | 11,536,402 |
| openhands-large-5242880 | 44.26 / 48.19 | 309.30 / 315.10 | 3/3 / 3/3 | 57,673,746 |

## Reproduce

```sh
python benches/native/openhands_setup.py
target/native-hosts/openhands-1.60.1/.venv/bin/python benches/openhands_provider.py --executable target/release/xnip
```
