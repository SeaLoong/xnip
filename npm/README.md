# xnip

Precise text edits for coding agents, available as a CLI and stdio MCP server.
Install the `xnip-cli` npm package to get the `xnip` command, JavaScript host
adapters and client-profile generator. Standalone binaries and installers are also available in the
[installation guide](https://github.com/SeaLoong/xnip/blob/master/docs/installation.md).

## Install

Requires Node **22.14+**. Supported platforms are Linux, macOS and Windows, each
x64 or ARM64. Keep optional dependencies enabled so npm can select the native
executable for your system. No Rust compiler is required.

```sh
npm install --global xnip-cli@latest
xnip --version
xnip doctor
```

If `xnip` is missing after installation, inspect `npm prefix --global`. npm puts
global commands in `<prefix>/bin` on macOS/Linux and `<prefix>` on Windows.
Add that command directory to the launching shell or client's `PATH` using the
[platform-specific checks](https://github.com/SeaLoong/xnip/blob/master/docs/installation.md#find-the-global-command).
The full command path can verify the installed package before updating `PATH`.

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
      "args": ["mcp", "--cwd", "/absolute/project", "--receipts", "text"]
    }
  }
}
```

The client must be able to find the installed command. Replace the project path
with the intended absolute directory. `--cwd` anchors relative tool paths for
both GUI and CLI launchers. Text receipts keep editing results
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
        "--cwd",
        "C:/absolute/project",
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
for tool arguments, previews and result modes. The
[support matrix](https://github.com/SeaLoong/xnip/blob/master/docs/support.md)
lists current SDK toolsets and 14 official-client configuration profiles.

## Client MCP profiles

The package includes the integration catalog and standard-library profile
generator. With Python 3.11+ and a project-local npm installation, first print
the verified executable path:

```sh
node -e "process.stdout.write(require('xnip-cli/native').resolveNativeExecutable())"
```

Pass that path and the intended project to the generator:

```sh
python3 node_modules/xnip-cli/integrations/host-profiles/xnip_profiles.py generate \
  --executable /absolute/verified/xnip --project /absolute/project \
  --output xnip-client-profiles
```

The fresh output directory contains fourteen client-specific configurations.
Review and apply the chosen artifact using the
[profile guide](https://github.com/SeaLoong/xnip/blob/master/docs/host-profiles.md).
Client permissions, approval and removal follow the application's lifecycle.

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

## Native host and SDK adapters

Select native entries explicitly when loading a Pi extension or constructing
an SDK session. The JavaScript adapters resolve the installed xnip executable;
the native setup and test matrix use Node 24.

`xnip-cli` 0.6.3 bundles the DSH native adapter. The
[DSH lifecycle guide](https://github.com/SeaLoong/xnip/blob/master/docs/native.md#dsh-profile-setup)
covers installation, verification, restoration and cleanup through the installed
command.

| Entry | Host seam and verified version |
|---|---|
| `xnip native install --profile ... --dsh ...` | [DSH rc.2 / rc.5 Linux host journey](https://github.com/SeaLoong/xnip/blob/master/docs/native.md#dsh-profile-setup), using the package's bundled adapter |
| `xnip-cli/pi-native` | [Pi 1.3.0 same-name tools](https://github.com/SeaLoong/xnip/blob/master/docs/pi-native.md) through its extension loader |
| `xnip-cli/gemini-native` | [Gemini core 0.27.2](https://github.com/SeaLoong/xnip/blob/master/docs/gemini-native.md) local `Config` filesystem service and per-tool scope |
| `xnip-cli/cline-native` | [Cline SDK/core 0.2.0](https://github.com/SeaLoong/xnip/blob/master/docs/cline-native.md) public `apply_patch` tool executor |
| `xnip-cli/opencode` | [OpenCode plugin tools](https://github.com/SeaLoong/xnip/blob/master/docs/opencode.md): `xnip_edit`, `xnip_write`, `xnip_apply`, `xnip_read` |
| Local `xnip-openhands` Python package | [OpenHands SDK/tools 1.60.1](https://github.com/SeaLoong/xnip/blob/master/docs/openhands-native.md) `file_editor` factory; Python 3.12+ |

Each guide covers session construction, host tool execution and restoration.
The [DSH provider guide](https://github.com/SeaLoong/xnip/blob/master/docs/native.md)
describes installation, verification, removal and cleanup through the installed
`xnip` command. Existing MCP registrations retain their
original setup paths above.

The support matrix also covers aider IO, DeepAgents backends, LangGraph tools
and official PydanticAI/OpenAI Agents/AutoGen/smolagents/Google ADK/CrewAI MCP toolsets. Python
integrations use their own locked virtual environments and local packages.

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
