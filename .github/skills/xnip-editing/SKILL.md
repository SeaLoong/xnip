---
name: xnip-editing
description: "Edit text files with xnip: exact replacements and insertion, whole-file writes, batches and patches."
---

# Edit with xnip

Use the configured xnip MCP tools or CLI for supported text edits. Respect the user's editor choice. Reuse known context; read only the missing region needed for a reliable edit. Use search and language tools for discovery and symbol analysis.

## Choose the edit

- Known block: `xnip_edit({file,old,text})` or `xnip edit FILE --old OLD --text NEW`. Literal matching requires one occurrence; extend the old block to disambiguate. Use `all=true` / `--all` only for an intentional global change.
- Exact insertion: add `position:"before"` or `"after"` to the known-block call (`--position` in CLI). The nonempty anchor must be unique; supplied bytes are inserted without adding a newline.
- Whole file: `operation:"write",text:CONTENT` / `--write` creates or replaces the file. Optional `old` checks the entire existing file. Use `create` / `--create` when an existing path must be rejected.
- Delete: `operation:"delete"` / `--delete`. Rename: `operation:"rename",to:DEST` / `--rename-to DEST`; the destination must be absent.
- Several files: `xnip_apply({files:{"a":[["old","new"]],"b":"complete content"}})`. Pairs run in order within each file; map files are independent. Read [manifest ordering](references/apply-format.md) for guarded whole writes, lifecycle, line edits or saved manifests.
- Existing patch: apply with `format="patch",patch_root=ROOT` / `--format patch --patch-root ROOT`; read [patch rules](references/patch.md).
- Known line/marker range, move or indentation: use CLI commands or native `xnip_apply` manifests; [examples](references/examples.md) cover these operations. For a known complete file, [compare representations](references/edit.md).

Pass short content inline. For large CLI content use `--text-stdin` or payload files; CLI arguments keep literal `\n`. Exact replacement preserves untouched bytes, line endings and EOF.

## Observe the result

Read the receipt: `written` commits changes, `unchanged` is a no-op, and preview/check never write. Structured results add `mode`, `written` and paths when enabled. `dry_run` / `--dry-run` previews a diff; `check` / `--check` validates. An identical replacement skips writing.

A missing/ambiguous old block fails without writing: inspect the relevant context and correct the next call. Verify the task's outcome with its relevant checks.

Read tools and previews bound output by default. Narrow the query or request a range when truncated; set an applicable limit to `0` only when the whole output is needed. MCP advertises five tools by default; `xnip mcp --toolset full` adds specialized tools.

## Recovery and reference

Batches validate before committing files and retain recovery snapshots if commit/rollback fails. Inspect the error and journal before retrying; read [recovery](references/recovery.md) before restoring an interrupted transaction. Expected bytes are checks, not locks. Git/backups provide exact recovery.

Load [edit details](references/edit.md), [exit codes](references/error-codes.md), [CLI options](https://github.com/SeaLoong/xnip/blob/master/docs/cli.md) or [MCP fields](https://github.com/SeaLoong/xnip/blob/master/docs/mcp.md) only for the operation at hand.
