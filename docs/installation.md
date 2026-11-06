# Install and update xnip

Use the standalone installer or download a binary from
[GitHub Releases](https://github.com/SeaLoong/xnip/releases). Neither requires
Node.js. You can also install through [npm](#npm) or [Cargo](#build-from-source).

## Install with the script

macOS/Linux, using a POSIX shell:

```sh
curl -fsSL https://github.com/SeaLoong/xnip/releases/latest/download/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
xnip doctor
```

Add the PATH setting to your shell startup file to keep it in future sessions.
The default installation directory is `$HOME/.local/bin`.

Windows, using PowerShell:

```powershell
Invoke-RestMethod https://github.com/SeaLoong/xnip/releases/latest/download/install.ps1 | Invoke-Expression
$env:PATH = "$env:USERPROFILE\bin;$env:PATH"
xnip doctor
```

The default directory is `%USERPROFILE%\bin`. Add that directory to your user
PATH through Windows Environment Variables for future terminals and restart GUI
clients that launch xnip. The script does not rewrite your persistent PATH.

Both installers download the archive and SHA256 sidecar and verify the bytes
before installation. You can download and inspect the script before executing
it instead of using a pipeline.

## Download a binary directly

Select the archive for your OS and CPU from the same
[release](https://github.com/SeaLoong/xnip/releases) as its `.sha256` sidecar.

### Supported release targets

| System | CPU | Target suffix | Archive |
|---|---|---|---|
| Linux | x86_64 | `x86_64-unknown-linux-musl` | `.tar.gz` |
| Linux | ARM64 | `aarch64-unknown-linux-musl` | `.tar.gz` |
| macOS | Intel | `x86_64-apple-darwin` | `.tar.gz` |
| macOS | Apple Silicon | `aarch64-apple-darwin` | `.tar.gz` |
| Windows | x64 | `x86_64-pc-windows-msvc` | `.zip` |
| Windows | ARM64 | `aarch64-pc-windows-msvc` | `.zip` |

Archive names follow `xnip-<tag>-<target>.<extension>`. Replace `<tag>` in the
examples with the complete release tag, including its leading `v`.

Verify the checksum before extracting:

```sh
# Linux
sha256sum -c 'xnip-<tag>-x86_64-unknown-linux-musl.tar.gz.sha256'
# macOS (substitute the archive for your CPU)
shasum -a 256 -c 'xnip-<tag>-aarch64-apple-darwin.tar.gz.sha256'
```

```powershell
$archive = 'xnip-<tag>-x86_64-pc-windows-msvc.zip'
$expected = ((Get-Content "$archive.sha256" -Raw).Trim() -split '\s+')[0]
if ((Get-FileHash $archive -Algorithm SHA256).Hash -ne $expected) { throw 'SHA256 mismatch' }
Expand-Archive $archive -DestinationPath .\xnip
.\xnip\xnip.exe --version
```

For a `.tar.gz` archive, use `tar -xzf ARCHIVE`, then place `xnip` in a
directory on PATH. Each archive also includes the README, MIT license and
third-party notices. SHA256 detects corrupt downloads; it is not an independent
signature of the publisher. Builds are unsigned. Older OS releases are not
individually certified.

## Choose a version or directory

Replace `<tag>` with the release tag you want to install:

```sh
export XNIP_AUTO_UPDATE=0
curl -fsSL 'https://github.com/SeaLoong/xnip/releases/download/<tag>/install.sh' |
  XNIP_VERSION='<tag>' XNIP_INSTALL_DIR="$HOME/bin" sh
```

```powershell
$env:XNIP_AUTO_UPDATE = '0'
$env:XNIP_VERSION = '<tag>'
$env:XNIP_INSTALL_DIR = "$env:USERPROFILE\bin"
Invoke-RestMethod 'https://github.com/SeaLoong/xnip/releases/download/<tag>/install.ps1' | Invoke-Expression
```

`XNIP_VERSION` accepts a release tag, its version without the leading `v`, or
`latest` (the default). These examples disable automatic updates in the current
shell. Set `XNIP_AUTO_UPDATE=0` in the agent/client environment too when it
launches xnip separately.

Standalone builds perform [silent background updates](updates.md). Rerun the
installer with `latest` to update manually, or choose an earlier tag to roll
back. Close running xnip/MCP processes before replacing a Windows executable
manually.

`XNIP_REPO` selects a trusted fork for the installer, using the same asset
naming convention. It does not change the background updater's official
release channel; disable automatic updates when retaining a fork build.

## npm

Requires Node **22.14+**. The package is named `xnip-cli`; the installed command
is `xnip`:

```sh
npm install --global xnip-cli@latest
xnip --version
# Pin a version (replace <version>):
npm install --global 'xnip-cli@<version>'
# Update:
npm install --global xnip-cli@latest
```

Keep optional dependencies enabled so npm can install the executable matching
your OS and CPU. The package also contains JavaScript host adapters, the
integration catalog and client-profile generator; activate the selected route
explicitly. See the [support matrix](support.md) and
[npm usage guide](../npm/README.md) for project-local use and MCP configuration.
`xnip-cli` 0.6.4 includes the adapter for the
[DSH native lifecycle](native.md#dsh-profile-setup).
Its recorded DSH host journey runs on Linux; the six executable targets above
describe the CLI/MCP distribution.

### Find the global command

npm creates the command link or shim under its configured global prefix. Your
shell and the application launching MCP select commands through their `PATH`.
Use the npm belonging to your selected Node installation to inspect that prefix:

```sh
npm prefix --global
npm list --global --depth=0 xnip-cli
```

| Platform | Global command directory |
|---|---|
| macOS / Linux | `<prefix>/bin` |
| Windows | `<prefix>` |

On macOS/Linux, verify the installed command directly, then add its directory
to the current shell:

```sh
xnip_npm_prefix="$(npm prefix --global)"
"$xnip_npm_prefix/bin/xnip" --version
export PATH="$xnip_npm_prefix/bin:$PATH"
command -v xnip
type -a xnip
```

In Windows PowerShell:

```powershell
$xnipNpmPrefix = (npm.cmd prefix --global).Trim()
& (Join-Path $xnipNpmPrefix 'xnip.cmd') --version
$env:Path = "$xnipNpmPrefix;$env:Path"
Get-Command xnip -All
where.exe xnip
```

Keep the same command directory in your shell startup or Node version manager
configuration; Windows users can add it to their user `PATH` through Environment
Variables. Reopen terminals and restart GUI clients after changing their launch
environment. `command -v node` / `command -v npm`, or `Get-Command node,npm -All`,
help identify multiple Node installations.

`npm root --global` locates installed modules, including
`xnip-cli/bin/xnip.cjs`. It serves a different purpose from the command directory
above. MCP clients can use an absolute Node executable and that `.cjs` launcher;
see the [direct launcher example](../npm/README.md#connect-an-mcp-client).
The [npm folder reference](https://docs.npmjs.com/cli/v11/configuring-npm/folders#executables)
describes its platform-specific links and shims.

npm manages this installation's updates; native self-update is disabled.
Restart MCP sessions after upgrading. If an older standalone executable appears
first in PATH, inspect `type -a xnip` on Unix or `Get-Command xnip -All` on
Windows. To remove the global npm installation:

```sh
npm uninstall --global xnip-cli
```

## Build from source

Install Rust 1.89 or newer with Cargo and the native platform linker. Windows
builds use the MSVC C++ build tools and Windows SDK. Choose a release tag:

```sh
cargo install --git https://github.com/SeaLoong/xnip --tag '<tag>' --locked xnip
```

For a checkout, run `cargo install --path . --locked` at its root. Cargo normally
installs to `~/.cargo/bin`. The explicit `xnip` argument selects the application
from the workspace.

## Configure your agent

Register `xnip mcp --cwd /absolute/project --receipts text` in your client's
stdio configuration. The [support matrix](support.md) covers host backends,
plugins, SDK toolsets and client profiles; the [MCP guide](mcp.md) describes
tool arguments and receipts.

## Troubleshooting and removal

| Symptom | Action |
|---|---|
| `xnip` is not found | Check the [npm global command directory](#find-the-global-command), or the installation directory for a standalone build, then restart the terminal/client. |
| Wrong executable starts | Inspect all matches with `type -a xnip` or `Get-Command xnip -All`; remove the stale installation. |
| Download returns 404 | Check the exact tag and supported architecture on the release page. |
| GitHub latest lookup is rate-limited | Set `XNIP_VERSION` to an explicit tag or download the assets manually. |
| Checksum mismatch | Discard the downloaded archive and retry both files from the same release. |
| MCP cannot find relative files | Pass `--cwd /absolute/project` in the server arguments or use absolute target paths. |
| Windows cannot replace the executable | Stop clients using `xnip mcp`, then rerun the installer. |

To uninstall a script/manual installation, delete only the installed
`xnip`/`xnip.exe` and remove its PATH entry if that directory is no longer
needed. For a Cargo installation, run `cargo uninstall xnip`. Remove xnip's MCP
entry from clients too. Target files and their `.bak` sidecars are not
installation files.
