# DeepSeek Harness integration

DSH has two local integration paths:

| Integration | Tools and behavior | Setup |
|---|---|---|
| Native filesystem provider | Host `read`, `write`, `edit`, observations, sandbox, approval, diff and optional rewind | [Profile installer](native.md#dsh-profile-setup) |
| MCP bundle | Five namespaced xnip tools through the official DSH MCP bridge | The `xnip-cli/dsh` plugin below |

Both paths have real-host coverage for DSH `0.2.1-rc.2` and `0.2.1-rc.5`. The
native bundle selects its provider and suspends an existing `xnip-mcp` entry for
that profile; removing it restores the earlier composition. Restart running
profile sessions after changing their installed bundles.

The `xnip-cli/dsh` entry is an explicit [Cordis plugin](https://github.com/deepseek-ai/deepseek-harness).
It registers xnip through the official `@deepseek-ai/dsh-mcp-client` stdio bridge.
The **xnip-cli** npm package selects the matching xnip platform payload; see
[installation](installation.md). Standard hosts at both verified versions carry
the bridge. DSH's official profile resolver makes that host dependency available
inside profile plugins, including this entry's dynamic import.

## Register the plugin

For a source build, prepare a private host-only tarball using the
[local npm validation guide](releasing.md#local-npm-validation), then add its
absolute path to the intended DSH-managed profile:

```sh
dsh plugin --profile my-profile add /absolute/path/to/xnip-cli-VERSION.tgz
```

The tarball contains the executable for the selected local platform. A published
package can be selected by passing `xnip-cli` to the same command.

The package declares its bundled `integrations/dsh/cordis.patch.yml`, which inserts the `xnip-mcp` plugin. DSH resolves its relative `./index.mjs` beside that patch. This explicit command updates the selected profile; installing the npm package alone does not enable the plugin or rewrite a profile.

The bundle uses the startup process directory. To choose a fixed project, put the following override in that profile's own `cordis.patch.yml`:

```yaml
- id: xnip-mcp
  config:
    cwd: /absolute/path/to/project
```

In an existing programmatic DSH/Cordis deployment with `ctx.tools` mounted, register the same plugin explicitly:

```js
import * as Xnip from 'xnip-cli/dsh';

await ctx.plugin(Xnip, {
  cwd: '/absolute/path/to/project',
});
```

The plugin resolves the native binary through `xnip-cli/native`'s synchronous `resolveNativeExecutable()`. It starts that binary directly with `mcp --receipts text`, sets an absolute subprocess working directory, and sets `XNIP_AUTO_UPDATE=0` for the child. It does not write a user profile or configuration file.

The official bridge exposes the five default tools as:

- `mcp__xnip__xnip_edit`
- `mcp__xnip__xnip_apply`
- `mcp__xnip__xnip_peek`
- `mcp__xnip__xnip_find`
- `mcp__xnip__xnip_recover`

The bridge discovers xnip's tool schemas and server instructions. Normal Cordis plugin disposal closes the bridge and its child process. Initial connection/discovery failures fail plugin startup.

## Configuration

| Option | Default | Meaning |
|---|---|---|
| `cwd` | Process working directory | Project directory for the xnip MCP subprocess; relative values become absolute when the plugin loads |
| `serverName` | `xnip` | Official bridge namespace; choose a distinct name for another server in the same registration scope |
| `receipts` | `text` | `text` for compact acknowledgements, or `structured` for machine-readable change results and output schemas |
| `toolset` | `compact` | Five everyday tools; `full` also advertises specialized line tools and diagnostics |
| `env` | `{}` | Explicit string environment overrides passed to the official bridge |

`XNIP_AUTO_UPDATE=0` is enforced after the supplied environment overrides. The bridge merges explicit overrides over its scrubbed ambient environment; the integration does not copy the complete parent environment and bypass that scrubbing. See [update behavior](updates.md) and [MCP result modes](mcp.md#result-display).

Relative xnip file paths resolve from this subprocess's configured `cwd`. Changing a DSH session's current directory does not retarget an already-running xnip process. Use absolute file paths or register the plugin with the intended project directory.

## Filesystem behavior

The MCP plugin sends file operations to the xnip subprocess. DSH's original
filesystem provider and observation policy continue to serve its native tools.
Read an MCP-modified file through the native read tool before a native mutation
that requires an observed file version. The [native provider](native.md)
integrates writes with that host observation policy directly.

Use the [xnip operation guide](SKILL.md) for exact replacements, guarded whole-file writes, insertion, batching, previews, checks and recovery. Text receipt mode keeps previews and recovery-journal details visible; select structured receipts when a consumer needs the machine fields.

## Host-local dependency resolution

A standard DSH-managed profile can use the host's bridge without installing another copy in the profile. DSH's [installation dependency mapping](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/boot/app-boot/src/profile.ts#L375-L431) and [ESM/CommonJS resolver](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/boot/app-boot/src/profile-resolution/resolver.ts#L836-L876) supply it to the plugin's own imports. A profile-local copy takes priority, so adding one also changes dependency selection.

A custom Cordis deployment without that DSH resolver needs the official peer in a module tree visible to the installed xnip package. If its bridge is elsewhere, inject the host-loaded official module explicitly. This keeps the package's native executable resolver and changes only where the official bridge is loaded:

```js
import * as McpClient from '@deepseek-ai/dsh-mcp-client';
import { createXnipPlugin } from 'xnip-cli/dsh';

const Xnip = createXnipPlugin({
  loadMcpClient: async () => McpClient,
});

await ctx.plugin(Xnip, { cwd: '/absolute/path/to/project' });
```

`createXnipPlugin({ resolveNativeExecutable, loadMcpClient })` also provides a source-development seam for supplying the package resolver and a separately loaded official runtime. These functions are programmatic dependencies; they are not saved profile options.

## Verification

For the full native-provider lifecycle and MCP selection/restoration matrix,
follow [native host E2E setup](testing.md#native-host-e2e). It starts the actual
official hosts at both verified versions, checks registered tools and schemas,
executes edits and retains zero-model-generation evidence. The unified matrix
covers native hosts and SDK integrations; `python3 tests/native_e2e.py --host dsh`
selects the DSH checks. Setup includes Node 24, Python 3, Rust and the official
plugin manager's pnpm 11.25.0 CLI.

The historical benchmark and worker contracts below pin official DSH
[commit 687f282e8625c6b8d11ff58211a90524be2e4317](https://github.com/deepseek-ai/deepseek-harness/tree/687f282e8625c6b8d11ff58211a90524be2e4317)
/ `0.2.1-rc.2`. Tool naming, stdio configuration and disposal follow its
[official MCP client](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/mcp/mcp-client/README.md).

From a source checkout, use Node 22.19+ or 24+ to prepare the isolated official DSH runtime explicitly, then verify its recorded source, dependency and bundle hashes:

```sh
node benches/native/dsh_setup.mjs
node benches/native/dsh_setup.mjs --check
```

This historical adapter preparation fetches pinned official source and locked
dependencies with dependency lifecycle scripts disabled. `--check` verifies its
cache. Its runtime is `target/native-bench/dsh-0.2.1-rc.2/runtime.mjs`; native host
E2E uses separately prepared official npm hosts through `dsh_native_setup.mjs`.

Run the configuration contracts independently:

```sh
node --test tests/dsh_integration_contract.cjs
```

The opt-in live test requires a real xnip binary and an isolated module exporting the official `Context`, `ToolRuntime`, `SystemPrompt`, `McpClient` and `loadOptionalPatches` (from DSH app-boot). Set `XNIP_DSH_TEST_RUNTIME` to that module's absolute path and `XNIP_DSH_TEST_BINARY` to the executable's absolute path, then run:

```sh
node --test tests/dsh_integration_live.cjs
```

The live test checks bundle-patch resolution through DSH's official loader separately from the actual official bridge and MCP subprocess. It checks discovered tools and disk bytes, preserves Unicode/CRLF/missing EOF newline, and verifies that preview/check leave files unchanged. It skips when neither opt-in path is configured; the loader check also reports a skip if its official export is unavailable. It never substitutes a fake file executor.

The installed-package test additionally requires `XNIP_DSH_TEST_INSTALL`, an absolute npm consumer directory containing the actual installed private host tarball. That consumer must also resolve the pinned official bridge, Cordis, ToolRuntime and SystemPrompt packages. The test does not install dependencies. It exercises the package's default resolver and bridge import without injection, then uses the official profile resolver to load a separate profile copy from the host's dependency tree, with no profile-local bridge:

```sh
node --test tests/dsh_packaged_live.cjs
```

The independent native-tool worker contracts use the same prepared runtime:

```sh
python tests/dsh_adapter_contract.py
```

Set `XNIP_DSH_TEST_BINARY` to the locally built executable when running the source bridge checks, including the `.exe` suffix on Windows. The packaged test uses the installed tarball's verified executable instead. The worker contracts cover all 13 benchmark fixtures through both filesystem and string-editor tools, with real reads/views in observed modes; bare modes explicitly omit those observation guards. These commands exercise local tools and captured protocol behavior without model calls or a DSH UI session.
