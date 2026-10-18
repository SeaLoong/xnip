# xnip — Exit codes & error semantics

Every xnip subcommand maps its outcome to one of these exit codes. Scripts and
agents should branch on them rather than parsing stderr text.

| Code | Name      | Meaning                                                         |
|------|-----------|-----------------------------------------------------------------|
| 0    | `SUCCESS` | Operation completed; files written (or read) as expected.       |
| 1    | `USAGE`   | User error: bad CLI args, locator not found, parse error, unknown subcommand. |
| 2    | `IO`      | IO error: cannot create tempfile, permission denied, disk full. |
| 3    | `CHECK`   | Validation failure: `--was` mismatch; `--check` rejected the plan; pattern matched nothing. |
| 4    | `PARTIAL` | Batch or whole-file rename commit failed; rollback was attempted, inspect status and recovery journal. |

## Exit-code reference per command

### `edit`

- `0` committed, previewed, checked or unchanged
- `1` invalid arguments/operation or unsupported target type
- `2` IO/persistence failure
- `3` existing create target, missing/ambiguous old block, failed deletion precondition or detected concurrent change
- `4` whole-file rename commit failed; rollback was attempted and a recovery journal was retained

### `peek`

- `0` printed the requested range
- `1` locator not found / out-of-bounds / no range given
- `2` file or output IO failure

### `find`

- `0` at least one hit emitted
- `1` no hits / locator missing / invalid regex
- `2` any input failed to load, even if another input matched

### `replace`

- `0` wrote (or printed in `--dry-run`)
- `1` invalid args / locator not resolved / `--repl` without `--pattern` / unsupported `--revert`
- `2` IO error
- `3` `--was` mismatch or `--check` failed; `--pattern` matched zero with no `--check`

### `insert` / `move` / `indent`

- `0` wrote (or printed in `--dry-run`)
- `1` invalid args / locator not resolved
- `2` IO error

### `apply`

- `0` all ops applied
- `1` manifest parse error
- `2` IO error reading manifest or expanding `--files-from`
- `3` phase-1 failure (no file modified)
- `4` phase-2 commit failure, even if automatic rollback completes; inspect the journal/status

### `recover`

- `0` restored or validated; snapshots are retained
- `3` invalid/corrupt journal, conflict, or recovery failure; inspect the message before retrying

### `doctor`

- `0` report written (including diagnostic failures such as a non-writable directory)
- `2` output IO failure

## Notes on `apply --json`

When `--json` is set on `apply`, execution-phase errors are emitted on stdout as NDJSON (argument, manifest-read and parse errors occur before execution and are reported on stderr):

```json
{"event":"error","kind":"phase1","message":"..."}
```

`kind` is one of `phase1`, `phase2`, `io`. The exit code still follows the table above.

`--check` is not a universal exit-code override: malformed CLI arguments and unresolved single-command locators still return 1; IO failures return 2. Batch phase-1 failures return 3. `find --max-matches 0` removes the hit cap; `--max-bytes 0` also removes the output byte cap.
