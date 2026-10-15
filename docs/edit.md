# Default file editing with edit

`xnip edit` is the general text-file editing entry point: create a file, replace an exact block, delete a file, or rename a whole file. It fills the common creation/string-replacement/deletion gap without requiring the agent to calculate line numbers or construct regular expressions.

This command and the `xnip_edit` MCP tool are available in v0.2.0. Upgrade using the release installers, or build the current checkout with `cargo install --path . --locked --force`. Older v0.1.1 binaries only provide the original line-based editing commands.

## Replace known text

```sh
xnip edit config.txt --old 'PORT=8080' --text 'PORT=3000'
xnip edit source.txt --old-file old-block.txt --text-file new-block.txt --dry-run
xnip edit source.txt --old-file old-block.txt --text-file new-block.txt --backup
```

Matching is literal, including whitespace and line endings. `$1` is literal replacement text here, not a capture expansion. The default requires exactly one occurrence, including overlapping occurrences. Missing or ambiguous text fails with exit 3 and leaves the target untouched. Include more surrounding context to identify a unique block, or deliberately use `--all` for every non-overlapping occurrence.

Payloads are byte-preserving: no trailing newline is added and CRLF is not converted. An empty replacement deletes the matched block. An empty old block is rejected; use the dedicated insert command to insert at a line anchor. Identical old/new bytes return success without writing or replacing backups.

`--old-file` and `--text-file` accept payloads from files. `--text-stdin` reads replacement bytes from stdin, including an empty stream. Inline and file sources are mutually exclusive for each payload. CLI strings follow shell quoting; xnip does not interpret `\n` escapes in shell arguments.

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

Replacement follows existing symlinks and preserves ordinary file permissions through the shared atomic-write layer. The file is read again before commit to reject detected concurrent changes, but this is not a lock or a filesystem compare-and-swap operation. Ownership, ACLs, extended attributes and hard-link identity are not guaranteed. See [persistence limits](architecture.md#persistence-and-operational-limits).

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

`operation` is `replace` (default), `create`, `delete`, or `rename`. Inputs mirror CLI fields using underscores: `old`, `old_file`, `text`, `text_file`, `all`, `dry_run`, `check`, `backup`. The protocol owns stdin, so `text_stdin` is not exposed. Unknown fields are rejected. Preview and check return a validation result, not confirmation that a write occurred.

## Choosing another xnip operation

Use `replace`/`insert` for line or marker ranges, `move` for line blocks, `indent` for indentation, and `apply` for disjoint batches on existing files. `edit` currently operates on one file per request; apply supports creation/deletion/rename with ordered virtual planning. Import text patches through `apply --format patch`. See [manifest syntax](apply-format.md) and [patch import](patch.md).

The agent can reuse already-known context to edit directly. Search or read only when needed to locate the intended block; default editing does not require an extra doctor/peek/find sequence for every change.
