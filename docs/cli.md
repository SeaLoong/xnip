# CLI usage

v0.2.0 uses [xnip edit](edit.md) for creation, exact block replacement, file deletion and rename; published v0.1.1 provides line/anchor editing of existing files. Line numbers are 1-based and ranges include both endpoints. Use `xnip COMMAND --help` for every option and its default.

## A runnable first edit

Use a scratch directory. In a POSIX shell:

```sh
mkdir xnip-demo
cd xnip-demo
printf 'alpha\nbeta\ngamma\n' > sample.txt
xnip peek sample.txt --all
xnip replace sample.txt --lines 2 --text BETA --dry-run
xnip replace sample.txt --lines 2 --text BETA --backup
xnip find sample.txt --pattern BETA
xnip peek sample.txt --all
```

In PowerShell, create the same UTF-8 file without a BOM:

```powershell
New-Item -ItemType Directory xnip-demo
Set-Location xnip-demo
[IO.File]::WriteAllText((Join-Path $PWD 'sample.txt'), "alpha`nbeta`ngamma`n", [Text.UTF8Encoding]::new($false))
xnip peek sample.txt --all
xnip replace sample.txt --lines 2 --text BETA --dry-run
xnip replace sample.txt --lines 2 --text BETA --backup
xnip find sample.txt --pattern BETA
```

The preview prints the complete proposed file; only the second replace writes. The original remains in `sample.txt.bak`. Restore exact original bytes by copying that backup over the target, or use Git when the file is tracked.

## Commands

| Command | Typical use | Output |
|---|---|---|
| `edit FILE --old OLD --text NEW` | Exact block replacement; `--create` and `--delete` handle whole files | Status or dry-run diff |
| `peek FILE --lines 3-8` | Inspect a range; `--all` reads every line | Numbered lines |
| `find FILE... --pattern RE` | Search explicit files | `path:line:column`, byte-based columns |
| `replace FILE --lines 3 --text TEXT` | Replace a line; empty text deletes it | Status on stderr |
| `insert FILE --lines 3 --position after --text TEXT` | Add complete lines | Status on stderr |
| `move FILE --from-lines 3-5 --to 10 --position before` | Reorder a block | Status on stderr |
| `indent FILE --all --add 2` | Add/remove indentation or convert tabs/spaces | Status on stderr |
| `apply MANIFEST` | Validate a batch, then commit each file | Status or optional NDJSON |
| `recover JOURNAL --check` | Validate recovery before restoring an interrupted transaction | Recovery report |
| `doctor` | Inspect version, platform, target and environment | Diagnostic report |
| `mcp` | Start the stdio tool server | JSON-RPC protocol only |

`find` takes files, not directories; glob expansion is the shell's responsibility and differs across shells. `--match-line` searches one line at a time and prints no column. `--max-matches N` caps the total output and `--first-only` caps each file to one hit. Regexes use Rust regex syntax; lookaround and backreferences in the pattern are not supported. Replacement strings support capture expansion such as `$1` and `${name}`.

## Locators and payloads

For replace/insert, choose one locator: `--lines`, `--match-line`, `--between`, or `--between-re`. Pattern replacement instead uses `--pattern` with `--repl`. `--between 'BEGIN'..'END'` excludes marker lines unless `--inclusive` is supplied. Anchors must resolve to the required range; insert requires one line. An existing empty file accepts `insert --lines 1`.

Move uses `--from-lines`/`--from-match-line`; indent uses `--lines`/`--all`. These commands do not accept the complete replace locator set.

Use `--text-file payload.txt` or `--text-stdin` for multiline content. CLI `--text` does not decode `\n`; the shell determines actual characters. Quote replacement strings with single quotes when the shell would otherwise expand `$1`. Native manifests, JSON and YAML each have their own escaping rules; see [manifest syntax](apply-format.md).

Line edits use LF separators and do not automatically convert new text to the target's newline convention. For CRLF files, provide CRLF payloads. Byte-oriented pattern replacement preserves unmatched bytes. Displaying non-UTF-8 data may use replacement characters; do not reconstruct original file bytes from display output.

## Preview and validation

`edit` and `replace` support `--check`; the older insert/move/indent commands currently offer `--dry-run` without a separate check flag. Check validates without writing. `edit` and `apply` dry runs print unified diffs; other single-file dry runs print the complete proposed contents. `--backup` appends `.bak` to the full target name and replaces an existing backup. `--quiet` suppresses success notices, `--no-color` disables ANSI output, and `--trace` adds diagnostics on stderr.

For line-based replacement, use `--was-file expected.txt` to compare the original range exactly, including line endings. `--was` provides the same comparison with a literal shell argument. Pattern replacement rejects these preconditions instead of ignoring them. A precondition is not a file lock; avoid simultaneous external writers.

`--revert` implements limited inverse operations, not a history stack. Read the [revert restrictions](apply-format.md#--revert-semantics) and use backups or Git for exact recovery.

## Batches and automation

Save this JSON as `edits.json` next to the tutorial's `sample.txt`:

```json
[
  {"op":"replace","file":"sample.txt","lines":1,"text":"ALPHA"},
  {"op":"insert","file":"sample.txt","lines":3,"where":"after","text":"delta"}
]
```

```sh
xnip apply edits.json --check
xnip apply edits.json --dry-run
xnip apply edits.json --backup --json
```

Apply resolves anchors against the original file and orders line edits from the bottom upward. In legacy grouped mode use disjoint ranges; do not mix pattern replacements with line edits on one file or combine a move with another edit to its file. Use `--sequential` for dependent edits. File lifecycle operations and text patches automatically use input-order virtual planning. Direct targets use the process working directory; other paths follow the [manifest path rules](apply-format.md#paths-and-edit-ordering).

Validation errors leave targets unchanged. Commit errors return exit 4 after attempting rollback and retain a recovery journal. Inspect the rollback result; use [recover](recovery.md) for an interrupted or incomplete restoration. `--json --check` emits JSON events only; `--json --dry-run` is rejected. Parse/read errors before execution still use stderr. See [exit codes](error-codes.md) for scripting behavior.
