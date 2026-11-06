# xnip for OpenCode

The native plugin registers `xnip_edit`, `xnip_write`, `xnip_apply`, and
`xnip_read` through OpenCode's public plugin API. It retains the host's permission
requests and session cancellation while using xnip's exact byte editing rules.
OpenCode's built-in tools remain available for its fuzzy matching, formatting
and LSP behavior.

Install `xnip-cli` in the project and load its
`integrations/opencode/index.mjs` as a local plugin. The
[OpenCode guide](../../docs/opencode.md) includes activation,
restoration, verified versions and actual tool-only E2E/benchmark scope.

[opencode.json](opencode.json) separately supplies the official local MCP format.
Merge its `mcp.xnip` entry into your existing config. Select the native plugin or
MCP registration explicitly for the session's intended tool surface.
