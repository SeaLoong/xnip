# AGENTS.md — xnip section

> Drop this snippet into your project's `AGENTS.md` (or append to an existing one). Any agent that reads `AGENTS.md` will pick this up.

## Editing files

Prefer the `xnip` CLI for non-trivial file edits. It encodes "path + locator + new bytes" in a single command, saving ≥ 70% tokens versus reading the file and emitting a full replacement.

### Commands

- `xnip peek <file> --lines a-b` — print numbered lines
- `xnip find <files...> --pattern '<regex>'` — locate matches
- `xnip replace <file> [Locator] [Content]` — replace or delete a region
- `xnip insert <file> [Locator] --position before|after [Content]` — single-anchor insert
- `xnip move <file> --from-lines a-b --to N [--position before|after]` — move a block
- `xnip indent <file> [--lines a-b | --all] (--add N | --remove N | --tabs-to-spaces N | --spaces-to-tabs N)` — adjust indentation
- `xnip apply <manifest>` — apply many edits atomically

### Locators (write commands; pick exactly one)

- `--lines 30` / `--lines 30-45`
- `--match-line '<regex>' [--occurrence N]`
- `--between 'BEGIN'..'END' [--inclusive]`
- `--between-re '^fn foo'..'^}' [--inclusive]`
- `--pattern '<regex>'` (replace only)

### Content sources

- `--text "literal\n"` (literal; `\n` works)
- `--text-stdin` (from stdin)
- `--text-file <path>` (from file)
- `--text ""` to delete the located region
- `--repl "..."` for `--pattern` only; supports `$1`

### Safety

- `--was "expected\n"` / `--was-file <path>` to assert original content
- `--dry-run` previews; `--check` validates; both never modify files
- `--backup` writes `<file>.bak` before atomic rename (default: off)

### Atomic batch

For 2+ edits, use `xnip apply` with a manifest in native / JSON / YAML. Group by file, sort by line desc, two-phase commit. See `docs/apply-format.md`.

### Exit codes

`0` success · `1` user error · `2` IO · `3` validation · `4` apply phase-2 partial commit

### MCP server (alternative)

If the agent runs inside an MCP-aware client (Claude Desktop / Code, Cursor, Cline, Continue, Zed, ...), prefer registering `xnip mcp` as an MCP server: `{"mcpServers": {"xnip": {"command": "xnip", "args": ["mcp"]}}}`. 8 structured tools become available (`xnip_peek` / `xnip_find` / `xnip_replace` / `xnip_insert` / `xnip_move` / `xnip_indent` / `xnip_apply` / `xnip_doctor`) with the same field names as the cli flags. Note that MCP does not expose `--dry-run` / `--check` / `--revert` / `--json` and cannot consume process stdin (use `text_file` / `manifest_text` instead).
