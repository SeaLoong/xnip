# xnip in DeepSeek Harness

The `xnip-cli` npm package copies this directory verbatim and advertises its
`cordis.patch.yml` as a DSH bundle. Explicitly adding that package to a profile
registers the official DSH MCP bridge, using the package's `xnip-cli/native`
resolver and a real xnip subprocess. Native DSH filesystem policies remain separate.

See [integration and verification documentation](https://github.com/SeaLoong/xnip/blob/master/docs/dsh.md)
for profile setup, host dependency resolution and real-executor checks.
