# xnip — Real-World Examples

## Example 1: Renaming a Function

**Task:** Rename `calculateTotal()` to `sumItems()` across a file.

```bash
xnip replace src/utils.ts \
  --pattern '\bcalculateTotal\b' \
  --repl sumItems
```

**What happens:**
- All occurrences of `calculateTotal` (as whole word) are replaced with `sumItems`
- Atomic operation; safe across the entire file

---

## Example 2: Editing a Config File by Line Number

**Task:** Update port in config, but verify first.

```bash
# Step 1: Preview
xnip peek config/app.json --lines 10-20

# Step 2: Preview the change
xnip replace config/app.json --lines 15 --text 'port: 3000,' --dry-run

# Step 3: Apply
xnip replace config/app.json --lines 15 --text 'port: 3000,'
```

---

## Example 3: Semantic Anchor-Based Insert

**Task:** Add a new export after the default export (survives code formatting changes).

```bash
xnip insert src/component.tsx \
  --match-line '^export default' \
  --position after \
  --text 'export const metadata = { title: "MyComponent" };'
```

**Advantage:** Works even if surrounding code reformats; regex anchors are drift-resistant.

---

## Example 4: Replace a Code Block

**Task:** Update the body of a function without counting lines.

```bash
# Step 1: Preview function location
xnip peek src/math.rs --pattern '^fn calculate'

# Step 2: Replace between markers
xnip replace src/math.rs \
  --between-re '^fn calculate'..'^}' \
  --inclusive \
  --text-file new-function.rs
```

**Why `--inclusive`:** Include the opening `fn` and closing `}` in the replacement.

---

## Example 5: Batch Multi-File Edits

**Task:** Apply version number changes across 3 files atomically.

**Create edits.json:**
```json
[
  {
    "op": "replace",
    "file": "src/config.ts",
    "match_line": "^export const VERSION",
    "text": "export const VERSION = '2.1.0';",
    "was": "export const VERSION = '2.0.0';\n"
  },
  {
    "op": "replace",
    "file": "package.json",
    "match_line": "\"version\"",
    "text": "  \"version\": \"2.1.0\",",
    "was": "  \"version\": \"2.0.0\",\n"
  },
  {
    "op": "replace",
    "file": "docs/changelog.md",
    "lines": "1-3",
    "text": "# Changelog\n\n## 2.1.0 (2024-05-16)\n",
    "text_file": "changelog-snippet.md"
  }
]
```

**Apply:**
```bash
# Dry-run first
xnip apply edits.json --dry-run

# Then apply
xnip apply edits.json
```

**Benefit:** All three files updated atomically; if any fails, none complete.

---

## Example 6: Indent a Block

**Task:** Fix indentation of a misaligned block.

```bash
# Before: lines 10-15 are indented by 2 spaces instead of 4
xnip indent src/app.js --lines 10-15 --add 2
```

**Alternatives:**
```bash
# Remove 1 level (4 spaces)
xnip indent src/app.js --lines 10-15 --remove 1

# Convert tabs to 2 spaces
xnip indent src/app.js --all --tabs-to-spaces 2

# Convert 2 spaces to tabs
xnip indent src/app.js --all --spaces-to-tabs 2
```

---

## Example 7: Move a Function

**Task:** Move lines 50-80 (a function) to after line 120.

```bash
xnip move src/main.rs \
  --from-lines 50-80 \
  --to 120 \
  --position after
```

**Result:** Lines 50-80 removed and inserted after the new location; all other line numbers adjust accordingly.

---

## Example 8: Handling Drift with `--was`

**Task:** Update a config value but fail if someone else changed it.

```bash
xnip replace config.yml \
  --lines 42 \
  --text 'timeout: 30s' \
  --was 'timeout: 60s\n'
```

**What happens:**
- If line 42 currently contains `timeout: 60s\n`, the edit proceeds
- If line 42 is different, xnip fails with exit code 3 (validation error)
- **Benefit:** Prevents silent conflicts

---

## Example 9: Replace in stdin (Piped Content)

**Task:** Process content from a pipe, then write to file.

```bash
# Generate edits from another tool, then apply
cat edits.txt | xnip apply --from-stdin
```

---

## Example 10: Complex Refactor Across Multiple Files

**Task:** Refactor an import across 5 files.

**edits.yaml:**
```yaml
- op: replace
  file: src/utils.ts
  match_line: '^import.*lodash'
  text: 'import { sum, map } from "modern-utils";'

- op: replace
  file: src/math.ts
  pattern: '_.sum'
  repl: sum

- op: replace
  file: src/collection.ts
  pattern: '_.map'
  repl: map

- op: insert
  file: src/index.ts
  match_line: '^// Utilities'
  text: 'export { sum, map } from "./math.ts";'
  position: after

- op: replace
  file: tests/utils.test.ts
  between_re: '^describe.*utils'..'^}'
  inclusive: true
  text_file: updated-tests.js
```

**Apply:**
```bash
xnip apply edits.yaml --dry-run
xnip apply edits.yaml
```

---

## Example 11: Locate Before You Edit

**Task:** Find all occurrences of a deprecated API.

```bash
# Find
xnip find src --pattern '@deprecated\s+getUser'

# Preview context
xnip peek src/auth.ts --pattern '@deprecated'

# Then replace
xnip replace src/auth.ts \
  --pattern 'getUser\(' \
  --repl 'getCurrentUser('
```

---

## Example 12: Delete a Region

**Task:** Remove a code block (lines 25-35).

```bash
xnip replace src/main.rs --lines 25-35 --text ''
```

**Alternative (semantic):**
```bash
xnip replace src/main.rs \
  --between-re '^// TODO: old code'..'^// END old code' \
  --inclusive \
  --text ''
```

---

## Example 13: Safe Edits with Backup

**Task:** Update a critical config file, but keep `.bak`.

```bash
xnip replace config/production.json \
  --lines 50 \
  --text 'db_host: "prod.example.com",' \
  --backup
```

**Result:** Creates `config/production.json.bak` with the original, then writes the new version.

---

## Example 14: Validation Without Write

**Task:** Check if a manifest is valid before applying.

```bash
xnip apply edits.json --check

# Exit code 0 = valid
# Exit code 3 = validation error
```

---

## Decision Tree: Which Command?

```
Do I want to...
├─ See lines?                          → xnip peek
├─ Search for something?               → xnip find
├─ Replace/delete?
│  ├─ Know exact line(s)?              → xnip replace --lines
│  ├─ Use semantic anchor?             → xnip replace --match-line
│  ├─ Replace all matches?             → xnip replace --pattern
│  └─ Replace a block?                 → xnip replace --between-re
├─ Insert content?
│  ├─ After a single anchor?           → xnip insert --position after
│  └─ Before a marker?                 → xnip insert --position before
├─ Move lines?                         → xnip move
├─ Adjust indentation?                 → xnip indent
└─ Apply 2+ edits atomically?          → xnip apply
```

---

See [SKILL.md](../SKILL.md) for command reference and safety options.
