# xnip in Goose

Use xnip **0.5.0+**, installed through npm or a standalone release, as a stdio MCP extension. A local `cargo build --release` is also supported; 0.4 binaries use an older MCP SDK. Start Goose in the project directory:

```sh
goose session --with-extension "xnip mcp --receipts text"
```

For a saved extension, merge [config.yaml](config.yaml) into the Goose configuration. The [Windows example](config.windows.yaml) uses a direct `.exe` path and YAML single quotes, which preserve backslashes and spaces. Set `cwd` to an existing absolute project directory, or omit it to use the Goose session directory. Keep executable paths in `cmd` and each argument in `args`; the saved configuration does not need shell quoting.

Append [.goosehints](.goosehints) to the project's existing `.goosehints` or `AGENTS.md` to prefer xnip editing. Goose prefixes extension tools, so an extension named `xnip` normally exposes `xnip__xnip_edit`. The original developer extension can remain available for shell/search; this integration does not silently remove it.

The saved configuration and session command are official Goose extension formats. [Verification and benchmark scope](../../docs/goose.md) distinguish a native editor run, a real Goose MCP client connection, and a complete model-driven Goose session.
