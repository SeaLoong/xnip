# Default file editing with edit

`xnip edit` replaces exact blocks, inserts at literal anchors, writes complete files, creates missing files, deletes files and renames whole files. Reuse known content without calculating line numbers or constructing regular expressions.

Use `xnip --version` and `xnip edit --help` to check the configured executable. Installation and source-build options are in [installation](https://github.com/SeaLoong/xnip/blob/master/docs/installation.md).

## Choose an edit

Required checks determine which representations are eligible. Keep an explicitly required old block intact. A partial snippet does not establish that a shorter substring is unique in the whole file.

Known exact blocks can also share one native manifest:

```json
{"name":"xnip_apply","arguments":{"manifest_text":"edit config.txt PORT=8080 PORT=3000\nedit src/example.txt old_name new_name all"}}
```

The first record requires a unique nonempty old block; `all` in the second explicitly replaces every non-overlapping occurrence. Exact records preserve payload bytes, CRLF and EOF without adding newlines. The JSON `\n` separates records; the final record needs no newline. Compare complete serialized arguments: a native manifest is not always shorter than typed JSON.

The same literal guard can also be expressed as a native manifest over MCP:

```json
{"name":"xnip_apply","arguments":{"manifest_text":"edit config.txt PORT=8080 PORT=3000\nedit src/example.txt old_name new_name all"}}
```

The first record requires a unique old block; the explicit final `all` in the second replaces every non-overlapping match. Both preserve supplied bytes and untouched CRLF/EOF, without newline synthesis. The JSON newline separates records; the final record needs no trailing newline. No full-file read is required for this representation when the required old text is already known. Compare the complete serialized arguments rather than assuming native text or typed JSON is always shorter.

When the complete current file is already known and unguarded edits are allowed, compare:

- A native line manifest for a known range, avoiding unchanged function headers or surrounding lines.
- Shorter exact pairs after trimming common old/new edges, while keeping a nonempty old block unique in the current virtual content. Preserve every required old-content check.
- A complete-file write when the final file itself is shorter.

For example, replacing an accumulator at known lines 2–5 needs only the new statement:

```json
{"name":"xnip_apply","arguments":{"manifest_text":"replace src/example.txt 2-5 \"    return sum(item.price for item in items)\""}}
```

This uses the default five-tool catalog. Native format is detected without an explicit `format`; the last record needs no trailing newline. Line replacement supplies a final LF only when the replaced range had one and the nonempty payload lacks one. Exact replacement and whole-file writes instead preserve supplied bytes literally.

For an insertion after known line 1:

```json
{"name":"xnip_apply","arguments":{"manifest_text":"insert src/example.txt 1 \"import sys\""}}
```

Native insert defaults to `after` and applies line-oriented newline rules. Use literal anchor insertion when supplied bytes must be inserted without newline synthesis. These choices share validation and persistence, but their preconditions and newline rules differ. Additional reads and tool-discovery costs still count when that context or interface is not already available.

## Replace known text

```sh
xnip edit config.txt --old 'PORT=8080' --text 'PORT=3000'
xnip edit source.txt --old-file old-block.txt --text-file new-block.txt --dry-run
xnip edit source.txt --old-file old-block.txt --text-file new-block.txt --backup
```

Matching is literal, including whitespace and line endings. `$1` is literal replacement text here, not a capture expansion. The default requires exactly one occurrence, including overlapping occurrences. Missing or ambiguous text fails with exit 3 and leaves the target untouched. Include more surrounding context to identify a unique block, or deliberately use `--all` for every non-overlapping occurrence.

Payloads are byte-preserving: no trailing newline is added and CRLF is not converted. An empty replacement deletes the matched block. An empty old block is rejected; use `--position` with a nonempty literal anchor for byte insertion, or the dedicated insert command for a line anchor. Identical old/new bytes return success without writing or replacing backups.

`--old-file` and `--text-file` accept payloads from files. `--text-stdin` reads replacement bytes from stdin, including an empty stream. Inline and file sources are mutually exclusive for each payload. CLI strings follow shell quoting; xnip does not interpret `\n` escapes in shell arguments.

## Insert exact bytes

```sh
xnip edit source.txt --old 'anchor' --position before --text 'prefix:'
xnip edit source.txt --old 'anchor' --position after --text ':suffix'
```

The anchor follows the same nonempty, unique byte-match rule as replacement. `before` inserts immediately before the block; `after` inserts immediately after its last byte. The anchor remains intact. No newline, indentation or separator is synthesized; include those bytes in the payload. Position is available only for the default replace operation and cannot be combined with `--all` or another file operation. MCP uses `position: "before"` or `position: "after"`.

## Write a complete file

```sh
xnip edit config.txt --write --text 'PORT=3000'
xnip edit config.txt --write --old-file expected-whole-file.txt --text-file complete-file.txt
xnip edit src/generated.txt --write --text-file complete-file.txt --check
```

`--write` explicitly creates a missing file or replaces an existing regular file with exactly the supplied content. It creates missing parents only when committing. Optional `--old`/`--old-file` compares the entire existing file, including CRLF and EOF; a supplied empty guard matches an existing empty file, never a missing file. Omit the guard when an unconditional whole-file write is intended. Identical content succeeds without writing.

Whole-file write rejects a final symlink, including dangling symlinks, and directories. It uses the shared transaction plan, supports backups and preview/check, and reports a retained journal if commit recovery needs attention. Use `--create` when existing paths must remain protected by no-clobber creation.

## Create and delete

```sh
xnip edit src/new.txt --create --text-file content.txt
xnip edit empty.txt --create --text ''
xnip edit obsolete.txt --delete --old-file expected.txt --backup
```

Create makes missing parent directories and uses a no-clobber commit: an existing path, including a dangling symlink, is never overwritten. The new file contains exactly the supplied bytes. Empty parent directories may remain if a later IO operation fails.

Delete accepts regular files only and does not recursively delete directories or follow a final symlink. Optional `--old`/`--old-file` compares the entire original file, rather than a contained block. With `--backup`, the original bytes are saved to `<file>.bak` before deletion. Without an old-content precondition, delete acts on the named regular file; use it only when removing that file is intended.

## Whole-file rename

```sh
xnip edit old-name.txt --rename-to new-name.txt --backup
```

The destination must be absent. Optional `--old`/`--old-file` checks the entire source. Pure moves preserve file identity using no-clobber hard-link/unlink on the same filesystem; no data-copy fallback is hidden behind a failed move. Rename does not accept replacement text or `--all`. Same-name moves are no-ops; case-only renames on a case-insensitive filesystem are rejected. Rename uses the [recovery plan](recovery.md), including previews and rollback after commit failure.

MCP uses `operation: "rename"` and `to` for the destination.

## Preview and validation

`--dry-run` returns a unified diff; `--check` validates without the diff. Both leave targets and directories unchanged, and are mutually exclusive. Status output identifies create/replace/delete even for empty-file operations that have no textual diff. CLI status goes to stderr, and dry-run diff goes to stdout.

Replacement follows existing symlinks and preserves ordinary file permissions through the shared atomic-write layer. The file is read again before commit to reject detected concurrent changes, but this is not a lock or a filesystem compare-and-swap operation. Ownership, ACLs, extended attributes and hard-link identity are not guaranteed. See [persistence limits](architecture.md#persistence-and-limits).

Compact receipts distinguish committed changes, unchanged content, preview and check. Rename reports both endpoints. CLI compact previews limit payloads to 12,000 bytes with an explicit notice; `--output human` gives the complete diff and `--output json` a complete machine-readable receipt. MCP uses the same default preview budget, configurable through `max_bytes` (zero for full output), and returns [structured result metadata](mcp.md#result-display) by default. `xnip mcp --receipts text` keeps the receipt/preview while omitting that metadata and its advertised output schema.

## MCP

```json
{
  "name": "xnip_edit",
  "arguments": {
    "file": "config.txt",
    "old": "PORT=8080",
    "text": "PORT=3000",
    "dry_run": true
  }
}
```

`operation` is `replace` (default), `write`, `create`, `delete`, or `rename`. Inputs mirror CLI fields using underscores: `old`, `old_file`, `text`, `text_file`, `position`, `all`, `dry_run`, `check`, `backup`. For `write`, `old` is a whole-file guard rather than a contained block. The protocol owns stdin, so `text_stdin` is not exposed. Unknown fields are rejected. Preview and check return a validation result, not confirmation that a write occurred.

## Choosing another xnip operation

Use `replace`/`insert` for line or marker ranges, `move` for line blocks, `indent` for indentation, and `apply` for validated batches. `edit` currently operates on one file per request; apply supports per-file exact pairs, complete-file writes and creation/deletion/rename with virtual planning. Import text patches through `apply --format patch`. See [manifest syntax](apply-format.md) and [patch import](patch.md).

The agent can reuse already-known context to edit directly. Search or read only when needed to locate the intended block; default editing does not require an extra doctor/peek/find sequence for every change.
