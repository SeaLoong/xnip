# Cline SDK native patch backend

The Cline adapter supplies xnip persistence for the official SDK's `apply_patch`
tool. Support is verified with `@cline/sdk` / `@cline/core` **0.2.0**, published
from the official [Cline repository](https://github.com/cline/cline/tree/main/sdk).
It applies to an application that constructs its own SDK tool list and agent.
Cline's VS Code extension and the prebuilt CLI keep their own tool construction.

The official `computePatchChanges()` function owns patch parsing, context/fuzzy
matching and EOL normalization. `createBuiltinTools()` owns the unchanged
`apply_patch` name, description, schema, argument validation, timeout and result
wrapper. xnip lowers the computed file changes to guarded operations in its
existing Rust `core::transaction::Plan`. The SDK continues to run before/after
tool hooks, policies, approval callbacks and tool events around execution.

## Use the SDK backend

Use Node 24 and install the packages in the SDK application:

```sh
npm install xnip-cli @cline/sdk@0.2.0
```

Construct the backend and pass its tools into the application's agent:

```js
import { Agent } from '@cline/sdk';
import { createXnipClineTools } from 'xnip-cli/cline-native';

const backend = await createXnipClineTools({ cwd: process.cwd() });
const agent = new Agent({
  providerId: 'anthropic',
  modelId: 'your-selected-model',
  systemPrompt: 'Your application instructions',
  tools: backend.tools,
  toolExecution: 'sequential',
  toolPolicies: { apply_patch: { autoApprove: false } },
  requestToolApproval: yourApprovalCallback,
});

try {
  // Run the application with this agent.
} finally {
  await backend.close();
}
```

The factory selects Cline's existing `apply_patch` route. Its separate `editor`
route uses its own executor. Options such as `enableBash`, `enableSearch`,
`executors` and `executorOptions` pass through to the official tool factory;
other tools retain their selected implementations. Official `read_files`
requests accept absolute paths, with optional `start_line` and `end_line`.

For a source checkout, build `cargo build --locked`, then import
`createXnipClineTools` from `integrations/cline-native/extension.mjs` and pass
`resolveExecutable: () => '/absolute/path/to/xnip/target/debug/xnip'`.
Windows executable paths end in `.exe`. `loadHost: async () => sdk` lets an SDK
application use its already imported `@cline/sdk` module.

The npm entry selects the checksum-verified executable cache and starts one
`xnip native serve` child lazily on the first patch. UTF-8 text transport is
negotiated with the child; the supported byte-array representation remains
available for older native sessions. Closing the backend settles submitted
transactions and closes the child. Reconstructing tools with the SDK's own
`createBuiltinTools({ enableEditor: false, enableApplyPatch: true })` restores
its original executor. SDK construction leaves user configuration on disk alone.

## Patch and persistence contracts

Create, update, delete, move and multi-file patches use the official patch
computation. The native transaction validates complete raw snapshots before
persistence. A later invalid target leaves earlier files intact. A changed file
keeps the concurrent writer's bytes and receives a tool failure. Cancellation
reaches pending native plans; completed commit receipts describe the written
state. SDK hooks and approval callbacks run before the executor starts.

The SDK's `oldContent` restores a chosen file EOL after matching. The adapter
captures referenced raw files before parsing, checks those exact bytes again
after parsing, and uses them as core's guard. Cline's patch EOL normalization stays visible, including
mixed-EOL normalization. `Add File` rejects an existing file in the official
parser. A move onto an existing destination replaces that destination; a move
to the source's own normalized path follows the official write-then-delete
result and removes that file.
Normalized path aliases retain the official entry order: every match uses the
initial file contents, and successive writes use the preceding planned bytes
as their core guard.

The backend uses UTF-8 patches and the process's filesystem authority, with
Cline's relative-path restriction and its acceptance of absolute paths. Core's
atomic persistence requires a readable existing target and a writable containing
directory, and replaces hard-linked file inodes. Core's final-symlink boundary
applies to patch targets. These filesystem identity and permission cases have
the same [persistence limits](architecture.md#persistence-and-limits) as the other
native adapters. Serialized native requests have a 32 MiB limit.

## Real tools and reproducible verification

```sh
node benches/native/cline_native_setup.mjs
node benches/native/cline_native_setup.mjs --check
node --test --test-concurrency=1 --test-reporter=tap \
  tests/cline_native_contract.mjs tests/cline_native_bytes.mjs \
  tests/cline_native_packaged.mjs
```

Setup installs the fixed official SDK closure with `npm ci --ignore-scripts`
under `target/native-hosts/cline-0.2.0`, checks package versions and source
hashes, and constructs the actual official tool definitions. The fixture creates
a real SDK `Agent`, checks its registry and enters its published JavaScript
tool-dispatch methods directly. Those methods are TypeScript-private: this
test-only entry exercises the actual policy, approval, hook and event chain
while the model stream counter stays at zero. Product integrations use public
SDK factories and the ordinary agent lifecycle.

The contracts cover registered schemas, tool governance, patch lifecycle,
fuzzy/BOM/CRLF behavior, byte fixtures, cancellation, stale files, multi-file
failure and original-executor restoration. The package contract installs a
private tarball, loads its actual export with the default executable resolver,
performs native edits, then executes an edit with the original SDK backend.
