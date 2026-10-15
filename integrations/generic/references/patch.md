# Import text patches

Use `xnip apply changes.patch`, `xnip apply --format patch --from-stdin`, or MCP `xnip_apply` with `format: "patch"`. Both unified text diffs and the `*** Begin Patch` format are supported. A `.patch`/`.diff` suffix or a recognized patch header selects the parser automatically.

Patch operations are planned in input order against virtual file contents. Nothing is written until every operation validates. Preview with `--dry-run` or validate with `--check`; MCP apply exposes the same modes. Commit errors attempt rollback and preserve a recovery journal.

## Agent patch format

```text
*** Begin Patch
*** Add File: src/new.txt
+first line
+second line
*** Update File: src/config.txt
@@
 before
-PORT=8080
+PORT=3000
 after
*** Update File: src/old-name.txt
*** Move to: src/new-name.txt
*** Delete File: obsolete.txt
*** End Patch
```

Update hunks use a space for context, `-` for removal and `+` for insertion. A bare `@@` starts a hunk; `@@ exact section line` moves the search past one uniquely matching section line. `*** End of File` restricts a hunk to EOF. Context must match exactly and uniquely after earlier hunks; xnip does not trim indentation or guess among duplicate matches. For a context-free append, use an EOF marker; otherwise include context.

Add File creates UTF-8 text with LF endings and never overwrites a path. Update can be combined with Move to. Delete File explicitly removes the named regular file. Move destinations must be absent at the start of the plan; a distinct path is required for a case-only rename on case-insensitive filesystems.

## Unified diff

```diff
--- a/config.txt
+++ b/config.txt
@@ -1,2 +1,2 @@
-PORT=8080
+PORT=3000
 HOST=localhost
```

Hunk counts are checked. Header positions identify the original lines; if a position has drifted, the old context must match uniquely elsewhere after the preceding hunk. `/dev/null` identifies file creation/deletion. Git `diff --git`, `index`, similarity and rename metadata are recognized. A pure `rename from`/`rename to` block is supported without content hunks.

`\ No newline at end of file` is supported on hunk lines. Existing context lines retain their endings; new lines use the existing file's first newline convention (LF if none exists). A no-newline marker must describe the final output line. Patch import requires UTF-8 source files; arbitrary byte payloads belong in literal `edit` operations.

## Paths and supported scope

Paths are relative to the process working directory. Git's `a/` and `b/` header prefixes are stripped. Absolute paths, parent traversal, backslashes, drive/stream separators and paths resolving outside the working directory are rejected. Symlink creation and binary patches are not supported. Existing symlink escapes are rejected; this path validation is not a sandbox against a concurrently malicious filesystem writer.

Binary payloads, executable-bit/mode changes and combined merge diffs are rejected instead of silently ignored. A normal `new file mode 100644` header is accepted; it does not override platform defaults for newly created file permissions. Deleted-file mode metadata is informational. For ambiguous contexts, regenerate the patch with more context or use an exact `edit` operation. Never retry a partially committed plan until its rollback/recovery status is understood.

The complete operation ordering and file lifecycle schema are in the [manifest reference](apply-format.md). Recovery steps are in [recovery](recovery.md).
