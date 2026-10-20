# Goose integration and native benchmark

xnip connects through Goose's official stdio MCP extension interface. Use **xnip 0.5.0+**, installed through npm or a standalone release; 0.4 binaries use an older MCP SDK that closes the connection during pinned Goose's default discovery lifecycle. For source development, `cargo build --release` also produces a compatible executable. Start Goose in the project directory with:

```sh
goose session --with-extension "xnip mcp --receipts text"
```

For a saved configuration, merge [the extension entry](../integrations/goose/config.yaml) into Goose's `config.yaml`. The [Windows entry](../integrations/goose/config.windows.yaml) uses a direct executable path and separate arguments; single-quoted YAML preserves backslashes and spaces. A configured `cwd` must be an existing absolute project directory. Without `cwd`, Goose starts the MCP process in the session directory. Relative xnip file paths use that process directory. Use separate sessions/processes for different projects.

Goose selects the configuration directory through its platform path strategy; the official extension documentation gives `~/.config/goose/config.yaml` as an example. `GOOSE_PATH_ROOT`, when set to an absolute directory, selects `<root>/config/config.yaml`. This integration supplies examples and does not modify any personal configuration. [Official extension configuration](https://github.com/aaif-goose/goose/blob/9920efa16d078777a4d9715b2f00d8800a4da188/documentation/docs/getting-started/using-extensions.md), [path selection](https://github.com/aaif-goose/goose/blob/9920efa16d078777a4d9715b2f00d8800a4da188/crates/goose/src/config/paths.rs), [stdio working directory](https://github.com/aaif-goose/goose/blob/9920efa16d078777a4d9715b2f00d8800a4da188/crates/goose/src/agents/extension_manager/stdio.rs).

Append [the short editing hints](../integrations/goose/.goosehints) to the project's existing `.goosehints` or `AGENTS.md` when xnip should be preferred. Goose loads these project hints and normally prefixes extension tools: an extension named `xnip` exposes `xnip__xnip_edit`. Keep the developer extension when shell/search tools are needed. `--receipts text` reduces edit/apply/recovery replies while preserving their existing receipts. [Hint loading](https://github.com/aaif-goose/goose/blob/9920efa16d078777a4d9715b2f00d8800a4da188/crates/goose/src/hints/load_hints.rs), [tool name prefixing](https://github.com/aaif-goose/goose/blob/9920efa16d078777a4d9715b2f00d8800a4da188/crates/goose/src/agents/extension_manager/mod.rs).

## Reproduce the native executor

```sh
python benches/native/goose_adapter.py --setup
python benches/native/goose_adapter.py --check
python -m unittest discover -s tests -p goose_native_contract.py
```

Local validation on Windows passed all nine real editor/source-integrity/stdio contracts, including the official default `Auto` lifecycle and the explicit 2025 initialization profile against the current xnip debug build. No model or complete Goose UI session was run.

`--setup` downloads SHA256-verified source from official Goose commit `9920efa16d078777a4d9715b2f00d8800a4da188` (version `1.55.0`) and builds a separate Rust crate. Source, compiled artifacts and setup fingerprints live under `target/native-bench/goose-v1.55.0`. The crate has its own committed `Cargo.lock`; it neither joins xnip's workspace nor changes the root lockfile. `--check` validates existing sources and binary without downloading or compiling. Rust is required only for preparing this optional benchmark. The pinned rmcp SDK declares Rust 1.88 or newer; the adapter was compiled with Rust 1.93.1.

The worker accepts one JSON object per line:

```json
{"action":"describe"}
{"action":"execute","root":"/absolute/existing/fixture","tool":"edit","arguments":{"path":"a.txt","before":"old","after":"new"}}
{"action":"execute","root":"/absolute/existing/fixture","tool":"write","arguments":{"path":"a.txt","content":"complete file"}}
```

The entire official [EditTools source](https://github.com/aaif-goose/goose/blob/9920efa16d078777a4d9715b2f00d8800a4da188/crates/goose/src/agents/platform_extensions/developer/edit.rs), including matching, UTF-8 reads, writes, parent creation, error context and receipts, is compiled unchanged with `include!`. The official edit/write Tool declarations and schema helper are extracted from the verified registration source and compiled verbatim, retaining native schemas and annotations. The adapter does not replace filesystem IO or implement its own search/replace algorithm.

`edit` requires one exact match; missing or ambiguous old text produces native error context. `write` creates or overwrites a file. Both operate on the actual fixture files and preserve the supplied byte sequences through Rust's UTF-8 IO. Results retain the native `CallToolResult`, text content, error status and elapsed time. Full-file writes and exact replacements remain separate benchmark methods, and benchmark callers retain every attempt and verify filesystem bytes independently of the tool's success status.

## Verify the real stdio connection

The same binary can connect to xnip using the complete, unchanged official `GooseClient` and `McpClient`, including their actual lifecycle negotiation, tool listing, request metadata, timeouts and responses:

```json
{"action":"bridge","root":"/absolute/existing/project","command":"/absolute/path/to/xnip","calls":[{"name":"xnip_edit","arguments":{"file":"a.txt","old":"old","text":"new"}}]}
```

The default bridge profile uses Goose's ordinary `Auto` lifecycle. A separate diagnostic `"protocol_version":"2025-11-25"` profile uses the official explicit legacy initialization option. This diagnostic option is not presented as a saved stdio extension setting: ordinary `ExtensionConfig::Stdio` has no protocol-version field. Goose's `mcp-probe` script and programmatic `AgentConfig.mcp_protocol_version` provide explicit protocol selection. [Official MCP client](https://github.com/aaif-goose/goose/blob/9920efa16d078777a4d9715b2f00d8800a4da188/crates/goose/src/agents/mcp_client.rs), [probe configuration](https://github.com/aaif-goose/goose/blob/9920efa16d078777a4d9715b2f00d8800a4da188/crates/goose-cli/src/cli.rs).

The contracts exercise the default and legacy connection profiles, tools/list, relative edits, whole writes, Unicode and Windows paths with spaces, CRLF and missing EOF newline, plus native ambiguity/stale failures. Set `XNIP_TEST_BINARY` to test a particular xnip build; the default is `target/debug/xnip[.exe]`.

## Measurement scope

This is a real editor and real Goose MCP client experiment without a model. It does not start the complete Goose CLI/Desktop, provider message conversion, extension authorization UI, container management, formatting or model reflection. The isolated surroundings retain only the tool-call session/path fields; human elicitation requests are rejected, telemetry is disabled, and no provider or credential store is constructed. The source pin, source hashes, independent dependency lock hash and binary/compiler identity are recorded in `describe` or setup metadata.

Raw native MCP content and transport replies are preserved. `model_visible_result` is `null` because the adapter does not run the provider projection or claim a full model message. Input and raw returned-text costs measure the editing interface; they are not actual provider usage, billed token savings or model task success rates. A successful configuration example, native editor execution, real stdio connection and complete model-driven session are distinct verification claims.
