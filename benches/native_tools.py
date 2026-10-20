#!/usr/bin/env python3
"""Paired execution of real native edit tools and xnip; never invokes a model."""
from __future__ import annotations
import argparse
import contextlib
import dataclasses
import difflib
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import shlex
import statistics
import itertools
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches"))
from edit_cost import Counter, Mcp, Scenario, scenarios, dump, execution_environment
from native.codex_adapter import discover as codex_discover, apply_patch as codex_apply
from native_report import write_report
from harness_backends import register_harnesses, policy_probe_cases
from report_privacy import fixture_session, public_report

FENCE = chr(96) * 3
TASKS = {
    "tiny-config": "Change the configured server port from 8080 to 3000, preserving other settings.",
    "large-local-change": "Set item_1000 to 42 and leave the other declarations unchanged.",
    "multiline-function": "Replace total's accumulator loop with sum(item.price for item in items).",
    "unicode": "Change the greeting to 高效编辑 🚀 while preserving the other Unicode text.",
    "crlf": "Set port to 3000 while preserving the original CRLF line endings.",
    "no-final-newline": "Set port to 3000 without adding a final newline.",
    "disambiguated-block": "Make second() return 1 while first() continues returning 0.",
    "global-replace": "Rename every old_name call to new_name.",
    "same-file-batch": "Set one to 10 and three to 30, leaving two unchanged.",
    "multi-file-batch": "Enable the feature, set timeout to 20 and retries to 3 in their existing files.",
    "long-line": "Change mode=old to mode=new while preserving the preceding data payload.",
    "insert-after-anchor": "Add import sys immediately after import os.",
    "delete-block": "Remove the obsolete comment and old_setting entry, preserving start and end.",
}


class Worker:
    def __init__(self, command):
        self.process = subprocess.Popen(command, cwd=ROOT, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf8",
            env=execution_environment(), bufsize=1)
        self.messages = queue.Queue()
        self.diagnostics = []
        def read():
            try:
                for line in self.process.stdout:
                    try:
                        self.messages.put(json.loads(line))
                    except json.JSONDecodeError:
                        self.diagnostics.append(line)
            finally:
                self.messages.put(EOFError("native adapter exited"))
        def errors():
            for line in self.process.stderr:
                self.diagnostics.append(line)
        self.reader = threading.Thread(target=read, daemon=True)
        self.errors = threading.Thread(target=errors, daemon=True)
        self.reader.start()
        self.errors.start()

    def request(self, value):
        self.process.stdin.write(dump(value) + "\n")
        self.process.stdin.flush()
        reply = self.messages.get(timeout=180)
        if isinstance(reply, BaseException):
            raise RuntimeError(str(reply) + ": " + "".join(self.diagnostics)[-2000:])
        return reply

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=5)
        self.reader.join(timeout=5)
        self.errors.join(timeout=5)
        for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
            stream.close()


def normalize(value, root):
    if isinstance(value, str):
        return value.replace(str(root), "/workspace").replace(str(root).replace("\\", "/"), "/workspace")
    if isinstance(value, list):
        return [normalize(item, root) for item in value]
    if isinstance(value, dict):
        return {key: normalize(item, root) for key, item in value.items()}
    return value


def grouped_changes(before, after):
    a, b = before.splitlines(), after.splitlines()
    return a, b, list(difflib.SequenceMatcher(a=a, b=b, autojunk=False).get_grouped_opcodes(3))


def patch_payload(files, expected):
    result = ["*** Begin Patch\n"]
    for name in sorted(files):
        if files[name] == expected[name]:
            continue
        a, b, groups = grouped_changes(files[name], expected[name])
        result.append(f"*** Update File: {name}\n")
        for group in groups:
            result.append("@@\n")
            for tag, i, j, k, l in group:
                if tag == "equal": result.extend(" " + line + "\n" for line in a[i:j])
                if tag in ("delete", "replace"): result.extend("-" + line + "\n" for line in a[i:j])
                if tag in ("insert", "replace"): result.extend("+" + line + "\n" for line in b[k:l])
            if group[-1][2] == len(a):
                result.append("*** End of File\n")
    result.append("*** End Patch\n")
    return "".join(result)


def aider_payload(files, expected, format):
    result = []
    for name in sorted(files):
        if files[name] == expected[name]:
            continue
        before, after = files[name].replace("\r\n", "\n"), expected[name].replace("\r\n", "\n")
        if format == "whole":
            result.append(f"{name}\n{FENCE}\n{after}" + ("" if after.endswith("\n") else "\n") + FENCE + "\n")
        elif format == "diff":
            a, b, groups = grouped_changes(before, after)
            for group in groups:
                old = "\n".join(a[group[0][1]:group[-1][2]]) + "\n"
                new = "\n".join(b[group[0][3]:group[-1][4]]) + "\n"
                result.append(f"{name}\n{FENCE}\n<<<<<<< SEARCH\n{old}=======\n{new}>>>>>>> REPLACE\n{FENCE}\n")
        else:
            # Actual aider UnifiedDiffCoder parses these normal unified hunks.
            diff = list(difflib.unified_diff(before.splitlines(), after.splitlines(),
                fromfile=name, tofile=name, n=3, lineterm=""))
            result.append(FENCE + "diff\n" + "\n".join(diff) + "\n" + FENCE + "\n")
    return "".join(result)


def contextual_edits(files, expected):
    """Construct the before/after 3-line context explicitly prescribed by Gemini."""
    result = []
    for name, before in files.items():
        a, b = before.splitlines(keepends=True), expected[name].splitlines(keepends=True)
        for group in difflib.SequenceMatcher(a=a, b=b, autojunk=False).get_grouped_opcodes(3):
            result.append({"file": name, "old": "".join(a[group[0][1]:group[-1][2]]),
                "text": "".join(b[group[0][3]:group[-1][4]])})
    return result


def probes():
    return [
        {"name": "ambiguous-line", "files": {"src/example.txt": "first\nsame\nmiddle\nsame\nlast\n"},
         "edit": {"file": "src/example.txt", "old": "same\n", "text": "changed\n"},
         "observation": "Does an ambiguous target reject, modify the first occurrence, or request model repair?"},
        {"name": "stale-context", "files": {"src/example.txt": "version=2\n"},
         "edit": {"file": "src/example.txt", "old": "version=1\n", "text": "version=3\n"},
         "observation": "The supplied old text is stale; record mutation/rejection without treating different contracts as bugs."},
        {"name": "whitespace-drift", "files": {"src/example.txt": "  value = 1\n"},
         "edit": {"file": "src/example.txt", "old": "\tvalue = 1\n", "text": "\tvalue = 2\n"},
         "observation": "Record whether native fallback matching accepts whitespace drift."},
        {"name": "overlapping-literal", "files": {"src/example.txt": "ababa\n"},
         "edit": {"file": "src/example.txt", "old": "aba", "text": "X"},
         "observation": "Overlapping substring ambiguity is meaningful only for substring edit interfaces."},
        {"name": "aider-accepted-unclosed-whole", "files": {"src/example.txt": "port=8080"},
         "expected": {"src/example.txt": "port=3000"},
         "edit": {"file": "src/example.txt", "old": "port=8080", "text": "port=3000"},
         "observation": "WholeFileCoder accepts an unclosed fence that preserves missing EOF; this is an alternate accepted encoding, not the canonical closed-fence profile."},
        {"name": "identical-old-new", "files": {"src/example.txt": "same\n"},
         "edit": {"file": "src/example.txt", "old": "same", "text": "same"},
         "observation": "Preserve native no-op success/rejection rather than normalizing it to another tool's policy."},
        {"name": "mixed-eol-preservation", "files": {"src/example.txt": "one=1\r\ntwo=2\nthree=3\r\n"},
         "expected": {"src/example.txt": "one=1\r\ntwo=20\nthree=3\r\n"},
         "edit": {"file": "src/example.txt", "old": "two=2", "text": "two=20"},
         "observation": "Observe whole-file EOL normalization separately from byte-preserving edits."},
        {"name": "ordered-same-file-dependency", "files": {"src/example.txt": "start\n"},
         "expected": {"src/example.txt": "done\n"},
         "edits": [{"file": "src/example.txt", "old": "start", "text": "middle"},
                   {"file": "src/example.txt", "old": "middle", "text": "done"}],
         "observation": "Original-coordinate native multi-edit and ordered xnip batches are distinct contracts."},
        {"name": "late-batch-failure", "files": {"a": "old\n", "b": "keep\n"},
         "edits": [{"file": "a", "old": "old", "text": "new"},
                   {"file": "b", "old": "missing", "text": "bad"}],
         "observation": "Keep actual earlier writes when separate native calls fail later; do not infer a cross-file transaction."},
    ]


def reset_files(root, files):
    for name, content in files.items():
        dest = root / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(content.encode("utf8"))


def actual_files(root, files):
    # Include unexpected files as well as missing expected paths.
    names = set(files) | {path.relative_to(root).as_posix() for path in root.rglob("*") if path.is_file()}
    return {name: (root / name).read_bytes() if (root / name).is_file() else None for name in sorted(names)}


def observe(actual, expected):
    original = {name: value.encode("utf8") for name, value in expected.items()}
    exact = actual == original
    normalized = set(actual) == set(expected) and all(actual.get(name) is not None and
        actual[name].decode("utf8", errors="replace").replace("\r\n", "\n") == text.replace("\r\n", "\n")
        for name, text in expected.items())
    return {"byte_exact": exact, "lf_normalized_equal": normalized,
            "files": {name: {"sha256": hashlib.sha256(data).hexdigest() if data is not None else None,
                "bytes": len(data) if data is not None else None,
                "utf8": data.decode("utf8", errors="replace") if data is not None else None}
                for name, data in actual.items()}}


def unique_groups(before, after, context):
    a, b = before.splitlines(), after.splitlines()
    groups = list(difflib.SequenceMatcher(a=a, b=b, autojunk=False).get_grouped_opcodes(context))
    for group in groups:
        old = a[group[0][1]:group[-1][2]]
        if not old or sum(a[i:i + len(old)] == old for i in range(len(a) - len(old) + 1)) != 1:
            return None
    return a, b, groups


def unique_payload(files, expected, kind, counter):
    """Choose known-input unique context; never execute candidates to pick one.

    Enumerate symmetric 0..3-line context, then widen only if no unique option
    exists. This is the best observed encoding in that candidate set, not a
    proof of the globally shortest native representation.
    """
    sections = []
    for name in sorted(files):
        if files[name] == expected[name]:
            continue
        before = files[name].replace("\r\n", "\n")
        after = expected[name].replace("\r\n", "\n")
        candidates = []
        limit = len(before.splitlines())
        for n in range(max(3, limit) + 1):
            grouped = unique_groups(before, after, n)
            if grouped is None:
                continue
            a, b, groups = grouped
            parts = [f"*** Update File: {name}\n"] if kind == "patch" else []
            for group in groups:
                if kind == "patch":
                    parts.append("@@\n")
                    for tag, i, j, k, l in group:
                        if tag == "equal": parts.extend(" " + line + "\n" for line in a[i:j])
                        if tag in ("delete", "replace"): parts.extend("-" + line + "\n" for line in a[i:j])
                        if tag in ("insert", "replace"): parts.extend("+" + line + "\n" for line in b[k:l])
                    if group[-1][2] == len(a):
                        parts.append("*** End of File\n")
                else:
                    old = "\n".join(a[group[0][1]:group[-1][2]]) + "\n"
                    new = "\n".join(b[group[0][3]:group[-1][4]]) + "\n"
                    parts.append(f"{name}\n{FENCE}\n<<<<<<< SEARCH\n{old}=======\n{new}>>>>>>> REPLACE\n{FENCE}\n")
            payload = "".join(parts)
            candidates.append((counter.measure(payload)[counter.metadata["unit"]], n, payload))
            if n >= 3:
                break
        if not candidates:
            raise ValueError(f"no uniquely anchored {kind} encoding for {name}")
        sections.append(min(candidates)[2])
    body = "".join(sections)
    return "*** Begin Patch\n" + body + "*** End Patch\n" if kind == "patch" else body


def eof_aware_whole_payload(files, expected):
    changed = [(name, expected[name].replace("\r\n", "\n")) for name in sorted(files)
               if files[name] != expected[name]]
    parts = []
    for index, (name, after) in enumerate(changed):
        parts.append(f"{name}\n{FENCE}\n{after}")
        # WholeFileCoder uses a non-final response accessor that rstrips an
        # unclosed tail. Close newline-terminated goals; leave the last fence
        # open only for a known unterminated goal. No output trial chooses this.
        if index + 1 < len(changed) or after != after.rstrip():
            parts.append(("" if after.endswith("\n") else "\n") + FENCE + "\n")
    return "".join(parts)


def native_quote(value, compact=False, escaped=False):
    """Equivalent native encodings; bare tokens avoid reserved locator/payload syntax."""
    if compact and re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_./:=+-]*", value) and not value.startswith(("s/", "=/", "~/")):
        return value
    escapes = {"\\": "\\\\", '"': '\\"', "\t": "\\t"}
    if escaped:
        escapes.update({"\n": "\\n", "\r": "\\r"})
    return '"' + "".join(escapes.get(char, char) for char in value) + '"'


def argument_cost(counter, calls):
    return counter.total(value if isinstance(value, str) else dump(value)
                         for _, value in calls)


def choose_plan(plans, counter):
    if not plans:
        raise ValueError("no valid edit plan")
    measured = []
    unit = counter.metadata["unit"]
    for plan in plans:
        cost = argument_cost(counter, plan["calls"])
        encoded = dump(plan["calls"]).encode("utf8")
        measured.append({"name": plan["name"], "input_cost": cost,
                         "calls": len(plan["calls"]), "contract": plan["contract"],
                         "input_sha256": hashlib.sha256(encoded).hexdigest()})
    index = min(range(len(plans)), key=lambda i: (
        measured[i]["input_cost"][unit], len(plans[i]["calls"]),
        plans[i]["contract"] != "guarded", plans[i]["name"]))
    return plans[index], {
        "rule": "minimum serialized input cost before execution; ties prefer fewer calls, then guarded, then name",
        "counter": counter.metadata, "selected": plans[index]["name"],
        "selected_contract": plans[index]["contract"], "candidates": measured,
        "uses_observed_output_to_select": False,
    }


def relative_edits(edits):
    return [{**edit, "file": Path(edit["file"]).as_posix()} for edit in edits]


def grouped_pairs(edits):
    if any(edit.get("all") for edit in edits):
        return None
    result = {}
    for edit in edits:
        result.setdefault(edit["file"], []).append([edit["old"], edit["text"]])
    return result


def _unique_literal(content, old):
    first = content.find(old) if old else -1
    return first >= 0 and content.find(old, first + 1) < 0


def _trim_offsets(limit):
    # Enumerate small blocks completely; bound work for large shared contexts.
    values = set(range(min(limit, 32) + 1)) | {limit}
    step = 1
    while step < limit:
        values.add(max(0, limit - step))
        step *= 2
    return sorted(values)


def trim_snapshot_edits(files, expected, edits, counter=None):
    """Use only supplied complete snapshots; preserve ordered virtual edits.

    Common prefix/suffix trims retain a nonempty, overlap-unique old substring.
    Candidate pair encodings are scored before execution; large shared contexts
    use bounded offsets, so this is a known-input heuristic, not a global minimum.
    Explicit all replacements keep their original old text and matching policy.
    """
    counter = counter or Counter(None)
    current, result = dict(files), []
    for edit in edits:
        name, old, new = edit["file"], edit["old"], edit["text"]
        if name not in current or not old:
            return None
        before = current[name]
        if edit.get("all"):
            if old not in before:
                return None
            chosen = dict(edit)
            after = before.replace(old, new)
        else:
            if not _unique_literal(before, old):
                return None
            prefix = 0
            while prefix < min(len(old), len(new)) and old[prefix] == new[prefix]:
                prefix += 1
            suffix = 0
            while suffix < min(len(old), len(new)) and old[-suffix - 1] == new[-suffix - 1]:
                suffix += 1
            choices = []
            for left in _trim_offsets(prefix):
                for right in _trim_offsets(suffix):
                    if left + right >= len(old) or left + right > len(new):
                        continue
                    a = old[left:len(old) - right if right else len(old)]
                    b = new[left:len(new) - right if right else len(new)]
                    if _unique_literal(before, a):
                        cost = counter.measure(dump([a, b]))[counter.metadata["unit"]]
                        choices.append((cost, len(a) + len(b), a, b))
            if not choices:
                return None
            _, _, a, b = min(choices)
            chosen = {**edit, "old": a, "text": b}
            after = before.replace(a, b, 1)
            if after != before.replace(old, new, 1):
                return None
        result.append(chosen)
        current[name] = after
    return result if current == expected else None


def _lf_chunks(text):
    parts = text.split("\n")
    return [part + "\n" for part in parts[:-1]] + ([parts[-1]] if parts[-1] else [])


def _line_splice(content, start, end, payload):
    chunks = _lf_chunks(content)
    if start == end:
        prefix = "".join(chunks[:start])
        if start == len(chunks) and payload and prefix and not prefix.endswith("\n"):
            prefix += "\n"
        normalized = payload + ("\n" if payload and not payload.endswith("\n") else "")
        return prefix + normalized + "".join(chunks[start:])
    normalized = payload + ("\n" if payload and not payload.endswith("\n")
                            and chunks[end - 1].endswith("\n") else "")
    return "".join(chunks[:start]) + normalized + "".join(chunks[end:])


def snapshot_line_plans(files, expected):
    """Existing native line operations, validated against the complete goal.

    Source coordinates are grouped and emitted descending. LF-only chunks match
    Rust; normalization is simulated before admitting a candidate, including CRLF,
    empty replacements, and unterminated EOF. No editor is run to choose a plan.
    """
    if set(files) != set(expected):
        return []
    plans, seen = [], set()
    for context, compact, shorten in itertools.product((0, 1, 3), (False, True), (False, True)):
        records, valid = [], True
        for name, before in files.items():
            after = expected[name]
            if before == after:
                continue
            a, b = _lf_chunks(before), _lf_chunks(after)
            groups = list(difflib.SequenceMatcher(a=a, b=b, autojunk=False).get_grouped_opcodes(context))
            virtual = before
            for group in reversed(groups):
                start, end = group[0][1], group[-1][2]
                payload = "".join(b[group[0][3]:group[-1][4]])
                if shorten and payload.endswith("\n"):
                    shorter = payload[:-1]
                    if _line_splice(virtual, start, end, shorter) == _line_splice(virtual, start, end, payload):
                        payload = shorter
                virtual = _line_splice(virtual, start, end, payload)
                quote = lambda value: native_quote(value, compact=compact)
                if start == end:
                    anchor = str(start if start else 1)
                    where = " before" if not start and a else ""
                    records.append("insert " + quote(name) + " " + anchor + where + " " + native_quote(payload))
                else:
                    lines = str(start + 1) if end == start + 1 else f"{start + 1}-{end}"
                    records.append("replace " + quote(name) + " " + lines + " " + native_quote(payload))
            if virtual != after:
                valid = False
                break
        program = "\n".join(records)
        if valid and program and program not in seen:
            seen.add(program)
            plans.append({"name": f"snapshot-lines-context{context}-bare{int(compact)}-short{int(shorten)}",
                          "calls": [("xnip_apply", {"manifest_text": program})], "contract": "snapshot"})
    return plans


def mcp_plans(files, expected, edits, allow_snapshot=False, probe=False, counter=None):
    edits = relative_edits(edits)
    exact = [("xnip_edit", edit) for edit in edits]
    plans = [{"name": "literal-calls", "calls": exact, "contract": "guarded"}]
    # A guard probe must test its supplied old bytes; no regenerated full-file
    # context, insert conversion or unconditional write can bypass that check.
    if probe:
        return plans
    plans.append({"name": "direct-edits", "calls": [("xnip_apply", {"edits": edits})],
                  "contract": "guarded"})
    pairs = grouped_pairs(edits)
    if pairs:
        plans.append({"name": "file-pairs", "calls": [("xnip_apply", {"files": pairs})],
                      "contract": "guarded"})
    inserted = []
    for edit in edits:
        if not edit.get("all") and edit["text"].startswith(edit["old"]) and edit["old"]:
            inserted.append(("xnip_edit", {**edit, "text": edit["text"][len(edit["old"]):],
                                          "position": "after"}))
        elif not edit.get("all") and edit["text"].endswith(edit["old"]) and edit["old"]:
            inserted.append(("xnip_edit", {**edit, "text": edit["text"][:-len(edit["old"])],
                                          "position": "before"}))
        else:
            inserted.append(("xnip_edit", edit))
    if any("position" in arguments for _, arguments in inserted):
        plans.append({"name": "literal-anchor-insertions", "calls": inserted, "contract": "guarded"})
    changed = {name: text for name, text in expected.items() if files[name] != text}
    guarded = {name: {"old": files[name], "text": text} for name, text in changed.items()}
    plans += [
        {"name": "guarded-whole-calls", "contract": "guarded",
         "calls": [("xnip_edit", {"file": name, "operation": "write", "old": files[name], "text": text})
                   for name, text in changed.items()]},
        {"name": "guarded-whole-map", "contract": "guarded",
         "calls": [("xnip_apply", {"files": guarded})]},
    ]
    if allow_snapshot:
        trimmed = trim_snapshot_edits(files, expected, edits, counter)
        if trimmed is not None:
            plans += [
                {"name": "snapshot-trimmed-literal-calls", "contract": "guarded",
                 "calls": [("xnip_edit", edit) for edit in trimmed]},
                {"name": "snapshot-trimmed-direct-edits", "contract": "guarded",
                 "calls": [("xnip_apply", {"edits": trimmed})]},
            ]
            trimmed_pairs = grouped_pairs(trimmed)
            if trimmed_pairs:
                plans.append({"name": "snapshot-trimmed-file-pairs", "contract": "guarded",
                              "calls": [("xnip_apply", {"files": trimmed_pairs})]})
        plans.extend(snapshot_line_plans(files, expected))
        plans += [
            {"name": "snapshot-whole-map", "calls": [("xnip_apply", {"files": changed})],
             "contract": "snapshot"},
            {"name": "snapshot-whole-calls", "contract": "snapshot",
             "calls": [("xnip_edit", {"file": name, "operation": "write", "text": text})
                       for name, text in changed.items()]},
        ]
        if pairs:
            names = list(changed)
            for mask in itertools.product((False, True), repeat=len(names)):
                if not any(mask) or all(mask):
                    continue
                entries = {name: changed[name] if whole else pairs[name]
                           for name, whole in zip(names, mask)}
                plans.append({"name": "mixed-map-" + "".join("w" if item else "e" for item in mask),
                              "calls": [("xnip_apply", {"files": entries})], "contract": "snapshot"})
    seen = {dump(plan["calls"]) for plan in plans}
    for plan in cli_plans(files, expected, edits, allow_snapshot, format="native", counter=counter):
        # CLI constructors append a record LF after the closing payload token.
        # Removing that one delimiter preserves payload LF/EOF and all guards.
        for final_lf in (True, False):
            calls = [("xnip_apply", {"manifest_text": program if final_lf else program.removesuffix("\n")})
                     for _, program in plan["calls"]]
            encoded = dump(calls)
            if encoded not in seen:
                seen.add(encoded)
                plans.append({"name": "native-program:" + plan["name"] + (":record-lf" if final_lf else ":record-eof"),
                              "calls": calls, "contract": plan["contract"]})
    return plans


def cli_plans(files, expected, edits, allow_snapshot=False, probe=False, format="native", encoding=None, counter=None):
    edits = relative_edits(edits)
    if format == "compact":
        plans = []
        pairs = grouped_pairs(edits)
        if pairs:
            plans.append({"name": "compact-file-pairs", "calls": [(format, dump(pairs))],
                          "contract": "guarded"})
        # Probe old bytes must survive untouched. With an unsupported global
        # count, native literal input is the guarded fallback.
        if probe:
            return plans or cli_plans(files, expected, edits, False, True, "native", counter=counter)
        changed = {name: text for name, text in expected.items() if files[name] != text}
        plans.append({"name": "compact-whole-guarded", "contract": "guarded",
                      "calls": [(format, dump({name: {"old": files[name], "text": text}
                                              for name, text in changed.items()}))]})
        if allow_snapshot:
            plans.append({"name": "compact-whole-snapshot", "contract": "snapshot",
                          "calls": [(format, dump(changed))]})
            trimmed = trim_snapshot_edits(files, expected, edits, counter)
            trimmed_pairs = grouped_pairs(trimmed) if trimmed is not None else None
            if trimmed_pairs:
                plans.append({"name": "snapshot-trimmed-compact-file-pairs", "contract": "guarded",
                              "calls": [(format, dump(trimmed_pairs))]})
        return plans
    if encoding is None:
        variants = []
        for style in ("quoted-multiline", "quoted-escaped", "bare-multiline", "bare-escaped"):
            for plan in cli_plans(files, expected, edits, allow_snapshot, probe, format, style, counter):
                variants.append({**plan, "name": plan["name"] + ":" + style})
        if allow_snapshot and not probe:
            trimmed = trim_snapshot_edits(files, expected, edits, counter)
            if trimmed is not None:
                for compact in (False, True):
                    quote = lambda value: native_quote(value, compact=compact)
                    program = "\n".join("edit " + quote(edit["file"]) + " " + quote(edit["old"]) +
                        " " + quote(edit["text"]) + (" all" if edit.get("all") else "") for edit in trimmed)
                    variants.append({"name": f"snapshot-trimmed-native-bare{int(compact)}",
                                     "contract": "guarded", "calls": [("native", program)]})
            for plan in snapshot_line_plans(files, expected):
                variants.append({**plan, "calls": [("native", args["manifest_text"]) for _, args in plan["calls"]]})
        return variants
    quote = lambda value: native_quote(value, compact=encoding.startswith("bare"), escaped=encoding.endswith("escaped"))
    literal = "".join("edit " + quote(edit["file"]) + " " + quote(edit["old"]) +
                      " " + quote(edit["text"]) + (" all" if edit.get("all") else "") + "\n"
                      for edit in edits)
    plans = [{"name": "native-literal-program", "calls": [("native", literal)], "contract": "guarded"}]
    if probe:
        return plans
    changed = {name: text for name, text in expected.items() if files[name] != text}
    choices = []
    for name, text in changed.items():
        original = [edit for edit in edits if edit["file"] == name]
        block = "".join("edit " + quote(name) + " " + quote(edit["old"]) +
                        " " + quote(edit["text"]) + (" all" if edit.get("all") else "") + "\n"
                        for edit in original)
        guarded = "write " + quote(name) + " " + quote(text) + " was=" + quote(files[name]) + "\n"
        variants = [("edit", block, "guarded"), ("write-guarded", guarded, "guarded")]
        if allow_snapshot:
            variants.append(("write-snapshot", "write " + quote(name) + " " + quote(text) + "\n", "snapshot"))
        choices.append(variants)
    for combination in itertools.product(*choices):
        plans.append({"name": "native-" + "-".join(item[0] for item in combination),
                      "calls": [("native", "".join(item[1] for item in combination))],
                      "contract": "snapshot" if any(item[2] == "snapshot" for item in combination) else "guarded"})
    return plans


def mcp_call(worker, tool, arguments):
    start = time.perf_counter()
    reply = worker.call(tool, arguments)
    elapsed = (time.perf_counter() - start) * 1000
    returned = "\n".join(item.get("text", "") for item in reply.get("content", []))
    if "jsonrpc_error" in reply:
        returned = dump(reply["jsonrpc_error"])
    return {"tool": tool, "input": arguments, "native_result": reply,
            "returned_text": returned, "model_visible_result": None,
            "ok": not (reply.get("isError") or reply.get("jsonrpc_error")),
            "elapsed_ms": elapsed, "timing_scope": "warm MCP call; initialization separate",
            "serialized_tool_request": getattr(worker, "last_request", None),
            "serialized_mcp_result": getattr(worker, "last_response", None)}


def native_literal_arguments(mode, root, edit, purpose, current):
    relative = "relative" in mode
    name = edit["file"] if relative else str(root / edit["file"])
    if mode.startswith("gemini"):
        args = {"file_path": name, "old_string": edit["old"],
                "new_string": edit["text"], "instruction": purpose}
        if edit.get("all"):
            args["expected_replacements"] = current[edit["file"]].count(edit["old"])
    else:
        args = {"filePath": name, "oldString": edit["old"], "newString": edit["text"]}
        if edit.get("all"):
            args["replaceAll"] = True
    return args


class Backend:
    def __init__(self, name, family, mode, metadata, worker=None, format=None, line_endings=None, counter=None):
        self.name, self.family, self.mode = name, family, mode
        self.metadata, self.worker = metadata, worker
        self.format, self.line_endings = format, line_endings
        self.counter = counter or Counter(None)
        self.receipt_mode = metadata.get("receipt_mode", "structured") if family == "xnip" else None
        self.contract_cohort = "snapshot" if "snapshot" in mode or format == "whole" or mode.endswith("-trim") else "guarded"
        self.transport = "mcp" if mode == "xnip" or mode.startswith("xnip-mcp") else (
            "cli-stdin" if mode.startswith("xnip-cli") or mode.startswith("codex") else "native-function")

    def execute(self, root, files, expected, edits, purpose, probe=False):
        calls = []
        if self.mode.startswith("xnip-mcp"):
            if self.mode == "xnip-mcp-relative":
                plans = mcp_plans(files, expected, edits, probe=True)
            else:
                plans = mcp_plans(files, expected, edits, "snapshot" in self.mode, probe, self.counter)
            selected, selection = choose_plan(plans, self.counter)
            started = time.perf_counter()
            with Mcp(Path(self.metadata["executable"]).resolve(), root, receipts=self.receipt_mode) as worker:
                setup_ms = (time.perf_counter() - started) * 1000
                for tool, arguments in selected["calls"]:
                    call = mcp_call(worker, tool, arguments)
                    if not calls:
                        call["selection"] = selection
                        call["setup_elapsed_ms"] = setup_ms
                        call["workspace_context"] = {"cwd": str(root), "relative_paths": True}
                    calls.append(call)
                    if not call["ok"]:
                        break
            return calls
        if self.mode.startswith("xnip-cli"):
            format = "compact" if "compact" in self.mode else "native"
            plans = cli_plans(files, expected, edits, "snapshot" in self.mode, probe, format, counter=self.counter)
            selected, selection = choose_plan(plans, self.counter)
            for format, program in selected["calls"]:
                command = [str(Path(self.metadata["executable"]).resolve()), "apply",
                           "--format", format, "--from-stdin"]
                start = time.perf_counter()
                result = subprocess.run(command, input=program.encode("utf8"), cwd=root,
                                        capture_output=True, timeout=30, env=execution_environment())
                elapsed = (time.perf_counter() - start) * 1000
                reply = {"returncode": result.returncode, "stdout": result.stdout.decode("utf8"),
                         "stderr": result.stderr.decode("utf8")}
                invocation = subprocess.list2cmdline(command) if os.name == "nt" else shlex.join(command)
                calls.append({"tool": "apply-" + format, "input": program, "native_result": reply,
                    "returned_text": reply["stdout"] + reply["stderr"], "model_visible_result": None,
                    "ok": result.returncode == 0, "elapsed_ms": elapsed,
                    "timing_scope": "cold CLI process including startup; not ranked against warm MCP",
                    "selection": selection, "invocation": invocation, "invocation_argv": command,
                    "workspace_context": {"cwd": str(root), "relative_paths": True},
                    "transport": "stdin; complete raw program counted; invocation counted separately"})
            return calls
        if self.mode in ("gemini-context", "gemini-context-relative") and not probe:
            edits = contextual_edits(files, expected)
        known_file_selection = None
        if self.mode.endswith("-trim") and not probe:
            trimmed = trim_snapshot_edits(files, expected, edits, self.counter)
            candidates = [("known-file-original", edits)]
            if trimmed is not None:
                candidates.append(("known-file-trimmed", trimmed))
            plans = []
            for name, candidate in candidates:
                virtual, planned = dict(files), []
                for edit in candidate:
                    args = native_literal_arguments(self.mode, root, edit, purpose, virtual)
                    planned.append(("replace" if self.mode.startswith("gemini") else "edit", args))
                    virtual[edit["file"]] = virtual[edit["file"]].replace(
                        edit["old"], edit["text"], -1 if edit.get("all") else 1)
                plans.append({"name": name, "contract": "guarded", "calls": planned})
            selected, known_file_selection = choose_plan(plans, self.counter)
            edits = dict(candidates)[selected["name"]]
        patch = None
        if self.mode in ("codex", "codex-unique", "opencode-patch", "opencode-patch-unique"):
            if probe:
                edit = edits[0]
                patch = ("*** Begin Patch\n*** Update File: " + edit["file"] + "\n@@\n" +
                    "".join("-" + line + "\n" for line in edit["old"].splitlines()) +
                    "".join("+" + line + "\n" for line in edit["text"].splitlines()) + "*** End Patch\n")
            else:
                patch = unique_payload(files, expected, "patch", self.counter) if "unique" in self.mode else patch_payload(files, expected)
        if self.mode in ("codex", "codex-unique"):
            reply = codex_apply(root, patch, self.metadata["executable"])
            ok = reply["returncode"] == 0 and not reply["transport_error"]
            calls.append({"tool": "apply_patch", "input": patch, "native_result": reply,
                "returned_text": reply["stdout"] + reply["stderr"], "model_visible_result": None,
                "ok": ok, "elapsed_ms": reply["elapsed_ms"], "timing_scope": "native CLI process including startup"})
        elif self.mode.startswith("aider"):
            if probe and self.format == "whole":
                edit = edits[0]
                response = f'{edit["file"]}\n{FENCE}\n{edit["text"]}'
            elif probe:
                edit = edits[0]
                response = f'{edit["file"]}\n{FENCE}\n<<<<<<< SEARCH\n{edit["old"]}=======\n{edit["text"]}>>>>>>> REPLACE\n{FENCE}\n'
            elif self.mode == "aider-unique":
                response = unique_payload(files, expected, "aider", self.counter)
            elif self.mode == "aider-eof-aware":
                response = eof_aware_whole_payload(files, expected)
            else:
                response = aider_payload(files, expected, self.format)
            reply = self.worker.request({"action": "execute", "format": self.format,
                "root": str(root), "response": response, "chat_files": list(files),
                "line_endings": self.line_endings})
            if reply.get("adapter_error"): raise RuntimeError(dump(reply["adapter_error"]))
            calls.append({"tool": self.format, "input": response, "native_result": reply,
                "returned_text": reply.get("stdout", "") + reply.get("stderr", "") + reply.get("io_output", "") + (reply.get("native_error") or {}).get("message", ""),
                "model_visible_result": None, "ok": reply.get("ok", False),
                "elapsed_ms": reply.get("edit_elapsed_ms"), "timing_scope": "warm native get_edits/apply_edits; setup separate"})
        elif self.mode in ("opencode-patch", "opencode-patch-unique"):
            args = {"patchText": patch}
            reply = self.worker.request({"action": "execute", "tool": "apply_patch", "root": str(root), "arguments": args})
            if reply.get("adapter_error") or reply.get("status") == "unavailable":
                raise RuntimeError("adapter failure: " + dump(reply))
            calls.append({"tool": "apply_patch", "input": args, "native_result": reply,
                "returned_text": (reply.get("output") or reply.get("error", "")),
                "model_visible_result": reply.get("output"), "ok": reply.get("ok", False),
                "elapsed_ms": reply.get("elapsed_ms"), "timing_scope": "warm native tool execution"})
        else:
            current = dict(files)
            for edit in edits:
                if self.mode == "xnip":
                    args = {**edit, "file": str(root / edit["file"])}
                    call = mcp_call(self.worker, "xnip_edit", args)
                    calls.append(call)
                    if not call["ok"]:
                        break
                    current[edit["file"]] = (root / edit["file"]).read_bytes().decode("utf8")
                    continue
                elif self.mode.startswith("gemini"):
                    args = native_literal_arguments(self.mode, root, edit, purpose, current)
                    reply = self.worker.request({"action": "execute", "root": str(root), "arguments": args})
                    elapsed, ok = reply.get("elapsed_ms"), reply.get("ok", False)
                    text = reply.get("model_visible_result") or reply.get("error", "")
                    visible = reply.get("model_visible_result")
                else:
                    args = native_literal_arguments(self.mode, root, edit, purpose, current)
                    reply = self.worker.request({"action": "execute", "root": str(root), "arguments": args})
                    elapsed, ok = reply.get("elapsed_ms"), reply.get("ok", False)
                    text, visible = (reply.get("output") or reply.get("error", "")), reply.get("output")
                if reply.get("adapter_error") or reply.get("status") == "unavailable":
                    raise RuntimeError("adapter failure: " + dump(reply))
                tool_name = "xnip_edit" if self.mode == "xnip" else "replace" if self.mode.startswith("gemini") else "edit"
                calls.append({"tool": tool_name, "input": args, "native_result": reply,
                    "returned_text": text, "model_visible_result": visible, "ok": ok,
                    "elapsed_ms": elapsed, "timing_scope": "warm tool call; worker initialization excluded"})
                if known_file_selection is not None and len(calls) == 1:
                    calls[0]["selection"] = known_file_selection
                if not ok: break
                current[edit["file"]] = (root / edit["file"]).read_bytes().decode("utf8")
        return calls


def record(backend, root, case, counter, repeats=1, probe=False):
    files = case["files"]
    expected = case.get("expected", files)
    edits = case["edits"]
    outcomes, timings, setup_timings, first = [], [], [], None
    for _ in range(repeats):
        with tempfile.TemporaryDirectory(prefix="case-", dir=root) as temporary:
            workspace = Path(temporary)
            reset_files(workspace, files)
            try:
                calls = (backend.policy_probe(workspace, case) if case.get("policy_probe") else
                         backend.execute(workspace, files, expected, edits, case["purpose"], probe))
                actual = actual_files(workspace, files)
            finally:
                reset = getattr(backend, "reset_case", None)
                if reset is not None:
                    reset(workspace)
            result = observe(actual, expected)
            result["native_reported_ok"] = all(call["ok"] for call in calls)
            result["unchanged"] = actual == {name: data.encode("utf8") for name, data in files.items()}
            outcomes.append({**{key: value for key, value in result.items() if key != "files"},
                "file_hashes": {name: item["sha256"] for name, item in result["files"].items()}})
            timings.append(sum(call["elapsed_ms"] or 0 for call in calls))
            setup_timings.append(sum(call.get("setup_elapsed_ms", 0) for call in calls))
            if first is None:
                first = {"calls": normalize(calls, workspace), "raw_calls": calls, "outcome": result}
    calls = first["calls"]
    input_cost = counter.total(call["input"] if isinstance(call["input"], str) else dump(call["input"]) for call in calls)
    output_cost = counter.total(call["returned_text"] if isinstance(call["returned_text"], str) else dump(call["returned_text"]) for call in calls)
    repair = [request for call in calls for request in call["native_result"].get("model_requests_blocked", [])]
    selection = next((call["selection"] for call in calls if "selection" in call), None)
    required_probe = probe and case.get("requires_old_guard", True)
    selected_contract = (selection["selected_contract"] if selection else
                         "guarded" if required_probe else backend.contract_cohort)
    operation = selection["selected"] if selection else getattr(backend, "operation", backend.mode)
    if required_probe:
        preconditions = "original supplied old bytes"
    elif selected_contract == "guarded":
        preconditions = "old block/anchor or whole-file guard"
    elif operation.startswith("snapshot-lines"):
        preconditions = "known original coordinates; no supplied old guard"
    else:
        preconditions = "known complete final file; explicit whole write without old guard"
    host_setup = next((call["host_setup"] for call in calls if "host_setup" in call), None)
    def measure_optional(value):
        return counter.measure(value if isinstance(value, str) else dump(value)) if value is not None else None
    visible_known = all(call.get("returned_text_boundary_known",
        getattr(backend, "returned_text_boundary_known", backend.family != "aider")) for call in calls)
    host_layers = None
    if host_setup is not None:
        raw_results = [call.get("raw_mcp_tool_result") for call in calls]
        actual_values = [call["native_result"].get("native_result", call["native_result"].get("result")) for call in calls]
        host_layers = {
            "api_tool_catalog": measure_optional(host_setup.get("tool_definitions")),
            "model_schema_projection": measure_optional(host_setup.get("model_schema_projection")),
            "prompt_contributions": measure_optional(host_setup.get("prompt")),
            "server_instructions": measure_optional(host_setup.get("server_instructions")),
            "public_name_arguments_projection": counter.total(dump(call.get("model_tool_input_projection",
                {"name": call["tool"], "arguments": call["input"]})) for call in calls)
                if backend.family != "goose" else None,
            "model_rendered_content": output_cost if visible_known else None,
            "raw_mcp_tool_result": counter.total(dump(value) for value in raw_results)
                if all(value is not None for value in raw_results) else None,
            "internal_native_dto": counter.total(dump(value) for value in actual_values)
                if all(value is not None for value in actual_values) else None,
            "full_jsonrpc_reply": None,
            "scope": "independent captured layers; provider envelopes/billing unmeasured; never sum overlapping layers",
        }
    return {"backend": backend.name, "family": backend.family, "repeats": repeats,
        "native_reported_ok": all(outcome["native_reported_ok"] for outcome in outcomes),
        "outcomes": outcomes, "stable_outcome": all(item == outcomes[0] for item in outcomes),
        "input_cost": input_cost, "returned_text_cost": output_cost,
        "adapter_return_cost": counter.total(dump(call["native_result"]) for call in calls),
        "contract_cohort": "guarded" if probe and case.get("requires_old_guard", True) else backend.contract_cohort,
        "transport": backend.transport,
        "receipt_mode": backend.receipt_mode if backend.transport in {"mcp", "host-mcp-bridge"} else None,
        "returned_text_boundary_known": all(call.get("returned_text_boundary_known",
            getattr(backend, "returned_text_boundary_known", backend.family != "aider")) for call in calls),
        "frontier_eligible": getattr(backend, "frontier_eligible", True),
        "frontier_exclusion": getattr(backend, "frontier_exclusion", None),
        "frontier_group": getattr(backend, "frontier_group", "main"),
        "policy_profile": getattr(backend, "policy_profile", "legacy-native-kernel"),
        "input_conformance": getattr(backend, "input_conformance", "see profile contract"),
        "path_configuration": getattr(backend, "path_configuration", "see profile contract"),
        "source_provenance": {"metadata_key": backend.family,
            "commit": getattr(backend, "metadata", {}).get("commit", getattr(backend, "metadata", {}).get("revision")),
            "execution_level": getattr(backend, "metadata", {}).get("execution_level")},
        "read_call_count": sum(call.get("role") == "required-read" for call in calls),
        "edit_call_count": sum(call.get("role", "edit") == "edit" for call in calls),
        "required_read_input_cost": counter.total(dump(call["input"]) for call in calls if call.get("role") == "required-read"),
        "required_read_return_cost": counter.total(call["returned_text"] for call in calls if call.get("role") == "required-read"),
        "host_setup": host_setup, "host_layer_costs": host_layers,
        "full_serialized_mcp_cost": counter.total(dump(call["serialized_mcp_result"]) for call in calls) if calls and all(call.get("serialized_mcp_result") is not None for call in calls) else None,
        "serialized_tool_request_cost": counter.total(dump(call["serialized_tool_request"]) for call in calls) if calls and all(call.get("serialized_tool_request") is not None for call in calls) else None,
        "invocation_cost": counter.total(call["invocation"] for call in calls) if calls and all(call.get("invocation") is not None for call in calls) else None,
        "setup_timings_ms": setup_timings, "median_setup_ms": statistics.median(setup_timings),
        "selection": selection,
        "selected_contract": selected_contract,
        "operation": operation,
        "preconditions": preconditions,
        "guard_requirement": "supplied old bytes" if probe and case.get("requires_old_guard", True) else "profile contract",
        "model_repair_requests_blocked": len(repair),
        "repair_request_cost": counter.total(dump(item) for item in repair),
        "median_ms": statistics.median(timings), "timings_ms": timings,
        "measurement_scope": "deterministic native editor execution, not an agent/model task",
        "cost_scope": "tool inputs and returned text normalized to /workspace; adapter_return_cost includes instrumentation and is not a model response",
        "repair_request_scope": "one recorded repetition; inference requests are blocked, never billed",
        **first}


def fingerprint():
    suffixes = {".py", ".mjs", ".cjs", ".ts", ".rs", ".json", ".toml", ".lock", ".yml", ".yaml", ".md"}
    excluded = {"target", "__pycache__", "node_modules", ".git"}
    roots = [ROOT / "benches/native", ROOT / "integrations/dsh", ROOT / "integrations/pi",
             ROOT / "integrations/goose", ROOT / "npm/xnip"]
    paths = set((ROOT / "benches").glob("*.py"))
    for directory in roots:
        if directory.exists():
            paths.update(file for file in directory.rglob("*") if file.is_file()
                and file.suffix in suffixes and not excluded.intersection(file.relative_to(directory).parts))
    return {file.relative_to(ROOT).as_posix(): hashlib.sha256(file.read_bytes()).hexdigest()
            for file in sorted(paths)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--xnip", type=Path, default=ROOT / "target/debug" / ("xnip.exe" if os.name == "nt" else "xnip"))
    parser.add_argument("--codex", type=Path)
    parser.add_argument("--aider-python", type=Path, default=ROOT / "target/native-bench/aider-venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python"))
    parser.add_argument("--harnesses", default="dsh,pi,goose",
        help="additional real native harnesses; comma-separated or empty to retain legacy profiles only")
    parser.add_argument("--skip-host-exchange", action="store_true",
        help="omit the separately reported real host-to-xnip MCP lanes")
    parser.add_argument("--only-profiles", help="comma-separated development subset; include xnip-edit for relative reports")
    parser.add_argument("--encoding")
    parser.add_argument("--tokenizer-path", type=Path, default=ROOT / "target/benchmark-python")
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--json", type=Path, default=ROOT / "target/native-bench/results.json")
    parser.add_argument("--markdown", type=Path, default=ROOT / "target/native-bench/results.md")
    parser.add_argument("--report-only", type=Path, metavar="RESULTS_JSON",
        help="regenerate relative statistics, Markdown and SVG charts from an existing native run; no executors or tokenizer required")
    args = parser.parse_args()
    if args.report_only is not None:
        write_report(json.loads(args.report_only.read_text(encoding="utf8")), args.markdown, args.report_only)
        print(f"report regenerated from {args.report_only}; raw results unchanged")
        return 0
    if args.repeats < 1: parser.error("--repeats must be positive")
    requested_profiles = set(filter(None, args.only_profiles.split(","))) if args.only_profiles else None
    if requested_profiles is not None and "xnip-edit" not in requested_profiles:
        parser.error("--only-profiles requires xnip-edit for relative reports")
    def wants_family(family):
        return requested_profiles is None or any(name.startswith(family + "-") for name in requested_profiles)
    if args.tokenizer_path.is_dir(): sys.path.insert(0, str(args.tokenizer_path))
    counter = Counter(args.encoding)
    initial = fingerprint()
    binary_hash = hashlib.sha256(args.xnip.read_bytes()).hexdigest()
    workers, backends, unavailable = [], [], []
    metadata = {}
    try:
        with contextlib.ExitStack() as scope:
            temporary = scope.enter_context(fixture_session("native-"))
            root = Path(temporary)
            xnip = Mcp(args.xnip.resolve(), root)
            workers.append(xnip)
            scope.callback(xnip.close)
            xnip_info = {"version": xnip.initialize["serverInfo"]["version"], "binary_sha256": binary_hash,
                         "executable": str(args.xnip.resolve())}
            backends.append(Backend("xnip-edit", "xnip", "xnip", xnip_info, xnip, counter=counter))
            for name, mode in [
                ("xnip-relative", "xnip-mcp-relative"),
                ("xnip-adaptive-guarded", "xnip-mcp-guarded"),
                ("xnip-adaptive-snapshot", "xnip-mcp-snapshot"),
                ("xnip-cli-native-guarded", "xnip-cli-native-guarded"),
                ("xnip-cli-native-snapshot", "xnip-cli-native-snapshot"),
                ("xnip-cli-compact-guarded", "xnip-cli-compact-guarded"),
                ("xnip-cli-compact-snapshot", "xnip-cli-compact-snapshot"),
            ]:
                backends.append(Backend(name, "xnip", mode, xnip_info, counter=counter))
            for name, mode in [("xnip-adaptive-guarded-text", "xnip-mcp-guarded"),
                               ("xnip-adaptive-snapshot-text", "xnip-mcp-snapshot")]:
                backends.append(Backend(name, "xnip", mode, {**xnip_info, "receipt_mode": "text"}, counter=counter))
            codex = (codex_discover(args.codex) if wants_family("codex") else
                {"status": "excluded", "reason": "not selected by --only-profiles"})
            metadata["codex"] = codex
            if codex["status"] == "available":
                backends.append(Backend("codex-apply-patch", "codex", "codex", codex))
                backends.append(Backend("codex-apply-patch-unique", "codex", "codex-unique", codex, counter=counter))
            elif codex["status"] != "excluded":
                unavailable.append({"name": "Codex apply_patch", "reason": codex.get("reason")})
            for name, command in [
                ("gemini", ["node", str(ROOT / "benches/native/gemini_adapter.mjs")]),
                ("opencode", ["node", str(ROOT / "benches/native/opencode_adapter.mjs")]),
                ("aider", [str(args.aider_python), str(ROOT / "benches/native/aider_adapter.py")]),
            ]:
                if not wants_family(name):
                    continue
                try:
                    worker = Worker(command)
                    workers.append(worker)
                    scope.callback(worker.close)
                    info = worker.request({"action": "describe"})
                    metadata[name] = info
                    if info.get("status") != "available": raise RuntimeError(dump(info))
                    if name == "gemini":
                        module_root = Path(info["module_root"])
                        for helper in ("dist/src/utils/pathCorrector.js", "dist/src/utils/bfsFileSearch.js"):
                            source = module_root / helper
                            info["source_files"].append({"path": helper, "sha256": hashlib.sha256(source.read_bytes()).hexdigest()})
                        backends.append(Backend("gemini-edit-context", name, "gemini-context", info, worker))
                        backends.append(Backend("gemini-edit-minimal", name, "gemini", info, worker))
                        backends.append(Backend("gemini-edit-relative-context", name, "gemini-context-relative", info, worker))
                        backends.append(Backend("gemini-edit-relative-minimal", name, "gemini-relative", info, worker))
                        backends.append(Backend("gemini-relative-known-file-trim", name, "gemini-relative-trim", info, worker, counter=counter))
                    if name == "opencode":
                        backends.append(Backend("opencode-edit", name, "opencode", info, worker))
                        backends.append(Backend("opencode-edit-relative", name, "opencode-relative", info, worker))
                        backends.append(Backend("opencode-relative-known-file-trim", name, "opencode-relative-trim", info, worker, counter=counter))
                        backends.append(Backend("opencode-apply-patch", name, "opencode-patch", info, worker))
                        backends.append(Backend("opencode-apply-patch-unique", name, "opencode-patch-unique", info, worker, counter=counter))
                    if name == "aider":
                        for format in ("diff", "whole", "udiff"):
                            for endings in ("platform", "lf"):
                                backends.append(Backend(f"aider-{format}-{endings}", name, "aider", info, worker, format, endings))
                        backends.append(Backend("aider-diff-unique-lf", name, "aider-unique", info, worker, "diff", "lf", counter))
                        for endings in ("platform", "lf"):
                            backends.append(Backend(f"aider-whole-eof-aware-{endings}", name, "aider-eof-aware", info, worker, "whole", endings, counter))
                except (OSError, RuntimeError, queue.Empty) as error:
                    unavailable.append({"name": name, "reason": str(error)[:1500]})
            extra, host_backends = register_harnesses(Worker, scope, workers, args, counter,
                metadata, unavailable, mcp_plans, choose_plan, ROOT)
            backends.extend(extra)
            if requested_profiles is not None:
                backends = [backend for backend in backends if backend.name in requested_profiles]
                host_backends = [backend for backend in host_backends if backend.name in requested_profiles]
                missing = requested_profiles - {backend.name for backend in [*backends, *host_backends]}
                if missing:
                    raise RuntimeError("selected profiles unavailable or unknown: " + ", ".join(sorted(missing)))
            unavailable.extend([
                {"name": "Claude Code Edit / Write", "reason": "No independently invocable official native executor was available; not simulated or scored."},
                {"name": "Cursor native edit", "reason": "Installed app has no verified independent native edit entry; not simulated or scored."},
            ])
            cases, host_exchanges = [], []
            for scenario in scenarios():
                case = {"name": scenario.name, "files": scenario.files, "expected": scenario.expected(),
                    "edits": scenario.edits, "purpose": TASKS[scenario.name]}
                print("native case: " + scenario.name, file=sys.stderr, flush=True)
                case["results"] = [record(backend, root, case, counter, args.repeats) for backend in backends]
                cases.append(case)
                if host_backends:
                    host_exchanges.append({"name": scenario.name, "purpose": case["purpose"],
                        "results": [record(backend, root, case, counter, args.repeats) for backend in host_backends]})
            observations = []
            for probe in probes():
                case = {**probe, "edits": probe.get("edits") or [probe["edit"]], "purpose": probe["observation"],
                        "requires_old_guard": probe["name"] != "aider-accepted-unclosed-whole"}
                if probe["name"] == "aider-accepted-unclosed-whole":
                    chosen = [backend for backend in backends if backend.family == "aider" and backend.format == "whole"]
                else:
                    chosen = [backend for backend in backends if not backend.mode.startswith("gemini-context") and not (
                        backend.family == "aider" and backend.format != "diff") and not (
                        probe["name"] == "overlapping-literal" and (backend.mode.startswith("codex") or backend.mode.startswith("opencode-patch") or backend.family == "aider"))]
                chosen = [backend for backend in chosen if not case["requires_old_guard"]
                          or getattr(backend, "supports_old_guard", True)]
                case["results"] = [record(backend, root, case, counter, args.repeats, True) for backend in chosen]
                observations.append(case)
            for case in policy_probe_cases():
                chosen = [backend for backend in backends if backend.family == "dsh" and backend.operation == "edit"]
                if chosen:
                    case["results"] = [record(backend, root, case, counter, args.repeats, True) for backend in chosen]
                    observations.append(case)
            report = {"schema_version": 2, "scope": "paired real native editor execution versus xnip",
                "model_calls_performed": 0, "platform": sys.platform, "python": sys.version, "counter": counter.metadata, "repeats": args.repeats,
                "metadata": metadata, "unavailable": unavailable, "adapter_fingerprints": initial,
                "xnip_binary_sha256": binary_hash,
                "backends": [{"name": backend.name, "family": backend.family,
                    "version": backend.metadata.get("version", "source pin"), "mode": backend.mode,
                    "line_endings": backend.line_endings, "contract_cohort": backend.contract_cohort,
                    "transport": backend.transport, "receipt_mode": backend.receipt_mode if backend.transport == "mcp" else None,
                    "returned_text_boundary_known": getattr(backend, "returned_text_boundary_known", backend.family != "aider"),
                    "source_commit": backend.metadata.get("commit", backend.metadata.get("revision")),
                    "execution_level": backend.metadata.get("execution_level"),
                    "policy_profile": getattr(backend, "policy_profile", "legacy-native-kernel"),
                    "input_conformance": getattr(backend, "input_conformance", "see profile contract"),
                    "path_configuration": getattr(backend, "path_configuration", "see profile contract"),
                    "frontier_eligible": getattr(backend, "frontier_eligible", True),
                    "frontier_group": getattr(backend, "frontier_group", "main"),
                    "strategy_scope": "known-input minimum serialized candidate before execution" if "adaptive" in backend.name or backend.mode.startswith("xnip-cli") or backend.mode.endswith("-trim") else "fixed supplied-input profile"} for backend in backends],
                "strategy_contracts": {
                    "guarded": "Requires old block/anchor or complete expected old file; guard probes retain the exact supplied old bytes.",
                    "snapshot": "Full before/after snapshot is supplied; explicit whole-file writes, original-coordinate line manifests and rederived unique literal guards are eligible. Selection retains the actual guarded or snapshot contract.",
                    "known_file_trim": "Snapshot-only positive cases trim shared prefix/suffix using each ordered virtual revision, retaining nonempty overlap-unique old text and explicit all semantics. Small common contexts enumerate offsets; large ones use bounded offsets. Compact pair cost selects the trim before full argument-plan scoring; original inputs remain eligible. OpenCode/Gemini accepted-relative profiles receive the same helper; required-old probes never trim.",
                    "snapshot_lines": "Existing default xnip_apply manifests and CLI native programs use LF-only diff groups with 0/1/3 context and original coordinates, descending records, always-quoted payloads, and source-derived byte simulation. A final payload LF is omitted only when actual normalization restores it. No candidate execution or extra source read selects an input.",
                    "selection": "Candidates are scored by complete serialized executor input before running any candidate. No outcome or returned text is consulted.",
                    "path_context": "Legacy xnip-edit keeps outer-parent cwd and absolute paths; new MCP profiles launch per case with case cwd and relative slash paths.",
                    "native_minimal": "Unique-context variants enumerate symmetric 0..3 lines, widening only when required. EOF-aware whole closes newline-terminated goals and leaves the last fence open only for an unterminated goal without trailing whitespace, before execution; not a global optimum proof.",
                    "native_relative": "Original absolute literal profiles remain. OpenCode relative uses its instance.directory fallback; Gemini relative uses its own correctPath validation against getTargetDir. Actual requests and native-corrected parameters are retained.",
                    "timing": "CLI profiles include cold process startup; MCP profiles record initialization separately and time warm calls. No cross-transport latency ranking.",
                    "accounting": "Complete stdin programs or complete tool argument objects are executor input. Actual CLI invocation and canonical serialized captured JSON-RPC request/reply are independent additional columns.",
                    "receipt_modes": "Original structured profiles explicitly select --receipts structured; current xnip defaults to text. Two text profiles use identical pre-input selection with no structuredContent or outputSchema; compare actual captured full replies separately from returned text.",
                },
                "cases": cases, "probes": observations, "host_exchanges": host_exchanges}
    finally:
        for worker in reversed(workers):
            if worker.process.poll() is None: worker.close()
    if initial != fingerprint(): raise RuntimeError("adapter source changed during run; refusing mixed report")
    if binary_hash != hashlib.sha256(args.xnip.read_bytes()).hexdigest(): raise RuntimeError("xnip binary changed during run")
    report = public_report(report)
    args.json.parent.mkdir(parents=True, exist_ok=True)
    args.markdown.parent.mkdir(parents=True, exist_ok=True)
    args.json.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf8")
    write_report(report, args.markdown, args.json)
    print(f"native backends: {len(backends)}; tasks: {len(cases)}; repetitions: {args.repeats}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
