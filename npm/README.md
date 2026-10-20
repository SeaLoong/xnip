# Install xnip with npm

`xnip` distributes the same Rust CLI/MCP executable through one public entry.
The main package contains the Node wrapper, native resolver, DSH bundle and Pi
extension. Six optional platform packages hold the native executables; npm
installs the one matching the current OS and CPU.

There is no postinstall download or build, no launcher network request and no
Node native addon. Local development packages remain private; public preparation
requires an explicit complete six-platform release.

## npm installation (preferred)

Requires Node **22.14+**. Install or update a published version:

```sh
npm install --global xnip
xnip --version
npm install --global xnip@latest
```

Keep optional dependencies enabled. Ordinary users install only `xnip`; they
do not need to select a platform package. The six supported targets are Linux
musl, macOS and Windows, each x64/ARM64. A separate `dsh-xnip` package is
unnecessary. For project-local use, install with `npm install xnip` and invoke
the project's command through `npx xnip`.

npm manages updates; native self-update is disabled. Restart MCP sessions after
upgrading. If an older native installation shadows the npm command, inspect
`type -a xnip` or `Get-Command xnip -All`. Uninstall with
`npm uninstall --global xnip`. Installation alone does not activate a host
plugin, edit its profile or rewrite project instructions.

## Prepare and validate a private local package

Build the current source first. Node 22.14+ and Python 3.11+ are required for
preparation; DSH has its own Node requirement. Tests use Node 24.

```powershell
cargo build --release --locked
python npm/prepare.py --host-binary target/release/xnip.exe --out target/npm-local --pack
npm install --prefix target/npm-install --cache target/npm-cache --offline --ignore-scripts --no-audit --no-fund ./target/npm-local/tarballs/xnip-0.5.0.tgz
node target/npm-install/node_modules/xnip/bin/xnip.cjs --version
```

On Unix use `target/release/xnip`. Use a new `--out` directory each time.
A private host-only parent tarball bundles its host payload for offline testing;
it is not a cross-platform release. Local GNU Linux binaries receive explicit
`*-gnu-local` package names and must not enter public releases. To test a global
installation with a matching host tarball:

```sh
npm install --global --offline --ignore-scripts ./xnip-0.5.0.tgz
xnip --version
```

The tarball can also be installed explicitly through the
[DSH](https://github.com/SeaLoong/xnip/blob/master/docs/dsh.md) or
[Pi](https://github.com/SeaLoong/xnip/blob/master/docs/pi.md) package manager.
Host SDK dependency setup remains separate; offline testing of xnip does not
imply offline installation of an entire harness.

## Complete public release preparation

CI builds all six native archives from one source commit. Each archive has a
SHA256 sidecar and a `.build.json` record binding its version, target, source
commit and archive hash. With those inputs and a matching stamped Cargo version:

```sh
python npm/prepare.py --public --dist PATH_TO_RELEASE_ARTIFACTS --out dist-npm --pack --source-commit COMMIT_SHA
python .github/scripts/publish_npm.py --prepared dist-npm --source-commit COMMIT_SHA --check
```

These two commands prepare and validate artifacts; they do not publish.
Without `--public`, preparation retains private metadata. Public mode rejects
host-only inputs, missing platforms, GNU local payloads and mismatched provenance.
The packer and prepack validation check inventory, binary version/target, hashes
and static Linux linking. The embedded identity and build records prevent
accidental relabelling; they are not independent signatures.

Public releases contain `xnip` plus `xnip-linux-x64-musl`,
`xnip-linux-arm64-musl`, `xnip-darwin-x64`, `xnip-darwin-arm64`,
`xnip-win32-x64` and `xnip-win32-arm64`, at one exact version. The parent
references all six through exact-version optional dependencies. Missing optional
payloads fail explicitly instead of causing an unverified runtime download.

## Entry points

- `xnip`: native CLI arguments pass through the wrapper.
- `xnip mcp --receipts text`: stdio MCP with concise receipts.
- `xnip/native`: `resolveNativeExecutable()` returns an absolute verified
  executable path; `childEnvironment()` disables native self-update.
- `xnip/dsh`: optional DSH plugin entry; bundle activation is explicit.
- `xnip/pi`: Pi extension entry using the host MCP lifecycle.
- [Goose](https://github.com/SeaLoong/xnip/blob/master/docs/goose.md) uses stdio MCP.

For Windows MCP hosts that directly spawn processes, use the absolute Node
executable as `command`, with the absolute `bin/xnip.cjs` path followed by
`mcp --receipts text` in `args`. A terminal's npm `xnip.cmd` shim is not
supported by every direct-spawn host. DSH/Pi entries instead resolve and start
the native cached executable directly.

## Update and lifetime

Child processes force `XNIP_AUTO_UPDATE=0`: package managers own those versions
and the native updater must not modify `node_modules` or a shared package store.
Standalone installations retain their existing updater policy.

The launcher verifies the payload against its manifest and copies it into an
immutable version/platform/SHA256 execution directory. Running MCP images do not
map the package-manager payload. Concurrent first launches publish a complete
directory atomically. Existing cache entries with an invalid hash are refused
and never repaired in place.

The cache is intentionally not automatically collected, avoiding deletion of
mapped executables while process lifetimes are unknown. Remove obsolete entries
only after their processes stop. `XNIP_NPM_CACHE_DIR` selects an explicit cache
directory; otherwise the platform user cache is used. It does not change the
package's pinned executable version.

No launcher text is added to MCP stdout. Native stdout/stderr, input, exit codes
and termination are forwarded. The wrapper owns its child's input pipe so wrapper
death closes native MCP input. Local contracts cover EOF, concurrent starts,
tampering and an active MCP session surviving package removal.

## CI publication

The [release guide](https://github.com/SeaLoong/xnip/blob/master/docs/releasing.md#npm-publication)
describes the full flow. The same six-build run produces the GitHub attachments
and seven npm tarballs in separate directories. All six native installation
checks must pass before publication. The coordinator validates authentication
and all existing registry versions, publishes platform packages first, then
publishes the parent, and verifies registry integrity and the channel tag.

Stable releases use `latest`; development releases use `next`. Rerun failed
jobs with the same verified delivery artifact after a partial failure. Existing
versions are accepted only when their bytes match; they are never overwritten.
Token bootstrap and subsequent Trusted Publisher/OIDC operation are explicit
authentication modes. Credentials are supplied by the publishing environment,
never stored in a package or a checked-in npm configuration.
