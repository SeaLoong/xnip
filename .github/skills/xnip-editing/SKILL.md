---
name: xnip-editing
description: 'Precise text editing via CLI — Use **xnip** for file edits instead of read-then-replace workflows. Supports 7 commands (peek/find/replace/insert/move/indent/apply), batch atomic commits, dry-run validation, and ≥ 70% token savings. Best for: editing specific line ranges, multi-file refactors, batch edits, regex-based replacements, and safe atomic operations with preview.'
argument-hint: 'Describe the file editing task: what file(s), what operation (find/replace/insert/move), and any constraints'
---

# xnip — Precise Text Editing for Agents

**xnip** is a compact, fast CLI for precise file editing. Instead of the traditional "read entire file → generate replacement → write back" workflow, xnip encodes edits atomically as **path + locator + new content** in a single command, reducing token overhead by **≥ 70%**.

## When to Use xnip

✓ **Preferred:** Specific line ranges, regex-based finds, batch multi-file edits, atomic operations with safety checks  
✗ **Skip:** Reading small files you've already loaded; use regular file tools instead

| Task | Best xnip Command | Reason |
|------|-------------------|--------|
| Show lines 30-45 in a file | `peek --lines 30-45` | No tokens wasted on full file |
| Find all occurrences of a symbol | `find --pattern '<regex>'` | Read-only, precise output |
| Replace line 42 | `replace --lines 42 --text '...'` | Atomic, can preview with `--dry-run` |
| Replace by anchor (line content/regex) | `replace --match-line '^const PORT'` | Survives minor formatting changes |
| Replace in a code block | `replace --between-re '^fn foo'..'^}'` | No need to count lines |
| Global find-replace (e.g., rename) | `replace --pattern 'OLD_NAME' --repl NEW_NAME` | Atomic rename across a file |
| Insert before/after a marker | `insert --match-line 'export default' --position before` | Semantic insert, survives diffs |
| Move a block of lines | `move --from-lines 10-15 --to 30` | Atomic block move |
| Fix indentation | `indent --lines 5-12 --add 2` | Adjust tabs ↔ spaces |
| Apply 10+ edits together | `apply edits.json` | Single atomic commit, all-or-nothing |

## The 7 Core Commands

### 1. peek — Print numbered lines
```bash
xnip peek <file> [OPTIONS]

# Show specific range
xnip peek src/Foo.vue --lines 30-45

# Show by regex (first match)
xnip peek src/Foo.vue --pattern '^export'

# Show all lines (numbered)
xnip peek src/Foo.vue --all
```

### 2. find — Locate matches across files
```bash
xnip find <files|--files-from list.txt> --pattern '<regex>'

# Find in single file
xnip find src/main.rs --pattern '\bfn foo\b'

# Find in multiple files
xnip find src/**/*.ts --pattern 'TODO|FIXME'

# Find with occurrence count
xnip find . --pattern '^import' --count
```

### 3. replace — Edit or delete a region
```bash
xnip replace <file> [LOCATOR] --text '...'

# By line number(s)
xnip replace src/config.js --lines 42 --text 'const X = 1;'
xnip replace src/config.js --lines 10-20 --text 'new code'

# By line content (regex)
xnip replace src/config.js --match-line '^const PORT' --text 'const PORT = 3000;'

# Between markers (regex or literal)
xnip replace src/Foo.vue --between-re '^<script'..'^</script>' --text-file new-script.js

# Pattern-based (find & replace all)
xnip replace src/main.rs --pattern '\bOLD_NAME\b' --repl NEW_NAME

# Delete a region
xnip replace src/config.js --lines 10-15 --text ''
```

### 4. insert — Add content before/after an anchor
```bash
xnip insert <file> --match-line '<anchor>' --position before|after --text '...'

# Insert after 'export default'
xnip insert src/App.tsx --match-line 'export default' --position after --text 'export const helper = () => {};'

# Insert before a line matching regex
xnip insert src/main.rs --match-line '^fn main' --position before --text '// Helper\n'
```

### 5. move — Relocate a block of lines
```bash
xnip move <file> --from-lines a-b --to N [--position before|after]

# Move lines 10-15 to after line 30
xnip move src/main.rs --from-lines 10-15 --to 30 --position after
```

### 6. indent — Adjust indentation or convert tabs ↔ spaces
```bash
xnip indent <file> [RANGE] <TRANSFORMATION>

# Indent by 2 spaces
xnip indent src/app.js --lines 5-12 --add 2

# Dedent by 1 level
xnip indent src/app.js --lines 5-12 --remove 1

# Convert tabs to spaces (4 spaces per tab)
xnip indent src/app.js --all --tabs-to-spaces 4

# Convert spaces to tabs
xnip indent src/app.js --all --spaces-to-tabs 4
```

### 7. apply — Batch atomic edits
```bash
xnip apply <manifest> [OPTIONS]

# Native format (.txt)
xnip apply edits.txt

# JSON format (.json)
xnip apply edits.json

# YAML format (.yaml or .yml)
xnip apply edits.yaml

# Preview before writing
xnip apply edits.json --dry-run

# Validate without modifying
xnip apply edits.json --check

# Machine-readable event stream
xnip apply edits.json --json
```

**Manifest format:** See [apply-format reference](./references/apply-format.md) for native/JSON/YAML syntax.

## Safety & Validation

### Preview Changes
```bash
# See what --dry-run would change (unified diff)
xnip replace src/main.rs --lines 10-15 --text 'new' --dry-run
```

### Validate Without Writing
```bash
# Check without modifying files
xnip apply edits.json --check
```

### Assert Original Content (Drift Detection)
```bash
# Fail if content has changed
xnip replace src/config.js --lines 42 --text 'new' --was 'old\n'

# Read expected content from file
xnip replace src/config.js --lines 42 --text 'new' --was-file expected.txt
```

### Backup Before Write
```bash
# Writes .bak file before atomic rename (off by default)
xnip replace src/config.js --lines 42 --text 'new' --backup
```

## Content Sources

- **Literal string:** `--text 'literal\n'` (supports `\n` escapes)
- **From stdin:** `--text-stdin` (pipe content via `|`)
- **From file:** `--text-file path/to/file.txt`
- **Delete:** `--text ''` (empty string)
- **Regex replacement:** `--repl '...'` (with `$1` capture groups for `--pattern`)

## Locators (Pick One for Write Commands)

| Locator | Syntax | Use Case |
|---------|--------|----------|
| Line number(s) | `--lines 42` or `--lines 10-20` | When you know exact line numbers |
| Line content | `--match-line '<regex>'` | Semantic anchor (survives formatting changes) |
| Code block | `--between-re '^fn foo'..'^}'` | Target a function/class/block |
| Regex pattern | `--pattern '<regex>'` | Replace/find mode; supports `$1` in `--repl` |
| Occurrence | `--occurrence 2` (with `--match-line`) | Target the 2nd occurrence of a pattern |

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | User error (bad args, locator not found, format error) |
| `2` | IO error (permission denied, file not found) |
| `3` | Validation failed (`--was` mismatch, `--check` rejected) |
| `4` | Partial commit (apply phase-2 incomplete) |

## Procedure: File Editing Workflow

### Step 1: Locate the Target
```bash
# Use peek to see context
xnip peek src/main.rs --lines 1-50

# Or find across files
xnip find src --pattern 'function to rename'
```

### Step 2: Preview the Change
```bash
# Use --dry-run to see unified diff
xnip replace src/main.rs --lines 10-15 --text 'new content' --dry-run
```

### Step 3: Apply with Safety
```bash
# Simple single-file edit
xnip replace src/main.rs --lines 10-15 --text 'new content'

# Or batch multiple edits
xnip apply edits.json
```

### Step 4: Verify Success
- Check exit code (`echo $?` or `$LASTEXITCODE`)
- If using `--dry-run`, review output before running without it
- If using `apply --check`, validate logic before actual write

## Advanced: Batch Editing with apply

For 2+ edits to one or more files, use `xnip apply`:

**Native format (edits.txt):**
```
replace /path/to/file 10 "new line"
insert /path/to/file --match-line 'anchor' "new text"
replace /path/to/another 42-50 @/path/to/snippet.txt
```

**JSON format (edits.json):**
```json
[
  {"op": "replace", "file": "/path/to/file", "lines": "10", "text": "new line"},
  {"op": "insert", "file": "/path/to/file", "match_line": "anchor", "text": "new text", "position": "after"},
  {"op": "replace", "file": "/path/to/another", "lines": "42-50", "text_file": "/path/to/snippet.txt"}
]
```

**YAML format (edits.yaml):**
```yaml
- op: replace
  file: /path/to/file
  lines: 10
  text: new line
- op: insert
  file: /path/to/file
  match_line: anchor
  text: new text
  position: after
```

All three formats produce **identical results** and use **atomic commits** (all-or-nothing).

## Integration with Agents

### In System Prompts / AGENTS.md
```markdown
When editing files, prefer xnip:
- xnip peek <file> --lines a-b
- xnip find --pattern '<regex>'
- xnip replace <file> [LOCATOR] --text '...'
- xnip insert | xnip move | xnip indent
- xnip apply <manifest> (for batch edits)
```

### Via MCP (Claude Desktop, Cursor, Cline, etc.)
Configure `xnip mcp` as an MCP server for structured tools:
```json
{"mcpServers": {"xnip": {"command": "xnip", "args": ["mcp"]}}}
```

8 tools become available:
- `xnip_peek` / `xnip_find` / `xnip_replace` / `xnip_insert`
- `xnip_move` / `xnip_indent` / `xnip_apply` / `xnip_doctor`

(Note: MCP doesn't expose `--dry-run`, `--check`, `--revert`, or stdin; use `text_file` instead)

## References

- [apply-format](./references/apply-format.md) — Detailed native/JSON/YAML manifest syntax
- [examples](./references/examples.md) — Real-world edit scenarios
- [error-codes](./references/error-codes.md) — Troubleshooting guide
- [xnip on GitHub](https://github.com/SeaLoong/xnip)

## Quick Comparison: xnip vs Manual Edit

| Scenario | Manual Read-Replace | xnip |
|----------|-------------------|------|
| Edit line 42 of a 1000-line file | Send full file (~50KB) | Send 1-line command (~200B) |
| Token cost | ~2000 tokens | ~50 tokens |
| Atomic safety | Must trust the LLM's replacement | Validated, atomic commit |
| Drift-resistant | No | Yes (`--was` option) |
| Batch edits | Multiple round-trips | Single call |

---

**Next steps:** Try `/xnip-editing` in chat to invoke this skill, or configure `xnip mcp` in your MCP-aware editor for structured tool access.
