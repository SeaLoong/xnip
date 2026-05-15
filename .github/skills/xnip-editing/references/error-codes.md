# xnip — Error Codes & Troubleshooting

## Exit Codes

| Code | Category | Meaning | Recovery |
|------|----------|---------|----------|
| `0` | ✅ Success | All operations completed successfully | N/A |
| `1` | ❌ User Error | Bad arguments, bad format, locator not found | Fix command syntax or manifest |
| `2` | ❌ IO Error | Permission denied, file not found, tmpfile creation failed | Check file permissions, paths |
| `3` | ❌ Validation Error | `--was` mismatch, `--check` failed | Review drift, retry with correct content |
| `4` | ⚠️ Partial Commit | Phase-2 rename incomplete (apply only) | Some files written, some not; manual recovery may be needed |

---

## Common Errors & Solutions

### 1. Locator Not Found

**Error:**
```
xnip: locator not found: --lines 999 in /path/to/file
```

**Causes:**
- File has fewer lines than specified
- Regex pattern doesn't match any line
- Off-by-one error in line numbers

**Solutions:**
```bash
# Check file line count
wc -l /path/to/file

# Preview lines around the target
xnip peek /path/to/file --lines 1-50

# Use regex to verify pattern matches
xnip find /path/to/file --pattern 'your_pattern'

# Correct the locator and retry
xnip replace /path/to/file --lines 42 --text 'new'
```

---

### 2. Format Auto-Detect Failed

**Error:**
```
xnip: auto-detect failed across native/json/yaml
```

**Causes:**
- Manifest file is malformed (all parsers failed)
- File extension not `.txt`, `.json`, or `.yaml`
- Content doesn't match any known format

**Solutions:**
```bash
# Specify format explicitly
xnip apply edits.txt --format native
xnip apply edits.json --format json
xnip apply edits.yaml --format yaml

# Validate JSON syntax
python -m json.tool edits.json > /dev/null

# Validate YAML syntax
yamllint edits.yaml

# For native format, ensure one operation per line:
# ✓ replace /path/to/file 42 "text"
# ✗ replace /path/to/file  (missing content)
```

---

### 3. Validation Failure (`--was` Mismatch)

**Error:**
```
xnip: validation failed: --was content mismatch at line 42
```

**Causes:**
- File has been modified since you prepared the command
- Another process changed the file
- Line numbers drifted due to previous edits

**Solutions:**
```bash
# Check current content
xnip peek /path/to/file --lines 42

# Re-read the correct `--was` value
xnip peek /path/to/file --lines 42 | head -1

# Update your command with the correct expected content
xnip replace /path/to/file --lines 42 \
  --text 'new content' \
  --was 'actual current content\n'

# Or disable drift detection (not recommended)
xnip replace /path/to/file --lines 42 --text 'new'
```

---

### 4. Permission Denied

**Error:**
```
xnip: permission denied (os error 13)
```

**Causes:**
- File is read-only
- Directory not writable (for tmpfile creation)
- Running with insufficient privileges

**Solutions:**
```bash
# Check file permissions
ls -l /path/to/file

# Make file writable
chmod +w /path/to/file

# Make directory writable (for tmpfile)
chmod +w $(dirname /path/to/file)

# Run with elevated privileges (if needed)
sudo xnip replace /path/to/file ...

# For apply, check tmpfile directory
# xnip creates tmpfiles in the same directory as the target
```

---

### 5. File Not Found

**Error:**
```
xnip: file not found: /path/to/nonexistent
```

**Causes:**
- Path typo
- File path not absolute
- File deleted between commands

**Solutions:**
```bash
# Verify file exists
test -f /path/to/file && echo "Found" || echo "Not found"

# Use absolute paths
xnip replace $(pwd)/myfile --lines 42 --text 'new'

# List directory to confirm
ls -la /path/to/
```

---

### 6. Line Content Mismatch (Regex/Between)

**Error:**
```
xnip: no match found for pattern '^const PORT'
```

**Causes:**
- Regex pattern is too strict
- Content has changed (whitespace, case)
- Pattern needs escaping

**Solutions:**
```bash
# Find what's actually there
xnip find /path/to/file --pattern 'PORT'

# Preview context
xnip peek /path/to/file --lines 1-50

# Use more flexible regex
xnip replace /path/to/file \
  --match-line '^\s*const PORT' \
  --text 'const PORT = 3000;'

# Test your regex in isolation
xnip find /path/to/file --pattern 'your_pattern'
```

---

### 7. Stdin-Related Issues

**Error:**
```
xnip: unexpected end of stdin
```

**Causes:**
- Not providing expected content via stdin
- Premature pipe closure
- Using `--text-stdin` but no pipe

**Solutions:**
```bash
# Provide content via pipe
echo 'new content' | xnip replace /path/to/file 42 --text-stdin

# Or use --text-file instead
xnip replace /path/to/file 42 --text-file snippet.txt

# For apply with stdin
xnip apply edits.txt < new_edits.txt

# Or use --from-stdin explicitly
xnip apply --from-stdin < edits.json
```

---

### 8. Partial Commit (Exit Code 4)

**Error:**
```
xnip: phase-2 partial commit detected (exit code 4)
```

**Causes:**
- apply succeeded in writing temp files (phase 1)
- Failed during atomic rename (phase 2) on some files
- Typically: file locked, permission changed, filesystem full

**Recovery:**
```bash
# Check which files were written
ls -la /path/to/file* | grep -E '\.bak|~' || true

# Manually verify state (some edits may have taken effect)
xnip peek /path/to/file --all

# Option 1: Retry the apply (if issue was transient)
xnip apply edits.json

# Option 2: Restore from backup and retry
mv /path/to/file.bak /path/to/file
xnip apply edits.json
```

---

### 9. Pattern Capture Groups in `--repl`

**Error:**
```
xnip: invalid capture group in --repl: $1 not found
```

**Causes:**
- Regex pattern has no capture groups
- Capture group index out of range

**Solutions:**
```bash
# ✓ Pattern with capture groups
xnip replace file.ts \
  --pattern 'const (\w+) = ' \
  --repl 'let $1 = '

# ✗ Pattern without groups → can't use $1
# xnip replace file.ts \
#   --pattern 'const' \
#   --repl 'let $1'  # ERROR: $1 doesn't exist

# Use literal replacement instead
xnip replace file.ts \
  --pattern 'const' \
  --repl 'let'
```

---

### 10. Manifest Syntax Errors (JSON/YAML)

**JSON Error:**
```
xnip: invalid json: expected value at line 5, column 3
```

**YAML Error:**
```
xnip: invalid yaml: mapping values are not allowed here
```

**Solutions:**
```bash
# Validate JSON
python -m json.tool edits.json

# Validate YAML
python -m yaml edits.yaml

# Common JSON issues:
# ✗ Trailing comma: [{"op": "replace"}, ]
# ✗ Unquoted keys: {op: replace}
# ✗ Single quotes inside double quotes: "text": "it's"

# Common YAML issues:
# ✗ Tabs instead of spaces
# ✗ Unquoted colons: text: some: value
# ✗ Inconsistent indentation

# Fix and retry with explicit format
xnip apply edits.json --format json
```

---

### 11. Dry-run Shows Nothing

**Issue:**
```bash
xnip replace file.ts --lines 42 --text 'new' --dry-run
# (no output)
```

**Cause:**
- Operation succeeded but there's no visible diff output
- Use `--check` to verify logic

**Solutions:**
```bash
# Verify with --check (validation only)
xnip apply edits.json --check

# Or run without --dry-run to see actual changes
xnip replace file.ts --lines 42 --text 'new' --was 'old\n'

# Check exit code (0 = success)
xnip replace file.ts --lines 42 --text 'new' --dry-run ; echo $?
```

---

### 12. Line Numbers Drift in apply

**Issue:**
```
apply started, but edits fail because line numbers changed mid-batch
```

**Causes:**
- Edits not sorted in descending line order
- File modified between apply phases

**Solutions:**
```bash
# When creating edits, sort by file, then descending line number
# ✓ Correct order (descending):
# replace /path/to/file 100 "new"
# replace /path/to/file 50 "old"
# replace /path/to/file 20 "middle"

# ✗ Wrong order (ascending will cause drift):
# replace /path/to/file 20 "middle"
# replace /path/to/file 50 "old"
# replace /path/to/file 100 "new"

# Tool sorts automatically, but if you're generating manifests, ensure order
```

---

## Quick Checklist

- ✅ File exists: `test -f /path && echo ok`
- ✅ File writable: `test -w /path && echo ok`
- ✅ Correct line count: `wc -l /path`
- ✅ Pattern matches: `xnip find /path --pattern 'regex'`
- ✅ Content unchanged: `xnip peek /path --lines N`
- ✅ Manifest valid: `xnip apply edits.json --check`
- ✅ Dry-run first: `xnip apply edits.json --dry-run`

---

## Contact & Debugging

For persistent issues:
1. Run with `--json` for structured error output: `xnip apply edits.json --json`
2. Check exit code: `echo $?` (Unix) or `$LASTEXITCODE` (PowerShell)
3. Run `xnip doctor` for diagnostic info
4. Refer to [xnip GitHub issues](https://github.com/SeaLoong/xnip/issues)

---

See [SKILL.md](../SKILL.md) for command reference and [examples.md](./examples.md) for usage patterns.
