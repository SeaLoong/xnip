# xnip — Copilot prompt

> Drop this prompt into `.github/copilot-instructions.md` (workspace) or a custom prompt to teach Copilot to use xnip.

When editing files, prefer the `xnip` CLI. It encodes "path + locator + new bytes" in a single command, saving ≥ 70% tokens versus reading the file and emitting a full replacement.

## Decision rules

| Situation | Use |
|-----------|-----|
| Show numbered lines | `xnip peek <file> --lines a-b` |
| Search for a token | `xnip find <files...> --pattern '<regex>'` |
| Replace a known line range | `xnip replace <file> --lines a-b --text '...'` |
| Edit by anchor | `xnip replace <file> --match-line '^pat' --text '...'` |
| Edit a block between markers | `xnip replace <file> --between-re '^fn foo' '^}' --inclusive --text-file snippet.txt` |
| Rename across a file | `xnip replace <file> --pattern '\bOLD\b' --repl NEW` |
| Insert / move / re-indent | `xnip insert | xnip move | xnip indent` |
| Many edits atomically | `xnip apply edits.txt` (or `.json` / `.yaml`) |

## Always pair with safety

- `--was 'expected\n'` to detect drift before writing
- `--dry-run` to preview new content (or unified diff for `apply`)
- `--check` to validate without writing

## Exit codes

- `0` success
- `1` user error (bad args / locator not found)
- `2` IO error
- `3` `--was` / `--check` failed
- `4` `apply` phase-2 partial commit

## apply quick-ref

```sh
xnip apply edits.txt          # native
xnip apply edits.json         # JSON
xnip apply edits.yaml         # YAML
xnip apply --from-stdin
xnip apply edits.txt --dry-run
xnip apply edits.txt --check
xnip apply edits.txt --json   # NDJSON event stream
```

## MCP server (alternative)

GitHub Copilot Chat (VS Code) and the Copilot CLI shell out to commands directly, so this prompt is enough. If you also use an MCP-aware editor on the same project (Cursor / Claude Desktop / Cline / Continue / Zed), you can additionally register `xnip mcp` there: `{"mcpServers": {"xnip": {"command": "xnip", "args": ["mcp"]}}}`. The MCP server exposes 8 structured tools (`xnip_peek` / `xnip_find` / `xnip_replace` / `xnip_insert` / `xnip_move` / `xnip_indent` / `xnip_apply` / `xnip_doctor`) backed by the same code as the cli.
