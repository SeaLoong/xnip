# xnip

Precise text edits for coding agents, available as a CLI and stdio MCP server.
Install the `xnip-cli` npm package to get the `xnip` command and optional DSH/Pi
integrations. Standalone binaries and installers are also available in the
[installation guide](https://github.com/SeaLoong/xnip/blob/master/docs/installation.md).

## Install

Requires Node **22.14+**. Supported platforms are Linux, macOS and Windows, each
x64 or ARM64. Keep optional dependencies enabled so npm can select the native
executable for your system. No Rust compiler is required.

```sh
npm install --global xnip-cli
xnip --version
xnip doctor
```

For a project-local installation:

```sh
npm install xnip-cli
npx --no-install xnip --version
```

For one invocation, select the package explicitly with
`npx --package xnip-cli xnip --version`. Installing the package does not enable
plugins or change your agent's configuration.

## Edit a file

Preview and then apply a replacement for a unique block you already know:

```sh
xnip edit config.toml --old "port = 8080" --text "port = 3000" --dry-run
xnip edit config.toml --old "port = 8080" --text "port = 3000"
```

A missing or ambiguous old block fails without writing. Exact edits preserve
untouched bytes, line endings and the final newline. See the
[operation guide](https://github.com/SeaLoong/xnip/blob/master/docs/SKILL.md)
for insertions, batches, file creation, checks and recovery.

## Connect an MCP client

Add xnip to your client's stdio MCP configuration:

```json
{
  "mcpServers": {
    "xnip": {
      "command": "xnip",
      "args": ["mcp", "--receipts", "text"]
    }
  }
}
```

The client must be able to find the installed command. Set its working directory
to your project, or use absolute target paths. Text receipts keep editing results
concise. Five everyday tools are advertised by default.

On Windows, some clients cannot directly spawn npm's `xnip.cmd` shim. Use Node
with the installed launcher instead:

```json
{
  "mcpServers": {
    "xnip": {
      "command": "C:/Program Files/nodejs/node.exe",
      "args": [
        "C:/absolute/path/to/node_modules/xnip-cli/bin/xnip.cjs",
        "mcp",
        "--receipts",
        "text"
      ]
    }
  }
}
```

Replace both paths with your installation paths; `npm root --global` shows the
global `node_modules` directory. No launcher text is written to MCP stdout.
Programmatic clients can load `xnip-cli/native`: `resolveNativeExecutable()`
returns an absolute verified executable path, and `childEnvironment()` prepares
its environment with native self-update disabled.

See the [MCP reference](https://github.com/SeaLoong/xnip/blob/master/docs/mcp.md)
for tool arguments, previews and result modes.

## DSH and Pi

Activate the integration explicitly through your host's package manager.

DeepSeek Harness:

```sh
dsh plugin --profile my-profile add xnip-cli
```

Replace `my-profile` with your DSH profile name. The plugin uses DSH's official
MCP bridge. See the
[DSH guide](https://github.com/SeaLoong/xnip/blob/master/docs/dsh.md)
for host requirements and project configuration.

Pi:

```sh
pi install npm:xnip-cli
# Or load it for one invocation:
pi -e npm:xnip-cli
```

See the [Pi guide](https://github.com/SeaLoong/xnip/blob/master/docs/pi.md).
[Goose](https://github.com/SeaLoong/xnip/blob/master/docs/goose.md) connects through
stdio MCP. Existing host file tools remain available.

## Update or uninstall

```sh
npm install --global xnip-cli@latest
npm uninstall --global xnip-cli
```

To pin a version, replace `<version>` in
`npm install --global 'xnip-cli@<version>'`. For a project-local installation,
run the corresponding command without `--global`.

npm owns the installed version; native self-update is disabled. Restart MCP
sessions after updating so they use the new executable. Existing sessions retain
the version they started with.

If an older installation shadows this command, inspect `type -a xnip` on Unix or
`Get-Command xnip -All` on Windows. Missing optional platform packages cause an
explicit error; reinstall with optional dependencies enabled.

The launcher runs a verified executable from a versioned cache. Set
`XNIP_NPM_CACHE_DIR` to choose its directory. Cached versions are not removed
automatically; stop their processes before removing obsolete entries.

## References

- [CLI reference](https://github.com/SeaLoong/xnip/blob/master/docs/cli.md)
- [MCP reference](https://github.com/SeaLoong/xnip/blob/master/docs/mcp.md)
- [Editing guide](https://github.com/SeaLoong/xnip/blob/master/docs/SKILL.md)
- [Installation options](https://github.com/SeaLoong/xnip/blob/master/docs/installation.md)
- [Source builds and local package validation](https://github.com/SeaLoong/xnip/blob/master/docs/releasing.md#local-npm-validation)
