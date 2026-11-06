# Python native transport

This local, standard-library package owns the JSON-lines subprocess connection
used by xnip's Python SDK adapters. Editing operations remain in the existing
Rust `core`; host matching, tool definitions and approval stay in each SDK.

Install it alongside the selected adapter from this checkout:

```sh
python -m pip install ./integrations/native-python ./integrations/openhands-native
```

`NativeClient(executable, cwd)` exposes `read(path)`, `request(action, **fields)`,
`send(action, **fields)`, `payload(bytes)` and `encoding`. It correlates replies
across worker threads, negotiates UTF-8 with byte-array fallback, and retains
request/response counters for benchmarks. Call `close()` after the host session
settles so EOF follows any queued commit.
