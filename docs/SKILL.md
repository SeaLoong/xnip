---
name: xnip-editing
description: Use xnip as the primary text-file editing interface for code and configuration changes, including creation, exact block replacement, deletion, renaming, patch import and recoverable batches. Search and semantic analysis still use their appropriate tools.
---

# Edit text files with xnip

Use xnip as the normal editing entry point when it supports the requested operation. Respect an explicit user choice of another tool. Choose an operation that expresses the intended change directly; do not introduce extra tool calls just to follow a sequence.

## Default editing flow

Use existing context to form the edit. Read only missing context needed to identify the right target; there is no mandatory `doctor → peek → find` prelude. For a known old text block, use `edit` rather than recalculating line numbers or inventing a regex.

```sh
# Exact block replacement, unique match required
xnip edit config.txt --old 'PORT=8080' --text 'PORT=3000'
# Create a missing file and parent directories; existing paths are rejected
xnip edit src/new-file.txt --create --text-file content.txt
# Rename without overwriting a destination
xnip edit old-name.txt --rename-to new-name.txt
# Delete a whole file, optionally checking all its original bytes
xnip edit obsolete.txt --delete --old-file expected.txt --backup
```

File lifecycle, patch import and journal recovery require v0.2.0; v0.1.1 lacks them. Check `xnip --help` once if the installed capabilities are unknown. On older versions use `replace`/`insert` for supported edits, or an existing editor for missing operations; do not claim a command succeeded when it is unavailable.

Literal replacement preserves bytes, line endings and EOF exactly. `--old` and `--text` supply inline strings; CLI arguments do not decode `\n`. Use inline MCP `old`/`text` for known blocks; use payload files or `--text-stdin` when content size or shell quoting warrants them. Do not create payload files for every small edit. No match fails; multiple matches (including overlaps) fail unless `--all` is intentional. Add surrounding context to disambiguate; do not silently widen a replacement.

Use `--dry-run` when a diff will resolve uncertainty and `--check` for validation without writing. `xnip_edit` exposes both flags over MCP too. A preview/check result is not a committed edit. An identical replacement skips writing. Verify the relevant outcome without routinely rereading unchanged files.

## Specialized operations

- Known line or marker ranges: `replace --lines`, `--match-line`, `--between` or `--between-re`; `--was-file` can check original range bytes.
- Insert at a single-line anchor: `insert`; reorder an existing block: `move`; adjust indentation: `indent`.
- Independent edits to existing files: `apply`. Read [manifest syntax and ordering](apply-format.md) when building a batch. Create/delete/rename/edit manifests execute in input order against virtual contents. Use `--sequential` for dependent legacy operations.
- Locate missing context with `peek`/`find`, or repository search for unknown files. Text matching does not establish symbol scope; use language analysis for semantic refactors, then apply the intended text changes.

Import unified or apply_patch-style text patches with `apply --format patch`; read [patch rules](patch.md) when needed. Specify CLI `--patch-root` or MCP `patch_root` when the server working directory is not the project root. Use `edit --rename-to` for whole-file moves. Binary/document editing, mode-only patches, cross-device pure rename and complete filesystem metadata preservation require their appropriate tools.

## Failure and recovery

Use Git/backups for exact recovery; `--revert` is a limited inverse, not an undo log. `.bak` is appended to the full target name and replaces an existing backup. Expected-content checks are not locks against concurrent writers. Create never overwrites an existing path; delete refuses symbolic links and directories.

Batches commit per file with recovery snapshots. Commit errors attempt rollback; inspect the rollback status and journal before retrying. For an interrupted/incompletely restored transaction, read [recovery](recovery.md) and validate with `recover JOURNAL --check`. Never recover over unknown external changes or while its writer is still running. A missing/ambiguous literal block is exit 3. For other failures or scripting behavior, consult [exit codes](error-codes.md).

## Interface and references

Use the configured interface. CLI supports shell workflows and payload stdin; MCP avoids shell quoting. Both `xnip_edit` and `xnip_apply` expose MCP preview/check; `xnip_recover` has a non-writing dry run. Other MCP write tools commit immediately. Loading this skill does not authorize installing software or changing MCP configuration.

Read [examples](examples.md) for a relevant command recipe, [CLI details](https://github.com/SeaLoong/xnip/blob/master/docs/cli.md) for options, and [MCP details](https://github.com/SeaLoong/xnip/blob/master/docs/mcp.md) for structured fields. Load references only when needed.
