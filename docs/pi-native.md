# Pi native file tools

The Pi native extension routes the existing `read`, `edit`, and `write` tools through xnip's Rust `core`. Pi owns the tool definitions, exact-first/fuzzy matching, original-file multi-edit coordinates, BOM/EOL conversion, mutation queue, prompts, result text, image processing, and UI renderers. xnip supplies file reads and guarded transactions through one `xnip native serve` subprocess per extension session.

Support is verified against **Pi 1.3.0**, official source commit [`fdce48eb404ec014adaf2ac10a3a53bc6b1740c2`](https://github.com/earendil-works/pi/tree/fdce48eb404ec014adaf2ac10a3a53bc6b1740c2). The extension uses Pi's public `createReadToolDefinition`, `createEditToolDefinition`, and `createWriteToolDefinition` operation interfaces, the same mechanism used by Pi's [SSH example](https://github.com/earendil-works/pi/blob/fdce48eb404ec014adaf2ac10a3a53bc6b1740c2/packages/coding-agent/examples/extensions/ssh.ts). `registerTool()` replaces a builtin with the same name inside Pi's actual tool registry.

## Load the extension

Install `xnip-cli` in the current project, then load its native extension with
the verified Pi 1.3.0 host:

```sh
npm install xnip-cli
pi -e ./node_modules/xnip-cli/integrations/pi-native/index.mjs
```

The installed entry resolves the checksum-verified platform executable through `xnip-cli/native`. Execution uses its prepared cache, with `XNIP_AUTO_UPDATE=0`. The `xnip-cli/pi-native` export names this entry for package consumers. The package's default Pi manifest continues to load the [MCP extension](pi.md); the native extension is selected explicitly. Both integrations can coexist, with MCP keeping its separate tool names.

The native extension preserves existing MCP registrations and active-tool choices. To select the original names while keeping MCP's read-only tools and other servers, Pi supports a per-run selection:

```sh
pi --exclude-tools 'mcp__xnip__xnip_edit,mcp__xnip__xnip_apply,mcp__xnip__xnip_recover' \
  -e ./node_modules/xnip-cli/integrations/pi-native/index.mjs
```

This selection belongs to Pi's invocation. Extension authors can instead use `pi.getActiveTools()` and `pi.setActiveTools()` to change only the active declarations while keeping tools registered, then restore the captured list. The package E2E proves this explicit selection with both xnip and an unrelated real MCP server, including restoration of the original builtin/MCP loadout.

For a persistent setup, add the absolute native entry path to the existing `extensions` array in Pi's settings and run `/reload`. Remove that entry and run `/reload` to restore the builtin tools. Pi's `session_shutdown` lifecycle closes the owned subprocess before rebuilding the registry. The `xnip native install` profile transaction currently applies to DSH; Pi activation follows Pi's extension lifecycle.

For a source checkout, run `cargo build --locked`, then create a local Pi extension that selects the built executable:

```js
import * as host from '@earendil-works/pi-coding-agent';
import { createXnipNativeExtension } from '/path/to/xnip/integrations/pi-native/extension.mjs';

export default createXnipNativeExtension({
  loadHost: async () => host,
  resolveExecutable: () => '/path/to/xnip/target/debug/xnip',
});
```

Load this file with `pi -e /path/to/extension.mjs`. On Windows, select `target/debug/xnip.exe` in the resolver. The static SDK import lets Pi's official loader supply the installed SDK even when Pi and xnip live in separate npm directories.

## Tool and persistence behavior

| Surface | Behavior |
| --- | --- |
| Public names and arguments | Pi's unchanged `read {path, offset?, limit?}`, `edit {path, edits:[{oldText,newText}]}`, and `write {path, content}` |
| Multi-edit | Every block matches the original file; Pi validates uniqueness and disjoint regions before writing |
| Results and UI | Pi's original text, structured read results, diff/patch details, and renderers |
| Hooks | Pi's normal `tool_call` permission handlers and `tool_result` transformations wrap execution |
| Read authority | Pi's normal local filesystem access checks, MIME detection, and image processing |
| Write authority | Pi's normal hooks and OS permissions; transactions request the same OS authority |
| Transaction | A guarded whole-file write uses the exact bytes read by Pi's edit operations; core checks drift before persistence |
| Cancellation | Pi retains the mutation queue until IO settles; active plans receive cancellation, and completed receipts determine the written state |
| Transport | UTF-8 text requests and receipts are negotiated through capabilities; binary content and older native sessions use byte arrays |

Pi's fuzzy matching and mixed-EOL normalization remain Pi behavior. xnip's stricter literal matching is available through the CLI and MCP surface. Pi's `edit` schema represents a global replacement as one unique block containing the original file; the byte-fixture test uses that lowering explicitly.

Transactions use xnip's atomic persistence and preserve file permissions. Snapshot preparation needs read access to an existing target, and atomic replacement needs write access to its containing directory; Pi's builtin direct write can succeed with a write-only file or a directory that permits updating an existing file in place. Updating an existing final symlink resolves and changes its regular target while retaining the link. A dangling final symlink reaches core's symlink boundary; hard-linked files receive a replacement inode instead of an update shared by every link. These permission and filesystem identity cases are distinct from the regular-file byte contract. The native protocol accepts request lines up to 32 MiB.

## Real-host verification

```sh
node --test --test-concurrency=1 --test-reporter=tap \
  tests/pi_native_contract.mjs tests/pi_native_bytes.mjs tests/pi_native_packaged.mjs
```

The fixture pins and hashes official Pi sources and installs its locked package dependencies under `target/native-bench/`. It loads an actual extension from disk, constructs an idle SDK session, and executes tools through official `runToolCall()` with that session's permission and result hooks. Test execution stays within the tool pipeline and performs zero model calls. Sequential test-file execution keeps shared upstream preparation ordered.

Coverage includes all 13 original benchmark byte fixtures, exact schema/prompt/renderer comparisons, original-coordinate multi-edit, fuzzy/BOM/EOL/error parity, mutation queue, cancellation, argument preparation, pagination, truncation, image output, binary transport, and live symlinks. The package test prepares and installs a private local tarball, loads its default MCP entry together with the explicit native extension, selects active tools through Pi's official API, executes another real MCP server, removes the native extension through settings and session reload, verifies subprocess exit, restores the prior loadout, and executes both builtin and MCP edits.
