# OpenHands SDK storage provider benchmark

OpenHands SDK/tools 1.60.1; 23 fixtures, 3 measured runs and 1 warm-up per provider. Zero model calls.

Both providers execute the original file_editor schema, action validation, matching, snippets and history through Conversation.execute_tool. The public tool-only API skips interactive confirmation; the separate Agent.step E2E covers confirmation, hooks, resource scheduling and interruption.

Warm execution includes action construction and SDK dispatch. Setup, fixture resets, byte hashing, result serialization accounting and waiting for asynchronous cleanup receipts are separate. Native IPC counters include every actual request/response. Three samples make nearest-rank p95 the maximum.

The independent byte goal preserves untouched storage. Both providers retain OpenHands universal-newline behavior; normalization differences remain visible. Compatibility separately requires identical actual SDK observations and stored bytes.

Binary SHA-256 `144f76db0a340d7eede61b7fc2545c8c7c1d7d937300f378fd6c77c8da3e0688` (release); maintained source `19a5c50e89ab59fe75871ec5fd44216c8493e31e4c57a37fea841964b874136a`.

| Fixture | Original mutation median / p95 ms | Native mutation median / p95 ms | Original / native byte-goal runs | Native IPC bytes |
|---|---:|---:|---:|---:|
| tiny-config | 1.03 / 1.03 | 3.37 / 3.42 | 3/3 / 3/3 | 2,383 |
| large-local-change | 1.90 / 1.92 | 7.06 / 7.17 | 3/3 / 3/3 | 491,098 |
| multiline-function | 1.06 / 1.09 | 3.44 / 3.49 | 3/3 / 3/3 | 3,282 |
| unicode | 1.09 / 1.11 | 3.30 / 3.44 | 3/3 / 3/3 | 2,517 |
| crlf | 1.06 / 1.08 | 3.28 / 3.37 | 0/3 / 0/3 | 2,401 |
| no-final-newline | 1.15 / 1.16 | 3.25 / 3.30 | 3/3 / 3/3 | 2,332 |
| disambiguated-block | 1.10 / 1.10 | 3.36 / 3.76 | 3/3 / 3/3 | 2,665 |
| global-replace | 1.20 / 1.25 | 3.73 / 3.75 | 3/3 / 3/3 | 4,887 |
| same-file-batch | 2.39 / 3.61 | 6.61 / 6.81 | 3/3 / 3/3 | 4,229 |
| multi-file-batch | 3.22 / 3.23 | 8.95 / 9.73 | 3/3 / 3/3 | 6,208 |
| long-line | 1.77 / 1.81 | 5.21 / 5.73 | 3/3 / 3/3 | 482,113 |
| insert-after-anchor | 1.05 / 1.09 | 3.19 / 3.30 | 3/3 / 3/3 | 2,394 |
| delete-block | 1.09 / 1.14 | 3.13 / 3.18 | 3/3 / 3/3 | 2,287 |
| openhands-missing | 0.32 / 0.33 | 0.53 / 0.60 | 3/3 / 3/3 | 726 |
| openhands-ambiguous | 0.30 / 0.33 | 0.55 / 0.57 | 3/3 / 3/3 | 753 |
| openhands-bom-crlf | 1.06 / 1.12 | 2.97 / 3.27 | 0/3 / 0/3 | 2,139 |
| openhands-mixed-eol | 1.06 / 1.20 | 3.00 / 3.16 | 0/3 / 0/3 | 2,197 |
| openhands-create | 0.56 / 0.66 | 1.95 / 1.98 | 3/3 / 3/3 | 1,091 |
| openhands-insert | 1.15 / 1.16 | 3.20 / 3.28 | 3/3 / 3/3 | 2,135 |
| openhands-undo | 1.81 / 1.83 | 5.87 / 5.95 | 3/3 / 3/3 | 3,472 |
| openhands-large-65536 | 1.99 / 2.07 | 6.38 / 6.47 | 3/3 / 3/3 | 722,934 |
| openhands-large-1048576 | 8.86 / 9.08 | 63.04 / 63.87 | 3/3 / 3/3 | 11,536,402 |
| openhands-large-5242880 | 47.22 / 48.36 | 360.59 / 369.91 | 3/3 / 3/3 | 57,673,746 |

## Reproduce

```sh
python benches/native/openhands_setup.py
target/native-hosts/openhands-1.60.1/.venv/bin/python benches/openhands_provider.py --executable target/release/xnip
```
