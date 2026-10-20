# Install and update xnip

npm is the preferred installation method for Node users. It distributes the same
Rust CLI/MCP executable and the optional DSH/Pi entries; it does not compile Rust
or download a binary from a postinstall script. Standalone
[GitHub Releases](https://github.com/SeaLoong/xnip/releases) remain available
without Node.

## npm (preferred)

Requires Node **22.14+**. Install, pin or update a published registry release:

```sh
npm install --global xnip
xnip --version
# Pin a published version:
npm install --global xnip@0.5.1
# Update an npm installation:
npm install --global xnip@latest
```

For unpublished development builds or offline validation, install a prepared
private tarball for your host:

```sh
npm install --global --offline --ignore-scripts ./xnip-0.5.1.tgz
xnip --version
```

Private host-only tarballs bundle the executable for the host that prepared them;
see the [npm guide](../npm/README.md) for local builds.
A full registry release consists of the parent `xnip` package and six
exact-version optional platform packages; keep optional dependencies enabled.
The package includes `xnip/native`, `xnip/dsh` and `xnip/pi`.
Installation alone does not activate a host plugin or change its configuration.

npm-managed processes disable native self-update and run a verified copy from an
immutable cache. Update through npm, then restart MCP sessions to use the new
version; existing sessions keep their loaded executable. Check `type -a xnip`
on Unix or `Get-Command xnip -All` on Windows if an older standalone executable
appears earlier in PATH. On Windows, direct-spawn MCP hosts should use Node and
an absolute `bin/xnip.cjs` path, or the package's native resolver, as described
in the npm guide. To remove the global npm installation use
`npm uninstall --global xnip`.

## Supported release targets

| System | CPU | Target suffix | Archive |
|---|---|---|---|
| Linux | x86_64 | `x86_64-unknown-linux-musl` | `.tar.gz` |
| Linux | ARM64 | `aarch64-unknown-linux-musl` | `.tar.gz` |
| macOS | Intel | `x86_64-apple-darwin` | `.tar.gz` |
| macOS | Apple Silicon | `aarch64-apple-darwin` | `.tar.gz` |
| Windows | x64 | `x86_64-pc-windows-msvc` | `.zip` |
| Windows | ARM64 | `aarch64-pc-windows-msvc` | `.zip` |

Names include the complete tag, for example `xnip-v0.5.1-x86_64-pc-windows-msvc.zip`. Each binary archive includes the executable, README, MIT license and third-party data notices, with an adjacent `.sha256` file. Builds are unsigned. CI tests current Linux, macOS and Windows runners; older OS releases are not individually certified.

## Install with the script

macOS/Linux, using a POSIX shell:

```sh
curl -fsSL https://github.com/SeaLoong/xnip/releases/latest/download/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
xnip doctor
```

Add the PATH setting to your shell startup file to keep it in future sessions. The default installation directory is `$HOME/.local/bin`.

Windows, using PowerShell:

```powershell
Invoke-RestMethod https://github.com/SeaLoong/xnip/releases/latest/download/install.ps1 | Invoke-Expression
$env:PATH = "$env:USERPROFILE\bin;$env:PATH"
xnip doctor
```

The default directory is `%USERPROFILE%\bin`. Add that directory to your user PATH through Windows Environment Variables for future terminals and restart GUI clients that launch xnip. The script does not rewrite your persistent PATH.

Both installers download the archive and SHA256 sidecar and verify the bytes before installation. You can download and inspect the script before executing it instead of using a pipeline.

## Pin a version or directory

```sh
export XNIP_AUTO_UPDATE=0
curl -fsSL https://github.com/SeaLoong/xnip/releases/download/v0.5.1/install.sh |
  XNIP_VERSION=v0.5.1 XNIP_INSTALL_DIR="$HOME/bin" sh
```

```powershell
$env:XNIP_AUTO_UPDATE = '0'
$env:XNIP_VERSION = 'v0.5.1'
$env:XNIP_INSTALL_DIR = "$env:USERPROFILE\bin"
Invoke-RestMethod https://github.com/SeaLoong/xnip/releases/download/v0.5.1/install.ps1 | Invoke-Expression
```

These examples pin both the initial release and future launches in the current shell. Set `XNIP_AUTO_UPDATE=0` in the agent/client environment as well when it launches xnip separately.

`XNIP_VERSION` accepts `v0.5.1`, `0.5.1`, or `latest` (default). `XNIP_REPO` selects a trusted fork for the installer only, using the same asset naming convention. It does not change the background updater's official release channel; disable automatic updates when retaining a fork build. Current builds perform [silent background updates](updates.md) automatically. To keep a deployment pinned, set `XNIP_AUTO_UPDATE=0` in its terminal/client environment. You can also rerun the installer with `latest` or select an earlier tag manually; close running xnip/MCP processes before a manual Windows installer replacement.

## Manual download and verification

Download an archive and its corresponding `.sha256` sidecar from the same release. Verify before extracting:

```sh
# Linux
sha256sum -c xnip-v0.5.1-x86_64-unknown-linux-musl.tar.gz.sha256
# macOS (substitute the archive for your CPU)
shasum -a 256 -c xnip-v0.5.1-aarch64-apple-darwin.tar.gz.sha256
```

```powershell
$archive = 'xnip-v0.5.1-x86_64-pc-windows-msvc.zip'
$expected = ((Get-Content "$archive.sha256" -Raw).Trim() -split '\s+')[0]
if ((Get-FileHash $archive -Algorithm SHA256).Hash -ne $expected) { throw 'SHA256 mismatch' }
Expand-Archive $archive -DestinationPath .\xnip
.\xnip\xnip.exe --version
```

For a `.tar.gz` archive, use `tar -xzf ARCHIVE`, then place `xnip` in a directory on PATH. SHA256 detects corrupt downloads; it is not an independent signature of the publisher.

## Build from source

Install Rust 1.89 or newer with Cargo and the native platform linker. Windows builds use the MSVC C++ build tools and Windows SDK.

```sh
cargo install --git https://github.com/SeaLoong/xnip --tag v0.5.1 --locked xnip
```

For a checkout, run `cargo install --path . --locked` at its root. Cargo installs to its configured binary directory, normally `~/.cargo/bin`. The explicit `xnip` argument selects the application rather than the `xtask` helper. The repository toolchain follows stable Rust; the minimum compiler is checked separately in CI. There is no separately verified crates.io distribution for this release.

Host-specific activation is covered in [DSH](dsh.md), [Pi](pi.md) and [Goose](goose.md).

## Troubleshooting and removal

| Symptom | Action |
|---|---|
| `xnip` is not found | Use `command -v xnip` or `Get-Command xnip`, check PATH, and restart the terminal/client. |
| Wrong executable starts | Inspect all matches with `type -a xnip` or `Get-Command xnip -All`; remove the stale installation. |
| Download returns 404 | Check the exact tag and supported architecture on the release page. |
| GitHub latest lookup is rate-limited | Set `XNIP_VERSION` to an explicit tag or download the assets manually. |
| Checksum mismatch | Discard the downloaded archive and retry both files from the same release. |
| MCP cannot find relative files | Configure the client's working directory or use absolute target paths. |
| Windows cannot replace the executable | Stop clients using `xnip mcp`, then rerun the installer. |

To uninstall a script/manual installation, delete only the installed `xnip`/`xnip.exe` and remove its PATH entry if that directory is no longer needed. For a Cargo installation, run `cargo uninstall xnip`. Remove xnip's MCP entry from clients as well. Target files and their `.bak` sidecars are not installation files.
