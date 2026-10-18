# `xnip apply` manifest format reference

`xnip apply` accepts compact JSON file groups, native/JSON/YAML operation lists and text patches. Use file groups for independent literal edits or complete-file writes; use operation lists for cross-file lifecycle dependencies and line operations.

The format is auto-detected by:

1. `--format <compact|native|json|yaml|patch>` (explicit)
2. File extension (`.json`/`.json5` → JSON, `.yaml`/`.yml` → YAML, otherwise native)
3. Try the preferred parser, then the remaining native/JSON/YAML/patch parsers. Compact objects use the JSON parser; `--format compact` restricts input to the file-map shape.

Recognized unified/apply_patch headers and `.patch`/`.diff` files prefer the patch parser. Without a recognized extension, leading `[` or `{` (after whitespace) prefers JSON; other input prefers native. Each parser is attempted at most once. Use `--format` to restrict parsing and get format-specific validation errors. A `.json5` suffix is only a JSON hint; JSON5 syntax is not supported.

---

## Compact file groups

A JSON object maps each target path to one of these values:

```json
{
  "src/a.rs": [["old_name", "new_name"], ["new_name()", "new_name(42)"]],
  "generated.txt": "complete file content\n",
  "config.txt": {"text": "PORT=3000", "old": "PORT=8080"}
}
```

An array contains ordered `[old,text]` pairs for that file. Every old block must be nonempty and unique in the virtual content produced by preceding pairs; empty replacement text removes a block. A string explicitly writes the complete file, creating it if missing. `{text,old?}` provides the same whole-file write with an optional complete-content guard; `old: ""` requires an existing empty file, never a missing file. Whole writes reject final symlinks and directories.

The map must contain at least one file, and pair arrays must be nonempty. Raw compact JSON parsing rejects duplicate keys. Normalized path aliases such as `a.txt` / `./a.txt` are rejected in every file map. Unknown fields in whole-write objects are rejected. Separate map entries are independent: do not use map-key order to express cross-file dependencies. All groups validate before a shared transaction commits the resulting per-file state.

Save the object as `changes.json` and run `xnip apply changes.json --check`, then `xnip apply changes.json`. Both `--format compact` and JSON parsing of a top-level object accept this syntax; automatic format detection also reaches that JSON parser. YAML operation lists use the schema below, rather than compact file-map syntax.

MCP carries the object directly as `xnip_apply({files:{...}})`; clients must use unique JSON keys because the protocol decoder collapses repeated raw keys before tool validation. Use `manifest_text` with `format:"compact"` to validate raw JSON key uniqueness. `files` is mutually exclusive with `edits`, `path`, `manifest_text`, `format` and `patch_root`. The existing direct `edits` array remains available for explicit global replacements with `all:true`.

## Native manifests

One operation per record; a quoted payload can span physical lines. Lines beginning with `#` after optional whitespace are comments; avoid trailing inline comments. Blank lines are skipped.

```
<op> <file | --files-from path> <locator> [<modifier>...] [<content>] [<named-modifier>...]
```

### Tokens

- Quoted strings `"..."` may contain spaces and physical LF/CRLF line breaks; C-style escapes inside: `\n` `\t` `\r` `\\` `\"`. Quoted payload bytes, including EOF, are preserved.
- Unquoted tokens are split by spaces/tabs and record line endings; an internal bare CR remains a literal byte
- `@<path>` reads content from a file
- `@-` reads from apply's stdin (at most once per manifest)
- `@@` is a literal `@`
- `""` is an empty payload; its effect depends on the operation (block deletion or an empty whole file).

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
- Named: `was="expected\n"`; legacy line operations also accept `was=@<path>`. Native whole-file write treats the guard as literal bytes.

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

# limited literal reversal; use backups for exact recovery
replace src/Foo.vue revert s/OLD/NEW/g
```

---

## JSON operation lists

Top-level arrays contain op objects. A top-level object instead uses compact file groups above. Field names match CLI flags (kebab-case). The objects below illustrate independent operations; do not combine these overlapping examples into one manifest for the same file.

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

## YAML operation lists

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
3. Legacy grouped operations resolve against the original file and run in **start line descending** order. Ordered file operations, compact pairs and `--sequential` instead use the current virtual content.
4. **Phase 1** — read targets and validate the complete edit plan in memory; preview/check stop here without creating temporary files or directories
5. **Phase 2** — persist recovery snapshots, optionally save `<file>.bak`, then stage and commit each changed target
6. On phase-1 failure: targets remain unchanged; exit 3
7. On commit failure, attempt rollback and return exit 4 with the journal path and rollback status. Writes are atomic per file; see [recovery](recovery.md).

Modes:

- `--check` — validates without writing; the compact check receipt goes to stderr
- `--dry-run` — validates without writing; a bounded unified diff goes to stdout
- `--from-stdin [--format ...]` — manifest from stdin
- `--backup` — write `.bak` (default: off)
- `--output compact|json|human` — compact receipts by default; JSON supplies one complete result and human expands receipts/previews
- `--json` — emit NDJSON events on stdout (also with `--check`); conflicts with `--dry-run` and a nondefault `--output`
- `--parallel <N>` — run phase 1 with up to N worker threads (rayon), capped by file count and available CPUs. Phase 2 stays serial. `0` or `1` = single-threaded.
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

- `replace --pattern --repl --revert` → supports nonempty literal patterns and nonempty replacements without capture expansion; regex patterns are rejected. This is not an undo log; existing replacement text can make reversal ambiguous.
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

### Paths and edit ordering

Direct `file` targets resolve relative to the process working directory. `files-from` list paths, entries within those lists, and payload `text-file`/`@path` paths resolve relative to the manifest directory (or the working directory for stdin manifests). `was-file` is read during parsing and resolves relative to the process working directory. Use absolute paths when sharing manifests across working directories.

Targets are canonicalized before grouping; diagnostics may therefore show absolute paths. Anchors are resolved against the original file before descending-line ordering. Overlapping ranges and multiple insertions at one anchor are rejected. Pattern edits cannot be mixed with line-based edits on the same file, and a move must be the only operation on its file. Use `--sequential` for dependent edits. A pattern with zero matches fails validation. JSON/YAML reject unknown fields and invalid `where` values.

## File lifecycle operations and ordered plans

JSON/YAML support these additional operations. Native equivalents are shown below. Direct targets and `to-file` destinations resolve relative to the working directory; `old-file` and `text-file` payloads for `edit`/`create`/`write` resolve relative to the manifest directory. As with legacy operations, `was-file` is loaded during parsing relative to the working directory.

```json
[
  {"op":"create","file":"new.txt","text":"alpha\nbeta"},
  {"op":"write","file":"config.txt","text":"PORT=3000","was":"PORT=8080"},
  {"op":"edit","file":"new.txt","old":"beta","text":"gamma"},
  {"op":"rename","file":"new.txt","to-file":"renamed.txt"},
  {"op":"delete","file":"obsolete.txt","was":"expected original bytes"}
]
```

```text
create new.txt "alpha\nbeta"
write config.txt "PORT=3000" was="PORT=8080"
edit new.txt "beta" "gamma"
rename new.txt renamed.txt
delete obsolete.txt
```

- `create`: requires `text` or `text-file`; an existing virtual target is rejected.
- `write`: explicitly replaces the complete regular file or creates it if missing; requires `text` or `text-file`. JSON/YAML use `was`/`was-file` for a whole-file guard, not `old`. Native syntax is `write FILE CONTENT [was=OLD]`; its optional guard is literal bytes, including quoted multiline text. Native `was=@path` does not load a guard file; use structured `was-file` for that. Content accepts the same `@path`, `@-`, `@@` and quoted literal rules as create. A supplied guard requires the virtual file to exist, even when the guard is empty. Final symlinks and directories are rejected.
- `edit`: requires `old` or `old-file` plus `text` or `text-file`; literal unique matching, with optional `all: true`. Native form accepts a final `all` modifier. Old content cannot consume stdin.
- `delete`: optional whole-file `was`/`was-file` in JSON/YAML. Native delete has only the target; use structured format for a precondition.
- `rename`: requires `to-file`; optional whole-file `was`/`was-file` in JSON/YAML. The destination must be absent at the beginning of the plan. Pure moves require the same filesystem.

Any file lifecycle operation or patch switches the entire manifest to **input-order execution** against virtual file contents. `--sequential` requests this mode for legacy operations too. This permits create→edit→rename, or a pattern edit followed by a locator targeting its result. `--parallel` only applies to legacy grouped preparation; dependent ordered plans execute serially. Intermediate states are not written: the final state is coalesced per path before commit. External payload files are still read from disk, not from virtual targets created earlier in the same manifest.

Legacy-only manifests without `--sequential` retain original-file anchor resolution and descending-line ordering. Do not assume input order in that mode. A batch validates every operation before touching targets, but individual commits remain observable to external programs; recovery is not cross-file atomic visibility.

## Text patch import

`xnip apply changes.patch` or `--format patch --from-stdin` imports unified diff and `*** Begin Patch` syntax, including new/deleted files and renames. See [patch import](patch.md) for context, newline and path rules. MCP apply also supports `dry_run`, `check` and `sequential`, with diff output returned in the tool result rather than protocol stdout.
