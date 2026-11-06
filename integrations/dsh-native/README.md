# Native DSH filesystem provider

This optional package replaces DSH's filesystem provider. DSH continues to own
the `read`, `write` and `edit` tools, permission checks, observations, diff
presentation and rewind events. The provider calls xnip's existing core through
the local `xnip native serve` JSON-lines process.

Use the transactional installer from a source checkout, with an initialized DSH
profile and the intended DSH executable:

```sh
target/debug/xnip native install --host dsh \
  --profile /absolute/DSH_HOME/profiles/web \
  --plugin /absolute/xnip/integrations/dsh-native \
  --dsh /absolute/path/to/dsh
```

Installation packs a local copy with the current xnip executable's absolute path
in its package-owned bundle configuration. The profile user's patch stays
unchanged. The installer records a baseline and restores it after any failed
phase. Use the same arguments with `native uninstall` to restore the original
provider and verify a real edit. Existing host sessions pick up provider changes
after restarting.

The existing `xnip-cli/dsh` MCP integration remains available independently.
Enable this bundle when choosing the native provider; its bundle patch disables
`fs-sandbox` and the existing `xnip-mcp` bridge, then inserts exactly one
`xnip-native-fs` entry. Removing it restores the earlier bundle composition.

Validated host versions are DSH `0.2.1-rc.2` and `0.2.1-rc.5`, with
`dsh-rewind-plugin@0.19.2` for profiles that enable rewind. Validation uses the real official
Loader, tools, idle AgentLoop, approval service, Session, diff presentation and
rewind plugin, without model calls. The lifecycle verifier exercises rewind when
it is selected by the profile. Other host versions are checked for the required
filesystem APIs before installation.

Host-normalized CRLF matching is translated to exact byte edits while preserving
untouched storage bytes, BOM and EOF. Mixed newline spellings in a global
replacement use a guarded whole-file write. The local protocol accepts operation
requests up to 32 MiB; small edits remain small requests even on larger files.

From a checkout with the official hosts prepared under
`target/native-hosts/dsh-0.2.1-rc.5`:

```sh
python3 tests/native_e2e.py --host dsh --version 0.2.1-rc.5
node tests/dsh_native_contract.mjs
node tests/dsh_native_bytes.mjs
node tests/dsh_native_rewind.mjs
node tests/dsh_native_packaged.mjs
cargo test --locked --test dsh_native_lifecycle_contract -- --ignored --nocapture
```

Set `XNIP_DSH_NATIVE_HOST` to a separately prepared host directory for the
individual Node and Rust suites. The unified runner uses `--version` to select
DSH versions and `--hosts-dir` to select their prepared parent directory.
See [native integration documentation](../../docs/native.md)
for the installer state, restoration boundaries and remaining host coverage.
