# Aider scripting with native xnip persistence

The adapter uses Aider's `Coder.create(io=...)` scripting seam. Aider retains its
format registry, prompt templates, parsing, matching, per-file confirmation,
git policy and error presentation. Its `InputOutput.write_text()` content is
persisted through the existing xnip Rust `core` using one `native serve` child.
Reads retain Aider's decoding and universal-newline policy; raw read snapshots
bind matching to guarded commits.

Support is verified against official commit
[`5dc9490bb35f9729ef2c95d00a19ccd30c26339c`](https://github.com/Aider-AI/aider/tree/5dc9490bb35f9729ef2c95d00a19ccd30c26339c),
source version **0.86.3.dev+less**. The shallow source distribution identifies
itself as `0.1.dev1+g5dc9490bb`. Aider's [scripting documentation](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/website/docs/scripting.md)
describes the Python API as version-sensitive. This SDK integration fixes that
revision and its complete Python dependency closure.

## Install in an application

Use Python 3.13 and a built xnip executable. Install the two local Python
packages together:

```sh
cargo build --release --locked
python3.13 -m pip install ./integrations/native-python ./integrations/aider-native
```

Use the application's ordinary Aider model and IO configuration:

```python
from aider.io import InputOutput
from aider.models import Model
from xnip_aider import create_xnip_coder

io = InputOutput()
session = create_xnip_coder(
    executable="/absolute/path/to/xnip",
    main_model=Model("gpt-4o"),
    edit_format="diff",
    io=io,
    fnames=["existing.py"],
)
try:
    session.run("Update the supplied file")
    # session.run_stream(message) retains the original streaming generator.
finally:
    session.close()
```

On Windows select an absolute `xnip.exe` path. The session keeps one child and
sets `XNIP_AUTO_UPDATE=0`. Each `run(message)`, `run_stream(message)` or
`apply_response(reply)` creates a separate operation scope. `apply_response`
accepts an already generated Aider-format reply and calls the real
`Coder.apply_updates()` pipeline, including dry-run matching and authorization.
The application can inspect the original `session.coder` and its
`reflected_message` after an error.

## Formats and governance

| Route | Supported format names |
|---|---|
| SEARCH/REPLACE | `diff`, `diff-fenced`, `editor-diff`, `editor-diff-fenced` |
| Whole file | `whole`, `editor-whole` |
| Unified diff | `udiff`, `udiff-simple` |

These eight executors use the selected IO for their content writes. The adapter
selects these routes explicitly. Aider's newer `patch` executor also performs
direct deletion/move filesystem calls, so that route belongs to Aider's
ordinary executor rather than this IO-backed selection.

The original IO owns confirmation and display, including application-defined
confirmation behavior. Aider decides which files enter the chat and when a new
file receives an empty placeholder. Its normal git, shell, external-editor and
lint/test lifecycle remains under the application's Aider configuration.
`dry_run=True` preserves the original preview behavior and starts zero native
write processes.

`writable_roots=[...]` optionally confines core commits to absolute directories;
`[]` permits zero commit targets and the default `None` uses the process's OS
authority. This policy covers the adapter's transactions. Aider owns the empty
placeholder creation that happens in its constructor or new-file authorization.

`session.cancel()` marks active IO scopes and aborts their pending core plans.
In-flight commits settle according to core cancellation checkpoints. The host's
actual receipt and file bytes determine the completed state. `session.restore()`
returns subsequent IO calls to the captured original reader/writer;
`session.close()` restores that behavior and drains/closes the owned child.
The original format registry and prompt objects stay in place throughout.

## File representation

InputOutput retains its selected encoding and `line_endings` behavior. Its reader
uses Python universal newlines. Formats that express a trailing newline keep
that representation, including full-file SEARCH/REPLACE and whole-file replies.
Core writes the bytes supplied by that host policy. UTF-8 content uses string
transport; legacy encoding bytes use the byte-array mode where needed.

Core prepares the encoded payload before persistence. For example, a Latin-1
target receiving an unrepresentable replacement keeps its existing bytes when
encoding fails; the original direct TextIO writer opens/truncates before its
encoding error. Real SDK tests retain this explicit storage difference.

Existing final symlinks resolve to their regular target. Atomic replacement
retains ordinary permissions and needs read access to an existing snapshot plus
write access to the containing directory. Hard links receive a replacement
inode. The [core persistence contract](architecture.md#persistence-and-limits)
describes external-writer and filesystem metadata boundaries. Each serialized
native request is bounded at 32 MiB; full-file snapshots use proportional memory.

## Real SDK verification and benchmarks

Prepare the source and hash-locked packages in an isolated host:

```sh
python3.13 benches/native/aider_native_setup.py
python3.13 benches/native/aider_native_setup.py --check
target/native-hosts/aider-5dc9490/aider-venv/bin/python tests/aider_native_contract.py
target/native-hosts/aider-5dc9490/aider-venv/bin/python tests/aider_provider_bench_contract.py
```

`--destination DIRECTORY` chooses another SDK checkout and environment. The
default is `target/native-hosts/aider-5dc9490`, with `aider-venv` and official
source under `aider`. `native-host.json` records the source revision, module
hashes, selected dependency versions and a successful real Coder round trip.
Setup uses Git, Python's venv and hash-checked pip; verification reads the
prepared environment. Model metadata and disabled analytics stay in temporary
fixture directories. The actual Model/LiteLLM completion entry points are
trapped and counted at zero.

The 24 contract cases cover all eight formats, the original thirteen byte
fixtures across `diff`, `whole` and `udiff`, authorization, new files, dry-run,
encoding, BOM/line endings, stale input, cancellation, actual git policy,
restoration and real installation of both local Python packages.

Run the separate provider benchmark after builds and setup have finished:

```sh
target/native-hosts/aider-5dc9490/aider-venv/bin/python benches/aider_provider.py \
  --executable target/release/xnip --repeats 3 --warmups 1
```

The report compares original/native IO through the same real Coder pipelines.
It records actual reply/receipt bytes and hashes, timing, independent file goals,
host-selected representation, child IPC and zero model calls. The source and
executable stay fixed for a measured run. [Benchmark index](benchmarks/README.md)
separates this SDK cohort from the historical editing-executor experiment.
