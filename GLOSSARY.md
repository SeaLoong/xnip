# xnip editing

Precise text changes and the evidence an agent needs to apply them.

## Language

**Exact block**:
A nonempty sequence of original bytes identifying a replacement target. It must occur once unless replacing all matches is explicitly requested.
_Avoid_: fuzzy match, approximate context

**Whole-file write**:
A complete replacement file supplied as exact bytes, creating the target when absent. An optional whole-content precondition applies to the entire existing file.
_Avoid_: implicit rewrite, partial replacement

**File group**:
One target with ordered exact replacements or a complete-file write. Different groups are independent; order within a group uses its preceding virtual content.
_Avoid_: cross-file sequence

**Edit plan**:
The proposed final file contents and file existence changes, validated before committing a batch.
_Avoid_: script, undo history

**Preview**:
A proposed edit shown without writing files.
_Avoid_: completed edit, committed change

**Check**:
Validation of an edit plan without writing files or displaying its diff.
_Avoid_: dry run with changes

**Receipt**:
The edit result stating whether changes were written, unchanged, previewed or checked. Text receipts carry concise status; structured receipts add operation details and affected paths.
_Avoid_: proof from exit status alone

**Output budget**:
An explicit cap on the text returned to an agent. Omitted text is marked; complete output requires an explicit request.
_Avoid_: silent clipping

**Recovery journal**:
Retained original file snapshots and transaction state for conflict-checked restoration after a failed or interrupted commit.
_Avoid_: persistent undo log

**Usage lease**:
Evidence that one xnip process is still in use. A surviving lease file alone does not establish active usage.

**Update worker**:
The background owner of release checks, preparation and installation for one xnip installation.

**State gate**:
Exclusive access to shared usage and update scheduling state.

**Prepared release**:
A newer stable executable whose downloaded and extracted bytes have been verified and are waiting for installation.

**Idle-start window**:
A short installation opportunity opened by new usage after earlier usage has ended or become idle.

**Update deadline**:
The point after which active usage no longer defers installation attempts for a prepared release.
