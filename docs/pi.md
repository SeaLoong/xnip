# Pi integration and native benchmark

This integration is verified against official Pi commit
[fdce48eb404ec014adaf2ac10a3a53bc6b1740c2](https://github.com/earendil-works/pi/tree/fdce48eb404ec014adaf2ac10a3a53bc6b1740c2),
whose coding-agent package version is **1.3.0**. Local verification uses Node 24.13.0
and real filesystem IO, with no model/provider request.

The integration ships in the **xnip 0.5.0+** npm package. Install it using Pi's
package manager, or load a matching local development package.

## Extension

The entry is [integrations/pi/index.mjs](../integrations/pi/index.mjs).
The npm packer copies that directory verbatim and advertises:

~~~json
{
  "pi": {
    "extensions": ["./integrations/pi/index.mjs"]
  }
}
~~~

Install the published package explicitly:

~~~sh
pi install npm:xnip
# Or load it for one invocation without adding it to settings:
pi -e npm:xnip
~~~

For a complete local development package with its matching native payload,
Pi can also load its directory using the same package mechanism:

~~~sh
pi -e /absolute/path/to/prepared/xnip-package
~~~

The directory form is a local package path. Both forms use Pi's package loader
to discover the manifest entry; a second plugin package is unnecessary.
The installed-tarball contract invokes the official `DefaultPackageManager.resolveExtensionSources`
used by `-e`, verifies the manifest-resolved `.mjs` entry, then loads its default
factory and executes the real MCP lifecycle. It does not start a full Pi session.

On `session_start` the extension calls Pi's real `registerMcpServer` API. It uses
`require("xnip/native").resolveNativeExecutable()` from the containing npm package
to obtain an absolute immutable-cache executable, then registers `xnip` with
`args:["mcp"]`, the session's `ctx.cwd`, `exposure:"direct"`, and
`XNIP_AUTO_UPDATE:"0"`. Pi owns transport creation, discovery, tool execution
hooks, permissions, result projection and connection teardown. The extension
unregisters the server on `session_shutdown`; shutdown is idempotent.

The public tool names are `mcp__xnip__xnip_edit`, `mcp__xnip__xnip_apply` and the
other discovered xnip names. Pi's own `edit` and `write` stay active and keep their
own input contracts. This extension neither rewrites their arguments nor
silently substitutes their implementation. A same-name `xnip` server in
`mcp.json` takes precedence according to Pi's own rules.

Pi's default MCP exposure is codemode. The extension explicitly chooses direct
exposure so tools are callable and visible directly. Registry conflicts, invalid
config and a Pi version lacking `registerMcpServer/unregisterMcpServer` fail
visibly instead of falling back to a different execution path.

The MCP process cwd is captured when the session starts. Reload/restart its
registration after a session cwd change; absolute paths also remain explicit.
A remote filesystem override of Pi's built-in tools does not automatically
redirect a local xnip process. Tool approval hooks and OS/filesystem confinement
are distinct concerns.

## Real native worker

Start the JSONL worker from the repository:

~~~sh
node benches/native/pi_adapter.mjs
~~~

One JSON object per line:

~~~json
{"action":"describe"}
~~~

~~~json
{"action":"execute","root":"/absolute/fixture","tool":"edit","arguments":{"path":"a.txt","edits":[{"oldText":"old","newText":"new"}]}}
~~~

~~~json
{"action":"execute","root":"/absolute/fixture","tool":"write","arguments":{"path":"new/a.txt","content":"complete text"}}
~~~

The worker clones official sources into `target/native-bench/pi-upstream`,
verifies the fixed commit and refuses changed executor source. It installs the
locked isolated dependencies with `npm ci --ignore-scripts` under
`target/native-bench/pi-v1.3.0`. Nothing is installed globally. The tracked
`pi_package.json` and `pi_package-lock.json` define the dependency closure.

An esbuild bundle imports the unchanged official `createEditTool`,
`createWriteTool`, argument validation, path resolver, mutation queue, fuzzy/diff
implementation and result content. No matching or write algorithm is reimplemented.
Uninvoked interactive JIT-loading helpers remain lazy references to the locked
official SDK package; no tool body, filesystem operation or model render is
replaced by a stub. Source fingerprints, dependency-lock SHA256, runtime-bundle
SHA256 and the context adaptations are returned by `describe`.

Execution replies include `ok`, `content` / `model_visible_result`, `output`,
`native_result`, error information and `elapsed_ms`. Pi's UI diff and patch live
in `native_result.details`; the model-facing success receipt is `content`.
The JSONL control envelope and source fingerprints are benchmark evidence,
not model tool arguments.

### Contracts which affect fair comparisons

| Pi native interface | Observable contract |
| --- | --- |
| `edit {path,edits:[{oldText,newText}]}` | Every match uses the original file; edits must be unique and disjoint. Dependent edits need separate calls or one merged block. |
| Native multi-edit | One file per call; no cross-file transaction. Late invalid entries reject before the normal write. |
| Matching | Official exact-first/fuzzy fallback, including Unicode/whitespace normalization, is preserved. Empty old text and no-change operations reject. Its non-overlapping occurrence count can accept `ababa` / `aba`, which differs from xnip's overlap-aware uniqueness check. |
| Text IO | BOM is retained; matching uses LF-normalized content; the detected original EOL is restored. Mixed EOL can become uniform. EOF behavior is native. |
| `write {path,content}` | Whole UTF-8 create/overwrite, including parent directories and empty content; no user-supplied old guard. Use the snapshot-enabled cohort. |
| Mutation queue | Same-process file mutations use Pi's actual `withFileMutationQueue`. |
| Cancellation | Native signal checks and settled IO/queue ownership are preserved. A late abort can follow a completed write; no rollback guarantee is added. |

Pi also has argument preparation for models which send `edits` as a JSON string,
a single edit object, or legacy fields. The worker runs that official preparation
before the official schema validator; its advertised schema remains Pi's current
`path + edits[]` shape.

## xnip through Pi's real MCP bridge

The same worker accepts:

~~~json
{"action":"describe","mode":"xnip-mcp","root":"/absolute/fixture","xnipExecutable":"/absolute/immutable/xnip"}
~~~

~~~json
{"action":"execute","mode":"xnip-mcp","root":"/absolute/fixture","xnipExecutable":"/absolute/immutable/xnip","tool":"xnip_edit","arguments":{"file":"a.txt","old":"old","text":"new"}}
~~~

`tool` is the raw xnip wire name. This mode loads the integration through official
`loadExtensionFromFactory/createExtensionRuntime` and invokes its actual
registered session handlers. It then uses official `McpServerConnection`,
`createDefaultTransport` and `createMcpToolDefinition` for real stdio discovery,
calls and model-content projection. The fixture supplies a cwd-only event context;
it does not claim to run the full AgentLoop or its approval/permission extensions.

`{"action":"reset"}` closes the bridge and unregisters the fixture session.
`action:"lifecycle"` records registration, repeated shutdown and final connection
state. Both EOF and explicit reset await transport closure. This worker does not
make paid model calls, run a UI or alter user Pi settings.

For local installed-package verification, replace `xnipExecutable` with
`extensionPath:"/absolute/consumer/node_modules/xnip/integrations/pi/index.mjs"`.
The same official loader then imports that installed default factory; its
`xnip/native` self-reference resolves the packaged immutable executable normally.
This opt-in path is a package acceptance check, separate from the benchmark's
explicit recorded executable.

Measure this lane separately from direct xnip MCP wire cost: Pi can wrap MCP
output schemas, project result content, retain canonical data and truncate large
MCP text according to its own implementation. The returned real Pi tool
definitions and real model-visible `content` are the accounting surface.

## Verification

~~~sh
node --test tests/pi_contract.mjs
~~~

For a different locally built native binary set `XNIP_PI_TEST_EXECUTABLE` to its
absolute path. Source tests inject that resolver into `createXnipExtension`;
the packaged default uses `xnip/native`. Set `XNIP_PI_TEST_PACKAGE` to the
absolute installed `node_modules/xnip` directory to also test that default entry
through real Pi registration, stdio, editing and teardown. The local private
0.5.0 tarball passed all 11 tests with this check enabled and no skipped tests.
The native executable's version/hash must be recorded by the enclosing benchmark run.

The contracts cover schemas/pins/hashes; original-coordinate multi-edit;
BOM/CRLF/EOF; dependency/overlap/ambiguity/no-op failure; native fuzzy matching;
mixed-EOL normalization; real concurrent mutation queue; argument preparation;
abort-before-write; create/overwrite/empty write; real host MCP registration,
discovery, preview/write/error projection; and idempotent shutdown.

A smoke over the shared 13 fixtures was byte-exact for Pi edit on this machine.
Its global-replacement input used one whole-file old/new guard because Pi has no
replace-all flag; multi-file required three calls. This is a correctness check,
not a token score, model success rate or claim of cross-file atomicity.
The central native benchmark runner owns published comparative measurements.

## Official sources

- [Pi extension MCP registration](https://github.com/earendil-works/pi/blob/fdce48eb404ec014adaf2ac10a3a53bc6b1740c2/packages/coding-agent/docs/extensions.md#mcp-servers)
- [Package loading](https://github.com/earendil-works/pi/blob/fdce48eb404ec014adaf2ac10a3a53bc6b1740c2/packages/coding-agent/docs/packages.md)
- [Native edit](https://github.com/earendil-works/pi/blob/fdce48eb404ec014adaf2ac10a3a53bc6b1740c2/packages/coding-agent/src/core/tools/edit.ts)
- [Native write](https://github.com/earendil-works/pi/blob/fdce48eb404ec014adaf2ac10a3a53bc6b1740c2/packages/coding-agent/src/core/tools/write.ts)
- [Native diff/fuzzy matching](https://github.com/earendil-works/pi/blob/fdce48eb404ec014adaf2ac10a3a53bc6b1740c2/packages/coding-agent/src/core/tools/edit-diff.ts)
- [Pi MCP bridge](https://github.com/earendil-works/pi/blob/fdce48eb404ec014adaf2ac10a3a53bc6b1740c2/packages/coding-agent/src/extensions/mcp/runtime.ts)
