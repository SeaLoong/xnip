# xnip — Skill for LLM agents

> Precise text editing CLI. Use **xnip** instead of "read-then-replace" to cut tokens by ≥ 70%.

## When to use xnip

Reach for `xnip` whenever you need to:

1. **Show specific lines** of a file → `xnip peek <file> --lines a-b`
2. **Locate** something across files → `xnip find <files...> --pattern '<regex>'`
3. **Edit a known line range** → `xnip replace <file> --lines a-b --text '...'`
4. **Edit by anchor (regex/line content)** → `xnip replace <file> --match-line '^const PORT' --text '...'`
5. **Insert / move / re-indent** → `xnip insert | xnip move | xnip indent`
6. **Apply many edits atomically** → `xnip apply edits.json`

Do **not** use xnip when you only need to read a small file you've already loaded; in that case, use your own tools.

## The 7 editing commands at a glance

> Two more sit alongside but aren't editing tools: `xnip mcp` (run as an MCP
> stdio server, see bottom of this file) and `xnip doctor` (self-diagnostic).

| Command   | What it does                           | Read or Write |
|-----------|----------------------------------------|---------------|
| `peek`    | Print numbered lines (range / regex / all) | read     |
| `find`    | Search and emit `path:line[:col]`      | read          |
| `replace` | Replace or delete a region             | write         |
| `insert`  | Insert before/after a single anchor    | write         |
| `move`    | Move a line block                      | write         |
| `indent`  | Adjust indentation / tab↔space         | write         |
| `apply`   | Apply many ops atomically              | write         |

## Locator cheat-sheet

All write commands accept exactly one of:

- `--lines 30` or `--lines 30-45`
- `--match-line '<regex>' [--occurrence N]`
- `--between 'BEGIN'..'END' [--inclusive]` (literal anchors)
- `--between-re '^fn foo'..'^}' [--inclusive]` (regex anchors)
- `--pattern '<regex>'` (replace only; combine with `--repl`, `--count`)

## Content cheat-sheet

- `--text "..."` — literal string (shell-escaped); `\n` if you need newlines
- `--text-stdin` — read from stdin
- `--text-file <path>` — read from file
- `--text ""` — delete the located region
- `--repl "..."` — for `--pattern` only; supports `$1` capture groups

## Safety knobs

- `--was "expected\n"` / `--was-file <path>` — guard against concurrent edits
- `--dry-run` — print new content (or unified diff for `apply`) without writing
- `--check` — validate locator + `--was`, no output, exit 3 on failure
- `--backup` — write `<file>.bak` before atomic rename (default: off; rely on git)
- `--revert` — invert a `replace --pattern --repl` op (other ops not strictly invertible)

## apply: prefer this for batch edits

For 2+ edits to one or more files, write them once and apply atomically:

```sh
xnip apply edits.txt          # auto-detect format
xnip apply edits.json         # JSON manifest
xnip apply edits.yaml         # YAML manifest
xnip apply --from-stdin < ... # piped manifest
xnip apply edits.txt --dry-run  # preview unified diff
xnip apply edits.txt --check    # validate only, exit 3 on failure
xnip apply edits.txt --json     # emit NDJSON event stream on stdout
```

See [`apply-format.md`](./apply-format.md) for the full manifest schema.

## Exit codes

| Code | Meaning                                                      |
|------|--------------------------------------------------------------|
| 0    | Success                                                      |
| 1    | User error (bad args, locator not found, parse failure)      |
| 2    | IO error (cannot write tmpfile, permission denied)           |
| 3    | Validation failure (`--was` mismatch, `--check` problem)     |
| 4    | apply phase-2 partial commit (some files renamed before fail) |

## Heuristics for picking a locator

- You **know exact line numbers** → `--lines`
- You can describe the line by content → `--match-line`
- You're editing a **block bounded by markers** → `--between` / `--between-re`
- You're doing a **rename refactor** across the file → `--pattern`

## Recipes

```sh
# Show context around an error reported on line 42
xnip peek src/Foo.vue --lines 30-50

# Find a constant and rename it everywhere
xnip find --pattern '\bMAX_RETRIES\b' src/**/*.rs
xnip replace src/Foo.rs --pattern '\bMAX_RETRIES\b' --repl MAX_TRIES

# Replace a function body without trusting line numbers
xnip replace src/Foo.rs \
  --between-re '^pub fn old\(' '^}' --inclusive \
  --text-file ./snippets/new_old.rs

# Atomic multi-file edit with rollback safety
cat <<'EOF' | xnip apply --from-stdin
replace src/Foo.rs 30 "const PORT = 3000;"
replace src/Bar.rs =/^const PORT/ "const PORT = 3000;"
EOF
```

## MCP server (alternative integration)

If the host agent supports [Model Context Protocol](https://modelcontextprotocol.io/), prefer registering `xnip mcp` as an MCP server—you'll get 8 structured tools without quote/escape pitfalls:

```json
{
  "mcpServers": {
    "xnip": { "command": "xnip", "args": ["mcp"] }
  }
}
```

Tools: `xnip_peek` / `xnip_find` / `xnip_replace` / `xnip_insert` / `xnip_move` / `xnip_indent` / `xnip_apply` / `xnip_doctor`. Argument names mirror cli flags 1:1 (`--match-line` → `match_line`, etc.). MCP does **not** expose `--dry-run` / `--check` / `--revert` / `--json` (use `Err` / re-read patterns instead) and cannot consume process stdin (use `text_file` or `manifest_text`). See `docs/mcp.md`.
