# xnip — Exit codes & error semantics

Every xnip subcommand maps its outcome to one of these exit codes. Scripts and
agents should branch on them rather than parsing stderr text.

| Code | Name      | Meaning                                                         |
|------|-----------|-----------------------------------------------------------------|
| 0    | `SUCCESS` | Operation completed; files written (or read) as expected.       |
| 1    | `USAGE`   | User error: bad CLI args, locator not found, parse error, unknown subcommand. |
| 2    | `IO`      | IO error: cannot create tempfile, permission denied, disk full. |
| 3    | `CHECK`   | Validation failure: `--was` mismatch; `--check` rejected the plan; binary file refused. |
| 4    | `PARTIAL` | `apply` phase 2 partial commit: some files were renamed before a later rename failed. |

## Exit-code reference per command

### `peek`

- `0` printed the requested range
- `1` locator not found / out-of-bounds / no range given

### `find`

- `0` at least one hit emitted
- `1` no hits / locator missing / invalid regex

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
- `4` phase-2 partial commit (some renames done, then failure)

### `doctor`

- `0` always (informational)

## Notes on `apply --json`

When `--json` is set on `apply`, error events are emitted on stdout as NDJSON:

```json
{"event":"error","kind":"phase1","message":"..."}
```

`kind` is one of `phase1`, `phase2`, `io`. The exit code still follows the table above.
