# Pi

The npm package copies this directory verbatim and points `pi.extensions` at
`./integrations/pi/index.mjs`. It registers xnip using Pi's own MCP API and the
package's `xnip/native` resolver, with direct exposure and native auto-update
disabled. Native Pi edit/write remain unchanged.

See [integration and benchmark documentation](https://github.com/SeaLoong/xnip/blob/master/docs/pi.md) for the pinned
host version, lifecycle, registry/local package setup and real-executor contracts.
