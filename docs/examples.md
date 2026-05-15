# xnip — Examples

End-to-end recipes covering common editing scenarios.

## 1. Replace a known line

```sh
xnip replace src/config.rs --lines 30 --text 'const PORT: u16 = 3000;'
```

With safety check:

```sh
xnip replace src/config.rs --lines 30 \
  --text 'const PORT: u16 = 3000;' \
  --was 'const PORT: u16 = 8080;\n'
```

## 2. Delete a range

```sh
xnip replace src/foo.rs --lines 30-45 --text ''
```

## 3. Anchor-based edit (won't drift if line numbers change)

```sh
xnip replace src/Foo.vue \
  --match-line '^const PORT' \
  --text 'const PORT = 3000;'
```

## 4. Replace a function body

```sh
xnip replace src/lib.rs \
  --between-re '^pub fn old\(' '^}' --inclusive \
  --text-file ./snippets/new_old.rs
```

## 5. Cross-file rename (regex)

```sh
xnip replace src/Foo.rs \
  --pattern '\bMAX_RETRIES\b' \
  --repl 'MAX_TRIES'
```

Limit to first 2 matches:

```sh
xnip replace src/Foo.rs --pattern OLD --repl NEW --count 2
```

Revert:

```sh
xnip replace src/Foo.rs --pattern OLD --repl NEW --revert
```

## 6. Insert an import after the last `import` line

```sh
# uses --occurrence 99 with a fallback if your file has fewer imports
xnip insert src/main.ts \
  --match-line '^import ' \
  --occurrence 99 \
  --position after \
  --text "import { ref } from 'vue';"
```

## 7. Move a function block

```sh
xnip move src/lib.rs --from-lines 100-150 --to 30 --position before
```

## 8. Reindent

```sh
# convert tabs to 4 spaces in lines 1-99
xnip indent src/foo.go --lines 1-99 --tabs-to-spaces 4

# add 2 spaces to a block
xnip indent src/foo.go --lines 30-45 --add 2
```

## 9. Atomic batch with `apply` (native format)

`edits.txt`:

```
# rename API constant + adjust caller
replace src/lib.rs s/MAX_RETRIES/MAX_TRIES/g
replace src/main.rs =/^use crate/ "use crate::config::MAX_TRIES;"
insert src/main.rs 1 before "// Updated on 2026-01-01"
```

```sh
xnip apply edits.txt --dry-run     # preview as unified diff
xnip apply edits.txt --check       # validate, no write
xnip apply edits.txt               # commit
```

## 10. Atomic batch with `apply` (JSON, agent-friendly)

```json
[
  {"op": "replace", "file": "src/lib.rs", "pattern": "MAX_RETRIES", "repl": "MAX_TRIES"},
  {"op": "replace", "file": "src/main.rs",
   "match-line": "^use crate", "text": "use crate::config::MAX_TRIES;"},
  {"op": "insert", "file": "src/main.rs",
   "lines": 1, "where": "before", "text": "// Updated on 2026-01-01"}
]
```

```sh
xnip apply edits.json --json    # NDJSON event stream on stdout
```

## 11. Read a window for context

```sh
xnip peek src/lib.rs --lines 80-120
xnip peek src/lib.rs --match-line '^pub fn build_app' --context 5
xnip peek src/lib.rs --all --max-lines 200
```

## 12. Find before edit

```sh
xnip find --pattern '\bTODO\b' src/**/*.rs
xnip find --match-line '^use serde' src/main.rs
```

## 13. Self-diagnose

```sh
xnip doctor
```

Outputs version, OS/arch, target triple, cwd writability, TTY flags.
