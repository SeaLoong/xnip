---
name: xnip-editing
description: "Edit text files with xnip: exact replacement, creation, deletion, renaming, batches and patches."
---

# Edit with xnip

Use the configured xnip MCP tools or CLI for supported text edits. Respect the user's editor choice. Reuse known context; read only the missing region needed for a reliable edit. Use search and language tools for discovery and symbol analysis.

## Choose the edit

- Known block: `xnip_edit({file,old,text})` or `xnip edit FILE --old OLD --text NEW`. Literal matching requires one occurrence; extend the old block to disambiguate. Use `all=true` / `--all` only for an intentional global change.
- New file: `operation="create"` / `--create`. Delete: `operation="delete"` / `--delete`. Rename: `operation="rename",to=DEST` / `--rename-to DEST`. Create/rename require an absent destination.
- Several known blocks: `xnip_apply({edits:[{file,old,text},...]})`. For file lifecycle, line edits or a saved manifest, read [manifest ordering](references/apply-format.md).
- Existing patch: apply with `format="patch",patch_root=ROOT` / `--format patch --patch-root ROOT`; read [patch rules](references/patch.md).
- Known line/marker range, move or indentation: use the dedicated CLI command; [examples](references/examples.md) cover these operations.

Pass short content inline. For large CLI content use `--text-stdin` or payload files; CLI arguments keep literal `\n`. Exact replacement preserves untouched bytes, line endings and EOF.

## Observe the result

Read `mode` and `written` in structured results. `dry_run` / `--dry-run` previews a diff; `check` / `--check` validates. Both leave files unchanged. An identical replacement skips writing.

A missing/ambiguous old block fails without writing: inspect the relevant context and correct the next call. Verify the task's outcome with its relevant checks.

Read tools and previews bound output by default. Narrow the query or request a range when truncated; set an applicable limit to `0` only when the whole output is needed. MCP advertises five tools by default; `xnip mcp --toolset full` adds specialized tools.

## Recovery and reference

Batches validate before committing files and retain recovery snapshots if commit/rollback fails. Inspect the error and journal before retrying; read [recovery](references/recovery.md) before restoring an interrupted transaction. Expected bytes are checks, not locks. Git/backups provide exact recovery.

Load [edit details](references/edit.md), [exit codes](references/error-codes.md), [CLI options](https://github.com/SeaLoong/xnip/blob/master/docs/cli.md) or [MCP fields](https://github.com/SeaLoong/xnip/blob/master/docs/mcp.md) only for the operation at hand.
