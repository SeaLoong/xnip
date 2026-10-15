---
name: xnip
description: Precise text editing CLI for LLM agents. Use it instead of "read-then-replace" to reduce repeated context and replacement text. Provides 7 editing commands (peek, find, replace, insert, move, indent, apply) with anchor-based locators and atomic writes, plus an MCP stdio server (`xnip mcp`) exposing the same operations as 8 structured tools.
---

# xnip — Skill

> Precise text editing CLI. Use **xnip** instead of "read-then-replace" to reduce repeated context and replacement text.

## When to use xnip

Reach for `xnip` whenever you need to:

1. **Show specific lines** of a file → `xnip peek <file> --lines a-b`
2. **Locate** something across files → `xnip find <files...> --pattern '<regex>'`
3. **Edit a known line range** → `xnip replace <file> --lines a-b --text '...'`
4. **Edit by anchor (regex/line content)** → `xnip replace <file> --match-line '^const PORT' --text '...'`
5. **Insert / move / re-indent** → `xnip insert | xnip move | xnip indent`
6. **Apply many edits after validation** → `xnip apply edits.json`

Do **not** use xnip when you only need to read a small file you've already loaded.

## Commands

| Command   | What it does                           | Read or Write |
|-----------|----------------------------------------|---------------|
| `peek`    | Print numbered lines (range / regex / all) | read     |
| `find`    | Search and emit `path:line[:col]`      | read          |
| `replace` | Replace or delete a region             | write         |
| `insert`  | Insert before/after a single anchor    | write         |
| `move`    | Move a line block                      | write         |
| `indent`  | Adjust indentation / tab↔space         | write         |
| `apply`   | Validate and apply many ops              | write         |

## Locators for replace and insert

- `--lines 30` or `--lines 30-45`
- `--match-line '<regex>' [--occurrence N]`
- `--between 'BEGIN'..'END' [--inclusive]`
- `--between-re '^fn foo'..'^}' [--inclusive]`
- `--pattern '<regex>'` (replace only)

## Content sources

- `--text "..."` — literal shell argument (use a file or stdin for portable newlines)
- `--text-stdin` — from stdin
- `--text-file <path>` — from file
- `--text ""` — delete the located region
- `--repl "..."` — for `--pattern` only; supports `$1`

## Safety knobs

- `--was-file expected.txt` (or `--was` with actual expected bytes) — guard against drift
- `--dry-run` — preview new content (or unified diff for apply)
- `--check` — validate without writing; inspect the documented exit code
- `--backup` — write `<file>.bak` before atomic rename (default off)

## apply: prefer this for batch edits

```sh
xnip apply edits.txt          # auto-detect format
xnip apply edits.json         # JSON manifest
xnip apply edits.yaml         # YAML manifest
xnip apply --from-stdin < ... # piped manifest
xnip apply edits.txt --dry-run
xnip apply edits.txt --check
xnip apply edits.txt --json   # NDJSON event stream
```

## Exit codes

- `0` success
- `1` user error (bad args / locator not found)
- `2` IO error
- `3` validation failure (`--was` / `--check`)
- `4` apply phase-2 partial commit

## Recipes

```sh
# show a window for context
xnip peek src/foo.rs --lines 30-50

# rename across a file
xnip replace src/foo.rs --pattern '\bMAX_RETRIES\b' --repl MAX_TRIES

# replace a function body without trusting line numbers
xnip replace src/foo.rs \
  --between-re '^pub fn old\('..'^}' --inclusive \
  --text-file ./snippets/new_old.rs

# validated multi-file edit
cat <<'EOF' | xnip apply --from-stdin
replace src/foo.rs 30 "const PORT = 3000;"
replace src/bar.rs =/^const PORT/ "const PORT = 3000;"
EOF
```

## MCP server (preferred when available)

If you are running inside Claude Desktop / Claude Code (or any MCP-aware client), prefer the `xnip mcp` server over shelling out: tool calls receive structured JSON arguments, eliminating quote/escape pitfalls.

Register in `~/Library/Application Support/Claude/claude_desktop_config.json` (Claude Desktop) or project `.mcp.json` (Claude Code):

```json
{
  "mcpServers": {
    "xnip": { "command": "xnip", "args": ["mcp"] }
  }
}
```

8 tools become available: `xnip_peek` / `xnip_find` / `xnip_replace` / `xnip_insert` / `xnip_move` / `xnip_indent` / `xnip_apply` / `xnip_doctor`. Their argument names match the cli flags 1:1 (e.g. cli `--match-line` → MCP `match_line`).

Differences from cli (intentional): no `--dry-run` / `--check` / `--revert` / `--json` (use `Err` semantics or read the file back); no `--text-stdin` / `apply --from-stdin` (stdin is occupied by the MCP protocol—use `text_file` or `manifest_text` instead). `was` / `was_file` are available for line-based replacement; `backup` is available on write tools.

Batch commits are atomic per file. There is no automatic rollback after a phase-2 failure; inspect the committed paths and restore from Git or backups before retrying. Use disjoint line ranges, do not mix pattern and line edits on one file, and run moves separately from other edits to that file. `was` is supported for line-based replacement only and does not lock out concurrent writers.
