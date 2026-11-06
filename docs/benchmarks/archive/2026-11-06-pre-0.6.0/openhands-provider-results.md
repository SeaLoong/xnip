# OpenHands SDK storage provider benchmark

OpenHands SDK/tools 1.60.1; 23 fixtures, 3 measured runs and 1 warm-up per provider. Zero model calls.

Both providers execute the original file_editor schema, action validation, matching, snippets and history through Conversation.execute_tool. The public tool-only API skips interactive confirmation; the separate Agent.step E2E covers confirmation, hooks, resource scheduling and interruption.

Warm execution includes action construction and SDK dispatch. Setup, fixture resets, byte hashing, result serialization accounting and waiting for asynchronous cleanup receipts are separate. Native IPC counters include every actual request/response. Three samples make nearest-rank p95 the maximum.

The independent byte goal preserves untouched storage. Both providers retain OpenHands universal-newline behavior; normalization differences remain visible. Compatibility separately requires identical actual SDK observations and stored bytes.

Binary SHA-256 `16cecdbad176790492338643e29480d2dc7793c5cda5629102e0dde434be8deb` (release); maintained source `0f14c44f74ce8f7b96cfe255c4cf95771265d8d9def569f5cd1e2ff390e8a417`.

| Fixture | Original mutation median / p95 ms | Native mutation median / p95 ms | Original / native byte-goal runs | Native IPC bytes |
|---|---:|---:|---:|---:|
| tiny-config | 0.99 / 1.02 | 4.84 / 8.06 | 3/3 / 3/3 | 2,383 |
| large-local-change | 2.03 / 2.05 | 8.66 / 9.23 | 3/3 / 3/3 | 491,098 |
| multiline-function | 1.35 / 1.37 | 4.25 / 4.60 | 3/3 / 3/3 | 3,282 |
| unicode | 1.04 / 1.16 | 4.89 / 5.16 | 3/3 / 3/3 | 2,517 |
| crlf | 1.02 / 1.02 | 4.58 / 4.68 | 0/3 / 0/3 | 2,401 |
| no-final-newline | 1.10 / 1.13 | 4.51 / 4.52 | 3/3 / 3/3 | 2,332 |
| disambiguated-block | 1.04 / 1.14 | 4.83 / 5.34 | 3/3 / 3/3 | 2,665 |
| global-replace | 1.23 / 1.29 | 4.59 / 4.61 | 3/3 / 3/3 | 4,887 |
| same-file-batch | 2.13 / 3.56 | 8.94 / 9.07 | 3/3 / 3/3 | 4,229 |
| multi-file-batch | 3.18 / 3.26 | 11.99 / 19.81 | 3/3 / 3/3 | 6,208 |
| long-line | 1.98 / 2.06 | 7.04 / 7.24 | 3/3 / 3/3 | 482,113 |
| insert-after-anchor | 1.16 / 1.22 | 4.13 / 4.13 | 3/3 / 3/3 | 2,394 |
| delete-block | 1.16 / 1.30 | 4.00 / 4.41 | 3/3 / 3/3 | 2,287 |
| openhands-missing | 0.27 / 0.28 | 0.57 / 0.66 | 3/3 / 3/3 | 726 |
| openhands-ambiguous | 0.28 / 0.31 | 0.59 / 0.62 | 3/3 / 3/3 | 753 |
| openhands-bom-crlf | 0.99 / 0.99 | 3.91 / 3.97 | 0/3 / 0/3 | 2,139 |
| openhands-mixed-eol | 1.10 / 1.38 | 3.93 / 4.18 | 0/3 / 0/3 | 2,197 |
| openhands-create | 0.55 / 0.56 | 2.51 / 2.82 | 3/3 / 3/3 | 1,091 |
| openhands-insert | 1.19 / 1.27 | 4.06 / 5.50 | 3/3 / 3/3 | 2,135 |
| openhands-undo | 1.82 / 1.97 | 7.27 / 8.79 | 3/3 / 3/3 | 3,472 |
| openhands-large-65536 | 2.40 / 2.40 | 8.43 / 8.44 | 3/3 / 3/3 | 722,934 |
| openhands-large-1048576 | 11.88 / 11.98 | 68.37 / 68.66 | 3/3 / 3/3 | 11,536,402 |
| openhands-large-5242880 | 49.89 / 50.93 | 370.69 / 395.05 | 3/3 / 3/3 | 57,673,746 |

## Reproduce

```sh
python benches/native/openhands_setup.py
target/native-hosts/openhands-1.60.1/.venv/bin/python benches/openhands_provider.py --executable target/release/xnip
```
