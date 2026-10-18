# Transaction failure and recovery

`apply` and whole-file rename use a shared plan. All operations are validated in memory before commit. The plan compares every observed file with its original contents before writing, and checks each target again before its individual commit. This detects changes but is not a lock against external writers or a cross-file atomic filesystem transaction.

## Recovery records

Before modifying target files, xnip creates a private `xnip-transaction-*` directory in the system temporary directory. It stores a versioned `journal.json`, original and planned bytes, SHA256 integrity values and ordinary file permissions. Snapshot files and the manifest are synced before commit; Unix also syncs the journal directory. This supports process-interruption recovery, but is not a guarantee against all power-loss or filesystem failures.

Journaling adds disk IO and temporary space proportional to original plus planned file contents; files are also planned in memory. Preview/check modes skip snapshot creation.

The journal path is available through `--trace` and is included in commit-failure errors. If a process crashes before returning an error, look for its recent `xnip-transaction-*` directory in the system temporary directory and inspect its recorded target paths. Do not select a journal based only on its age.

Successful commits remove their journal. If cleanup fails, the retained path is available in the execution report. Failed commits retain it whether automatic rollback succeeds or not. Journals contain file contents: keep them private and remove a retained directory only after verifying recovery and deciding its snapshots are no longer needed.

## Automatic rollback

If a commit step fails, xnip attempts to restore the original state. It checks all targets against their original or planned contents before restoring anything. If any path has an unexpected type, redirected parent, or conflicting content, recovery is refused and the snapshots are preserved. Errors report whether rollback completed or remains incomplete.

Pure same-filesystem renames use a no-clobber hard-link/unlink sequence, preserving the file's identity and metadata. There is a brief interval when both names exist. Recovery can reverse an interrupted move without recreating the file. Modified-file renames use staged content writes and are not inode-preserving. Filesystems without hard-link support and cross-device pure moves fail safely; case-only renames on case-insensitive filesystems are rejected.

Restoration can itself fail because of permissions, disk space or new concurrent writes. Empty parent directories created during commit may remain. `.bak` sidecars are user-requested backups and are not rolled back to an earlier sidecar version. A failed commit remains exit 4 even when rollback completes; it is not a successful edit.

## Explicit recovery

After the writer has stopped, use the journal from that transaction:

```sh
xnip recover /absolute/path/to/xnip-transaction-ID --check
xnip recover /absolute/path/to/xnip-transaction-ID --dry-run
xnip recover /absolute/path/to/xnip-transaction-ID
```

`--check` checks snapshot integrity and conflicts; `--dry-run` also prints rollback diffs. Neither restores files. Recovery is repeatable: paths already restored to their original contents are left alone. Journals are retained after explicit recovery for inspection. A failed recovery returns exit 3; inspect the message and retained snapshots before taking further action.

MCP exposes `xnip_recover` with `journal` and optional `dry_run`. It follows the same conflict rules. Only recover a known, locally generated transaction: the journal contains absolute paths and restoring it writes those paths. Never invent a journal path or silently force recovery over an external edit. No force-overwrite recovery mode is provided.

## Boundaries

Single-file create/replace/delete through `edit` retain their existing atomic-write/backup behavior; whole-file rename and all apply modes use the recovery plan. A completed successful edit is not kept in an automatic undo history. Use Git or `--backup` for later user-requested undo.

The journal preserves ordinary permissions, not a complete copy of ownership, ACLs or extended attributes for rewritten files. External content and path conflicts are checked optimistically; filesystem races cannot be completely eliminated without cooperation from other writers. Temp-directory cleanup by the OS can remove old recovery records, so recover promptly after an interruption.
