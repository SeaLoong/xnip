# Editing examples

Reuse context already available. Read/search only the missing region; choose a preview when its diff will resolve uncertainty.

## Exact replacement and file lifecycle

```sh
xnip edit config.txt --old 'PORT=8080' --text 'PORT=3000'
xnip edit source.txt --old-file old-block.txt --text-file new-block.txt --dry-run
xnip edit src/new.txt --create --text-file content.txt
xnip edit old-name.txt --rename-to new-name.txt
xnip edit obsolete.txt --delete --old-file expected-whole-file.txt --backup
```

An exact old block must be nonempty and unique. Extend it with context to disambiguate, or use `--all` for a deliberate global replacement. Literal editing preserves CRLF, EOF and dollar signs. Create/rename require an absent destination; preview/check do not create parent directories.

For several known blocks over MCP:

```json
{"name":"xnip_apply","arguments":{"edits":[
  {"file":"src/a.rs","old":"old_name","text":"new_name"},
  {"file":"src/b.rs","old":"old_name","text":"new_name"}
]}}
```

## Lines and anchors

```sh
xnip replace src/config.rs --lines 30 --text 'const PORT: u16 = 3000;'
xnip replace src/foo.rs --lines 30-45 --text ''
xnip replace src/Foo.vue --match-line '^const PORT' --text 'const PORT = 3000;'
xnip replace src/lib.rs --between-re '^pub fn old\('..'^}' --inclusive --text-file new-body.rs
```

Line numbers are 1-based and endpoints inclusive. Add `--was-file expected-range.txt` to verify original range bytes, including line endings. Line operations use LF separators; supply CRLF payloads when required. Anchors match text, so use language analysis to establish symbol scope before a semantic refactor.

## Patterns, insertion and movement

```sh
xnip replace src/Foo.rs --pattern '\bMAX_RETRIES\b' --repl 'MAX_TRIES'
xnip replace src/Foo.rs --pattern OLD --repl NEW --count 2
xnip insert src/main.ts --match-line '^import ' --occurrence 1 --position after --text "import { ref } from 'vue';"
xnip move src/lib.rs --from-lines 100-150 --to 30 --position before
xnip indent src/foo.go --lines 1-99 --tabs-to-spaces 4
xnip indent src/foo.go --lines 30-45 --add 2
```

Patterns use Rust regex syntax; replacement strings expand captures. Use a unique anchor or an occurrence known to exist. `--revert` supports only [restricted inverses](apply-format.md#--revert-semantics); Git/backups provide exact recovery.

## A validated batch

Save as `edits.json`:

```json
[
  {"op":"edit","file":"src/lib.rs","old":"MAX_RETRIES","text":"MAX_TRIES","all":true},
  {"op":"edit","file":"src/main.rs","old":"MAX_RETRIES","text":"MAX_TRIES","all":true}
]
```

```sh
xnip apply edits.json --check
xnip apply edits.json --dry-run
xnip apply edits.json --output json --check
xnip apply edits.json
```

Checks/previews leave the plan's original state available; only the final command writes. Exact replacements cannot be replayed after their old text has been removed. Commit failures attempt rollback and retain a [recovery journal](recovery.md); inspect it before retrying.

For a compact native manifest:

```text
edit src/lib.rs "MAX_RETRIES" "MAX_TRIES" all
edit src/main.rs "MAX_RETRIES" "MAX_TRIES" all
```

Import an existing text patch with `xnip apply changes.patch --patch-root /project`; see [patch rules](patch.md).

## Missing context and diagnostics

```sh
xnip peek src/lib.rs --lines 80-120
xnip peek src/lib.rs --match-line '^pub fn build_app' --context 5
xnip find src/main.rs --match-line '^use serde'
xnip find src/main.rs src/lib.rs --pattern '\bTODO\b'
xnip doctor
```

Reads are bounded by default. Narrow the query when truncated; request `peek --max-lines 0 --max-bytes 0` or `find --max-matches 0 --max-bytes 0` when full output is needed. `find` accepts file paths; shell glob expansion varies across platforms. `doctor` reports version, OS/architecture, target, cwd writability and TTY flags.
