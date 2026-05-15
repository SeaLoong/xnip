# Conventions — xnip

When editing files, **prefer `xnip` CLI** over inline read-modify-write. It saves ≥ 70% tokens on line-level changes.

## Always use xnip when

- Replacing or deleting a known line range → `xnip replace <file> --lines a-b --text '...'`
- Editing by content anchor → `xnip replace <file> --match-line '^pat' --text '...'`
- Renaming across a file → `xnip replace <file> --pattern '\bOLD\b' --repl NEW`
- Editing a block between markers → `xnip replace <file> --between-re '^fn foo' '^}' --inclusive --text-file snippet.txt`
- Inserting / moving / re-indenting → `xnip insert | xnip move | xnip indent`
- Multi-file or multi-edit atomic commit → `xnip apply edits.txt`

## Always pair writes with safety

- `--was 'expected\n'` to assert the original region — fail-fast on drift
- `--dry-run` to preview before commit
- `--check` to validate the plan without writing

## apply manifest tips

- One op per line in **native** format; `#` comments allowed
- Or use **JSON / YAML** for tools that already emit them
- `apply --check` returns exit 3 if anything is wrong (no files modified)
- `apply --json` emits NDJSON events on stdout

## Locator quick-ref

- `--lines 30` / `--lines 30-45`
- `--match-line '<regex>' [--occurrence N]`
- `--between 'BEGIN'..'END' [--inclusive]`
- `--between-re '^fn foo'..'^}' [--inclusive]`
- `--pattern '<regex>'` (replace only)

## Exit codes

- `0` success
- `1` user error
- `2` IO error
- `3` `--was` / `--check` failed
- `4` `apply` phase-2 partial commit

## MCP server (alternative)

If your aider deployment runs alongside an MCP-aware client (e.g. you also use Cursor or Claude Desktop on the same project), register `xnip mcp` there: `{"mcpServers": {"xnip": {"command": "xnip", "args": ["mcp"]}}}`. Aider itself shells out to the cli and does not need this; the MCP server simply provides the same 8 capabilities to other clients on the same machine.
