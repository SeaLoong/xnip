# `xnip apply` manifest format reference

`xnip apply` accepts three equivalent formats. You can pick whichever your toolchain emits more cheaply.

The format is auto-detected by:

1. `--format <native|json|yaml>` (explicit)
2. File extension (`.json`/`.json5` → JSON, `.yaml`/`.yml` → YAML, otherwise native)
3. Fallback chain JSON → YAML → native if extension hint fails

stdin defaults to **native**.

---

## 1. Native (recommended; smallest token footprint)

One operation per line. `#` starts a comment. Blank lines are skipped.

```
<op> <file | --files-from path> <locator> [<modifier>...] [<content>] [<named-modifier>...]
```

### Tokens

- Quoted strings `"..."` may contain spaces; C-style escapes inside: `\n` `\t` `\r` `\\` `\"`
- Unquoted tokens are split by whitespace
- `@<path>` reads content from a file
- `@-` reads from apply's stdin (sequentially per `@-` token)
- `@@` is a literal `@`
- `""` is the empty string (delete)

### Locators

| Syntax                      | Meaning                                       |
|-----------------------------|-----------------------------------------------|
| `30`                        | line 30                                       |
| `30-45`                     | lines 30..=45                                 |
| `=/regex/N?`                | match-line; optional Nth occurrence           |
| `"START".."END"[i]`         | between two literal anchors; `i` = inclusive  |
| `~/A/..~/B/i?`              | between two regex anchors; `/i` = inclusive   |
| `s/PAT/REPL/g`              | substitute all matches (also `s/PAT/REPL/3`)  |

### Modifiers

- `before` / `after` (insert/move position)
- `revert`
- `+N` / `-N` (indent: add/remove N spaces)
- `t2s:N` (tabs → N spaces) / `s2t:N` (N spaces → tab)
- Named: `was="expected\n"`, `was=@<path>`

### Content

- `"literal text"` — applies as bytes
- `@./snippets/x.txt` — read from file (relative to manifest dir)
- `@-` — consume from apply's stdin
- `""` — empty (delete)

### Examples

```
# replace one line
replace src/Foo.vue 30 "const X = 1;"

# delete lines 30-45
replace src/Foo.vue 30-45 ""

# replace by regex anchor
replace src/Foo.vue =/^const PORT/ "const PORT = 3000;" was="const PORT = 8080;\n"

# replace a regex-anchored block (inclusive)
replace src/Foo.vue ~/^function foo/..~/^}/i ""

# rename across files via subst
replace --files-from filelist.txt s/OLD_NAME/NEW_NAME/g

# read snippet from file
replace src/Foo.vue 30-45 @./snippets/new-foo.txt

# insert + move + indent
insert src/Foo.vue 5 after "import X from 'x';"
insert src/Foo.vue =/^import vue/ after "import { ref } from 'vue';"
move   src/Foo.vue 10-20 100
indent src/Foo.vue 30-45 +2
indent src/Foo.vue 1-99  t2s:4

# revert (forward then back; only --pattern is strictly invertible)
replace src/Foo.vue revert s/OLD/NEW/g
```

---

## 2. JSON

Top-level array of op objects. Field names match CLI flags (kebab-case).

```json
[
  {"op": "replace", "file": "src/Foo.vue", "lines": "30", "text": "const X = 1;"},
  {"op": "replace", "file": "src/Foo.vue", "lines": "30-45", "text": ""},
  {"op": "replace", "file": "src/Foo.vue",
   "match-line": "^const PORT",
   "text": "const PORT = 3000;",
   "was": "const PORT = 8080;\n"},
  {"op": "replace", "file": "src/Foo.vue",
   "between": ["// BEGIN", "// END"], "inclusive": false, "text": ""},
  {"op": "replace", "files-from": "filelist.txt",
   "pattern": "OLD_NAME", "repl": "NEW_NAME", "count": "all"},
  {"op": "insert", "file": "src/Foo.vue",
   "lines": 5, "where": "after", "text": "import X from 'x';"},
  {"op": "move", "file": "src/Foo.vue", "lines": "10-20", "to": 100},
  {"op": "indent", "file": "src/Foo.vue", "lines": "30-45", "by": 2},
  {"op": "indent", "file": "src/Foo.vue", "lines": "1-99", "tabs-to-spaces": 4},
  {"op": "replace", "file": "src/Foo.vue",
   "pattern": "OLD", "repl": "NEW", "revert": true}
]
```

Field semantics (kebab-case):

- target: `file` or `files-from`
- locator: `lines` (string `"a"` or `"a-b"`, or integer) / `match-line` / `between` (2-element array of strings) / `between-re` / `pattern`
- content: `text` / `text-file` / `repl`
- modifiers: `where` (`before`/`after`), `inclusive`, `occurrence`, `count` (`"all"` or integer), `by`, `tabs-to-spaces`, `spaces-to-tabs`, `revert`, `was` / `was-file`

---

## 3. YAML

Same schema as JSON. Multi-line text with `|` block scalars is more readable:

```yaml
- op: replace
  file: src/Foo.vue
  lines: "30-45"
  text: |
    function foo() {
      return 42;
    }

- op: insert
  file: src/Foo.vue
  match-line: "^import vue"
  where: after
  text: "import { ref } from 'vue';"
```

---

## Execution semantics

1. Parse → unified op list
2. Group by file
3. Within a file: order ops by **start line descending** (so later ops don't shift earlier ones)
4. **Phase 1** — for each file: read → apply ops → write a temp file in the same directory
5. **Phase 2** — atomic-rename each temp file; if `--backup`, save `<file>.bak` first
6. On phase-1 failure: nothing on disk changes; exit 3
7. On phase-2 failure: with `--backup`, restore committed files; without, list affected files on stderr; exit 4

Modes:

- `--check` — phase 1 only; prints `OK`
- `--dry-run` — phase 1 + unified diff to stdout
- `--from-stdin [--format ...]` — manifest from stdin
- `--backup` — write `.bak` (default: off)
- `--json` — emit NDJSON events on stdout
- `--parallel <N>` — run phase 1 across N worker threads (rayon). Phase 2 stays serial
  to keep commit-order-dependent rollback well-defined. `0` or `1` = single-threaded.
- `--stdin-file <PATH>` — supply the bytes that op-level `@-` consumes from a file
  instead of reading process stdin. Required when `--from-stdin` is used together with
  `@-`, since stdin is already consumed by the manifest.

### `@-` (op-level stdin) constraints

- A manifest may reference `@-` **at most once**. Stdin is a linear byte stream with no
  unambiguous splitter; multiple `@-` tokens fail phase 1 with exit 3.
- If no op uses `@-`, stdin is left untouched (xnip will not consume an unrelated pipe).
- If `@-` appears and `--stdin-file` is not given, the entire process stdin is read once
  and used as the payload for that single `@-` reference.

### `--revert` semantics

- `replace --pattern --repl --revert` → swaps `pattern` and `repl` (regex-escaped).
- `replace --lines a-b --text X --was Y --revert` → swaps `text` and `was` with a
  pre-condition check that the current content equals `X`. Anchor-based locators
  (`--match-line`, `--between`, `--between-re`) are explicitly **rejected** for
  range-revert because the anchor may no longer exist after forward execution.
- `insert --lines A --text X --revert` → deletes the inserted block with a
  pre-condition check.
- `move --from-lines S-E --to T --position P --revert` → computed via
  `reverse_params`; **only `--from-lines` is accepted** (`--from-match-line` cannot
  be inverted because it resolves a different block in the post-forward file).
- `indent --add/--remove/--tabs-to-spaces/--spaces-to-tabs --revert` → swaps the
  operator. `Remove`/`SpacesToTabs` are *not strictly* invertible (information may
  be lost), so revert may differ from the original bytes.
