# xnip apply — Manifest Format Reference

The `xnip apply` command accepts batch edit manifests in three equivalent formats: **native** (compact), **JSON**, or **YAML**. All three produce identical results with atomic commits.

## Native Format (.txt)

Compact, minimal overhead. One operation per line.

**Syntax:**
```
<op> <file> <locator> [content]
```

**Examples:**

```
# Replace line 42
replace /path/to/file 42 "new content"

# Replace lines 10-20
replace /path/to/file 10-20 "multiple\nlines"

# Replace by regex (first match)
replace /path/to/file --match-line '^const PORT' "const PORT = 8080;"

# Replace between markers
replace /path/to/file --between-re '^fn foo'..'^}' @/path/to/snippet.rs

# Global pattern replace
replace /path/to/file --pattern '\bOLD\b' --repl NEW

# Insert
insert /path/to/file --match-line 'export default' "export const helper = () => {};" --position after

# Move
move /path/to/file --from-lines 10-15 --to 30 --position after

# Indent
indent /path/to/file --lines 5-12 --add 2

# Delete (empty content)
replace /path/to/file 10-15 ""
```

**Content sources:**
- Literal: `"string"` (respects `\n`)
- From file: `@/path/to/file`
- From stdin: `@-`
- Delete: `""`

**Safety:**
- `--was "expected"` — Assert original content
- `--dry-run` — Preview
- `--check` — Validate without writing
- `--backup` — Write `.bak` file

## JSON Format (.json)

Structured, easier for programmatic generation.

**Syntax:**
```json
[
  {
    "op": "replace|insert|move|indent",
    "file": "/path/to/file",
    "locator": "<target>",      # see below
    "text": "new content",       # optional, content source
    "text_file": "/path",        # optional
    "text_stdin": true,          # optional
    "repl": "replacement",       # for pattern mode
    "position": "before|after",  # for insert/move
    "was": "expected",           # optional, assert
    "dry_run": false,            # optional
    "check": false,              # optional
    "backup": false              # optional
  }
]
```

**Locator objects** (pick one):
- `"lines": "42"` — Single line
- `"lines": "10-20"` — Range
- `"match_line": "^const PORT"` — Regex
- `"between": ["BEGIN", "END"]` — Literal markers
- `"between_re": ["^fn foo", "^}"]` — Regex markers
- `"pattern": "OLD_NAME"` — Regex (replace only)
- `"occurrence": 2` — (with `match_line`)

**Example:**
```json
[
  {
    "op": "replace",
    "file": "src/config.js",
    "lines": "42",
    "text": "const PORT = 3000;",
    "was": "const PORT = 5000;\n"
  },
  {
    "op": "insert",
    "file": "src/main.ts",
    "match_line": "export default",
    "text": "export const version = '1.0.0';",
    "position": "before"
  },
  {
    "op": "replace",
    "file": "src/utils.ts",
    "pattern": "\\bOLD_FUNC\\b",
    "repl": "NEW_FUNC"
  }
]
```

## YAML Format (.yaml)

Readable, close to intent. One operation per object.

**Syntax:**
```yaml
- op: replace|insert|move|indent
  file: /path/to/file
  lines: 42            # or "10-20"
  match_line: "^const" # or other locator
  text: new content
  text_file: /path
  text_stdin: true
  repl: replacement
  position: before|after
  was: expected        # assert original
  dry_run: false
  check: false
  backup: false
```

**Example:**
```yaml
- op: replace
  file: src/config.js
  lines: 42
  text: "const PORT = 3000;"
  was: "const PORT = 5000;\n"

- op: insert
  file: src/main.ts
  match_line: "^export default"
  text: "export const version = '1.0.0';"
  position: before

- op: replace
  file: src/utils.ts
  pattern: '\bOLD_FUNC\b'
  repl: NEW_FUNC
```

## Locator Reference

All write commands accept exactly one locator:

| Locator | CLI Flag | JSON Key | YAML Key | Example |
|---------|----------|----------|----------|---------|
| Line number | `--lines 42` | `"lines": "42"` | `lines: 42` | Single line |
| Line range | `--lines 10-20` | `"lines": "10-20"` | `lines: 10-20` | Multiple lines |
| Regex match | `--match-line '^const'` | `"match_line": "^const"` | `match_line: "^const"` | First match |
| Occurrence | `--occurrence 2` | `"occurrence": 2` | `occurrence: 2` | Nth match (with match_line) |
| Between literal | `--between 'BEGIN'..'END'` | `"between": ["BEGIN", "END"]` | `between: [BEGIN, END]` | Literal markers |
| Between regex | `--between-re '^fn'..'^}'` | `"between_re": ["^fn", "^}"]` | `between_re: ["^fn", "^}"]` | Regex markers |
| Inclusive | (use with between) | `"inclusive": true` | `inclusive: true` | Include markers |
| Pattern | `--pattern '\bOLD\b'` | `"pattern": "\bOLD\b"` | `pattern: '\bOLD\b'` | Replace mode only |

## Content Sources

| Source | CLI | JSON | YAML | Notes |
|--------|-----|------|------|-------|
| Literal | `--text 'abc'` | `"text": "abc"` | `text: abc` | Respects `\n` |
| File | `--text-file path` | `"text_file": "path"` | `text_file: path` | Relative to cwd |
| Stdin | `--text-stdin` | `"text_stdin": true` | `text_stdin: true` | Sequential consumption |
| Delete | `--text ''` | `"text": ""` | `text: ""` | Empty content |
| Replace | `--repl 'NEW'` | `"repl": "NEW"` | `repl: NEW` | With `--pattern` only; supports `$1` |

## Operation Types

### replace
Replace or delete a region.
```json
{"op": "replace", "file": "...", "lines": "42", "text": "new content"}
```

### insert
Insert before or after an anchor.
```json
{"op": "insert", "file": "...", "match_line": "anchor", "text": "new", "position": "after"}
```

### move
Move a block to a new location.
```json
{"op": "move", "file": "...", "from_lines": "10-15", "to_line": 30, "position": "after"}
```

### indent
Adjust indentation or convert tabs ↔ spaces.
```json
{"op": "indent", "file": "...", "lines": "5-12", "add": 2}
```

For `indent`, use one of:
- `"add": N` — Indent by N
- `"remove": N` — Dedent by N
- `"tabs_to_spaces": N` — Convert tabs to N spaces
- `"spaces_to_tabs": N` — Convert N spaces to tab

## Safety & Validation

### Assert Original Content
Prevents applying edits if the file has changed:
```json
{"op": "replace", "file": "...", "lines": "42", "text": "new", "was": "old\n"}
```

### Dry-run (Preview)
Preview changes without writing:
```
xnip apply edits.json --dry-run
```

### Check (Validate)
Validate manifest without modifying files:
```
xnip apply edits.json --check
```

### Backup
Write `.bak` file before atomic rename:
```json
{"op": "replace", "file": "...", "lines": "42", "text": "new", "backup": true}
```

## Execution Semantics

1. **Grouping by file**: Edits for the same file are collected
2. **Sort by line (descending)**: Prevents line-number drift
3. **Phase 1**: Write to temp files in same directory
4. **Validation**: Verify all writes succeeded
5. **Phase 2**: Atomic rename (tmpfile → target)
6. **Rollback**: If any phase-2 rename fails, partial state may remain (exit code 4)

**All-or-nothing guarantee**: Either all edits succeed or none do (per file).

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | All edits applied successfully |
| `1` | User error (bad manifest, missing file, bad locator) |
| `2` | IO error (permission denied, tmpfile creation failed) |
| `3` | Validation failed (`--was` mismatch, `--check` rejected) |
| `4` | Partial commit (phase-2 rename incomplete) |

## Tips

- **Preserve order:** List edits per file in descending line order to avoid conflicts
- **Use `--was`** to catch unintended drift
- **Always `--dry-run` first** for complex batches
- **Three formats are equivalent:** Pick based on readability or generation convenience
- **Stdin content is stateful:** Edits consuming `@-` pull sequentially

See [SKILL.md](../SKILL.md) for command-line examples.
