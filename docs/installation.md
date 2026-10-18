# Install and update xnip

xnip is distributed through [GitHub Releases](https://github.com/SeaLoong/xnip/releases). Choose a binary for your operating system and CPU architecture. Rust is only needed when building from source.

## Supported release targets

| System | CPU | Target suffix | Archive |
|---|---|---|---|
| Linux | x86_64 | `x86_64-unknown-linux-musl` | `.tar.gz` |
| Linux | ARM64 | `aarch64-unknown-linux-musl` | `.tar.gz` |
| macOS | Intel | `x86_64-apple-darwin` | `.tar.gz` |
| macOS | Apple Silicon | `aarch64-apple-darwin` | `.tar.gz` |
| Windows | x64 | `x86_64-pc-windows-msvc` | `.zip` |
| Windows | ARM64 | `aarch64-pc-windows-msvc` | `.zip` |

Names include the complete tag, for example `xnip-v0.3.1-x86_64-pc-windows-msvc.zip`. Each binary archive includes the executable, README, MIT license and third-party data notices, with an adjacent `.sha256` file. Builds are unsigned. CI tests current Linux, macOS and Windows runners; older OS releases are not individually certified.

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
curl -fsSL https://github.com/SeaLoong/xnip/releases/download/v0.3.1/install.sh |
  XNIP_VERSION=v0.3.1 XNIP_INSTALL_DIR="$HOME/bin" sh
```

```powershell
$env:XNIP_AUTO_UPDATE = '0'
$env:XNIP_VERSION = 'v0.3.1'
$env:XNIP_INSTALL_DIR = "$env:USERPROFILE\bin"
Invoke-RestMethod https://github.com/SeaLoong/xnip/releases/download/v0.3.1/install.ps1 | Invoke-Expression
```

These examples pin both the initial release and future launches in the current shell. Set `XNIP_AUTO_UPDATE=0` in the agent/client environment as well when it launches xnip separately.

`XNIP_VERSION` accepts `v0.3.1`, `0.3.1`, or `latest` (default). `XNIP_REPO` can select a trusted fork with the same asset naming convention. Current builds perform [silent background updates](updates.md) automatically. To keep a deployment pinned, set `XNIP_AUTO_UPDATE=0` in its terminal/client environment. You can also rerun the installer with `latest` or select an earlier tag manually; close running xnip/MCP processes before a manual Windows installer replacement.

## Manual download and verification

Download an archive and its corresponding `.sha256` sidecar from the same release. Verify before extracting:

```sh
# Linux
sha256sum -c xnip-v0.3.1-x86_64-unknown-linux-musl.tar.gz.sha256
# macOS (substitute the archive for your CPU)
shasum -a 256 -c xnip-v0.3.1-aarch64-apple-darwin.tar.gz.sha256
```

```powershell
$archive = 'xnip-v0.3.1-x86_64-pc-windows-msvc.zip'
$expected = ((Get-Content "$archive.sha256" -Raw).Trim() -split '\s+')[0]
if ((Get-FileHash $archive -Algorithm SHA256).Hash -ne $expected) { throw 'SHA256 mismatch' }
Expand-Archive $archive -DestinationPath .\xnip
.\xnip\xnip.exe --version
```

For a `.tar.gz` archive, use `tar -xzf ARCHIVE`, then place `xnip` in a directory on PATH. SHA256 detects corrupt downloads; it is not an independent signature of the publisher.

## Build from source

Install Rust 1.89 or newer with Cargo and the native platform linker. Windows builds use the MSVC C++ build tools and Windows SDK.

```sh
cargo install --git https://github.com/SeaLoong/xnip --tag v0.3.1 --locked xnip
```

For a checkout, run `cargo install --path . --locked` at its root. Cargo installs to its configured binary directory, normally `~/.cargo/bin`. The explicit `xnip` argument selects the application rather than the `xtask` helper. The repository toolchain follows stable Rust; the minimum compiler is checked separately in CI. There is no separately verified crates.io distribution for this release.

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
