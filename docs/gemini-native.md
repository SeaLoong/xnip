# Gemini SDK native file IO

Gemini CLI core **0.27.2** exposes `Config.setFileSystemService()` for SDK hosts.
The xnip adapter installs that service and keeps Gemini's own `read_file`,
`replace`, and `write_file` tool instances. Gemini continues to own matching,
path validation, confirmation, diff presentation, read pagination and tool
results. Text IO uses the existing xnip core through `xnip native serve`.

The upstream interfaces are
[`Config`](https://github.com/google-gemini/gemini-cli/blob/v0.27.2/packages/core/src/config/config.ts)
and
[`FileSystemService`](https://github.com/google-gemini/gemini-cli/blob/v0.27.2/packages/core/src/services/fileSystemService.ts).

This entry targets SDK applications that own their Gemini `Config` and tool
execution lifecycle. Ordinary Gemini CLI extensions use MCP, skills and hooks;
the SDK service is selected by the application that constructs the session.

## Install in an SDK application

Use Node 24 and install the packages in the SDK application:

```sh
npm install xnip-cli @google/gemini-cli-core@0.27.2
```

Install the service on the same `Config` used by the application's original
Gemini tools:

```js
import { installXnipFileSystem } from 'xnip-cli/gemini-native';

const native = await installXnipFileSystem(config);
try {
  await native.run(() => executeWithApproval(invocation, signal));
} finally {
  await native.close();
}
```

For a source checkout, import
`integrations/gemini-native/index.mjs` and provide
`{ executable: '/work/xnip/target/debug/xnip' }` as the second argument. Build
that executable with `cargo build --locked`; Windows uses `xnip.exe`.

`native.restore()` restores the captured filesystem service. `native.close()`
restores it and drains the owned subprocess. Tool definitions and registrations
retain their identity throughout installation and restoration. Another service
installed later retains ownership of the config.

`executeWithApproval` above is the application's existing SDK scheduler path:
it calls the original invocation's `shouldConfirmExecute`, presents its
confirmation, calls `onConfirm` with the selected outcome, and executes only
after approval. Wrap each complete invocation in `native.run(callback)`.
That scope binds its raw file reads to its eventual write. Concurrent
invocations keep separate snapshots; a changed file produces a guarded
transaction conflict. Gemini's original tool classes remain registered.

## Scope and persistence

The adapter supports local SDK configurations with standard OS file access.
Configurations selecting Gemini's sandbox retain their sandbox service.
Host path restrictions and the original approval policy continue through
Gemini's own tools. Raw service calls have the same filesystem authority as
the invoking process.

Valid UTF-8 travels as strings; binary read content uses byte arrays before
Node-compatible UTF-8 decoding. BOM, line endings and fuzzy/exact replacement
remain Gemini tool behavior. The service writes the complete content supplied
by Gemini with the invocation's raw old-content guard; xnip preserves that
supplied content in its atomic transaction.
Read snapshots require read access to existing files, and atomic replacement
requires write access to their containing directories. Existing final symlinks
resolve to their regular target; hard-linked files receive a replacement inode.
These filesystem identity and permission boundaries follow xnip's
[persistence contract](architecture.md#persistence-and-limits).

The protocol accepts serialized request lines up to 32 MiB. File loading and
transaction snapshots use memory proportional to the affected file. The
filesystem service interface has no cancellation argument; a started IO
operation settles before the service's subprocess closes.

## Real SDK verification

```sh
node benches/native/gemini_native_setup.mjs
node benches/native/gemini_native_setup.mjs --check
node --test-reporter=tap tests/gemini_native_contract.mjs
node --test-reporter=tap tests/gemini_native_bytes.mjs
node --test-reporter=tap tests/gemini_native_packaged.mjs
```

The fixture uses the actual `Config`, `ToolRegistry`, original tools and
confirmation callbacks from the frozen official npm package. Gemini's official
empty-response fake content generator supplies an idle local configuration;
its actual call counter stays at zero. Tool discovery, real file IO, byte
fixtures, approval, workspace restrictions and service restoration run through
the public SDK interfaces. The 24 checks include independent concurrent
invocations, external changes after reads, new-file races and loading the
installed npm entry with its verified native payload.
