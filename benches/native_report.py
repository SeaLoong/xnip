"""Derived paired statistics and reproducible SVG charts for native editor runs.

No native executor, tokenizer, network connection, or plotting dependency is needed.
The source report remains unchanged; relative-results.json is a separate artifact.
"""
from __future__ import annotations

import hashlib
from html import escape
import json
import math
from pathlib import Path
import statistics

BASELINE = "xnip-edit"
COLORS = {"xnip": "#007f73", "codex": "#3b6fac", "gemini": "#9165b6",
          "opencode": "#c46a24", "aider": "#6c7785"}


def geomean(values, allow_zero=False):
    values = list(values)
    if not values:
        return None
    if any(value < 0 or (value == 0 and not allow_zero) or not math.isfinite(value) for value in values):
        raise ValueError("geometric means require positive finite paired ratios")
    if 0 in values:
        return 0.0
    return math.exp(statistics.fmean(math.log(value) for value in values))


def byte_exact(row):
    return bool(row["outcomes"]) and all(item["byte_exact"] for item in row["outcomes"])


def paired_statistics(report):
    """One ratio per task, never one weight per repetition/call/token."""
    unit = report["counter"]["unit"]
    summaries, pairs = [], []
    common_exact = []
    backend_names = {backend["name"] for backend in report["backends"]}
    if BASELINE not in backend_names:
        raise ValueError("relative reports require the xnip-edit baseline")
    for case in report["cases"]:
        rows = {row["backend"]: row for row in case["results"]}
        if len(rows) != len(case["results"]):
            raise ValueError("duplicate backend in task " + case["name"])
        baseline = rows.get(BASELINE)
        if baseline is None:
            raise ValueError("missing xnip-edit baseline in task " + case["name"])
        denominator = baseline["input_cost"][unit]
        if denominator is None or denominator <= 0:
            raise ValueError("positive baseline cost required in task " + case["name"])
        if backend_names <= rows.keys() and all(byte_exact(rows[name]) for name in backend_names):
            common_exact.append(case["name"])
        for backend in report["backends"]:
            row = rows.get(backend["name"])
            if row is None:
                continue
            numerator = row["input_cost"][unit]
            if numerator is None or numerator <= 0:
                raise ValueError("positive native cost required in task " + case["name"])
            # The native aider kernel does not expose its final model message.
            # Captured empty text for a known boundary is zero; an absent boundary is unknown.
            captured = row["family"] in {"xnip", "codex", "gemini", "opencode"}
            baseline_returned = baseline["returned_text_cost"][unit]
            returned = row["returned_text_cost"][unit] if captured else None
            returned_ratio = returned / baseline_returned if returned is not None and baseline_returned else None
            combined_ratio = ((numerator + returned) / (denominator + baseline_returned)
                if returned is not None and baseline_returned is not None else None)
            pairs.append({"scenario": case["name"], "backend": backend["name"],
                "native_input": numerator, "xnip_input": denominator,
                "native_captured_returned_text": returned,
                "xnip_captured_returned_text": baseline_returned,
                "captured_returned_text_ratio": returned_ratio,
                "input_plus_captured_returned_text_ratio": combined_ratio,
                "input_ratio": numerator / denominator,
                "both_byte_exact": byte_exact(row) and byte_exact(baseline),
                "native_byte_exact": byte_exact(row),
                "native_lf_normalized": all(item["lf_normalized_equal"] for item in row["outcomes"]),
                "stable_outcome": row["stable_outcome"]})
    for backend in report["backends"]:
        rows = [row for row in pairs if row["backend"] == backend["name"]]
        exact = [row for row in rows if row["both_byte_exact"]]
        summaries.append({"backend": backend["name"], "family": backend["family"],
            "pairs": len(rows), "exact_pairs": len(exact),
            "byte_exact_cases": sum(row["native_byte_exact"] for row in rows),
            "lf_normalized_cases": sum(row["native_lf_normalized"] for row in rows),
            "input_geomean_all": geomean(row["input_ratio"] for row in rows),
            "input_geomean_exact_pairs": geomean(row["input_ratio"] for row in exact),
            "input_geomean_without_long_line": geomean(row["input_ratio"] for row in rows if row["scenario"] != "long-line"),
            "captured_returned_text_pairs": sum(row["captured_returned_text_ratio"] is not None for row in rows),
            "captured_returned_text_geomean_all": geomean((row["captured_returned_text_ratio"] for row in rows if row["captured_returned_text_ratio"] is not None), allow_zero=True),
            "captured_returned_text_geomean_exact_pairs": geomean((row["captured_returned_text_ratio"] for row in exact if row["captured_returned_text_ratio"] is not None), allow_zero=True),
            "input_plus_captured_returned_text_geomean_all": geomean(row["input_plus_captured_returned_text_ratio"] for row in rows if row["input_plus_captured_returned_text_ratio"] is not None),
            "input_plus_captured_returned_text_geomean_exact_pairs": geomean(row["input_plus_captured_returned_text_ratio"] for row in exact if row["input_plus_captured_returned_text_ratio"] is not None),
            "exact_scenarios": [row["scenario"] for row in exact]})
    return {"schema_version": 1, "baseline": BASELINE, "unit": unit,
        "counter": report["counter"],
        "ratio": "native input cost / xnip input cost on the same task; xnip = 1x",
        "aggregation": "exp(mean(log(per-task ratio))); each task has equal weight; repetitions are not independent tasks",
        "all_cases_scope": "payload ratio only, including byte-incompatible native outputs",
        "returned_text_scope": "captured text boundary only; aider final model message unknown, never zero-filled",
        "combined_scope": "sum of independently counted input and captured returned text; excludes schemas, repair requests, host framing and complete agent workflow",
        "exact_pairs_scope": "both tools byte-exact in every repetition; subsets differ between backends",
        "common_exact_scenarios": common_exact, "backends": summaries, "pairs": pairs}


def multiple(value):
    return "n/a" if value is None else f"{value:.2f}x"


def svg_start(width, height, title, description):
    return [f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-labelledby="title desc">',
        f"<title id=\"title\">{escape(title)}</title><desc id=\"desc\">{escape(description)}</desc>",
        '<style>text{font-family:Segoe UI,Arial,sans-serif;fill:#192b3a;font-size:13px}.small{font-size:11px;fill:#526575}.title{font-size:21px;font-weight:600}.tick{fill:#dfe6eb;stroke:none}</style>',
        f'<rect width="{width}" height="{height}" fill="#fff"/>',
        f'<text x="28" y="36" class="title">{escape(title)}</text>']


def text(x, y, value, cls="", anchor="start"):
    return f'<text x="{x:.2f}" y="{y:.2f}" class="{cls}" text-anchor="{anchor}">{escape(str(value))}</text>'


def ratio_chart(stats):
    rows = stats["backends"]
    width, left, right, top, step = 1160, 250, 820, 112, 38
    height = top + len(rows) * step + 80
    values = [row[key] for row in rows for key in ("input_geomean_all", "input_geomean_exact_pairs") if row[key] is not None]
    low, high = min(0.5, min(values)), max(2, max(values) * 1.35)
    axis = lambda value: left + math.log(value / low) / math.log(high / low) * (right - left)
    out = svg_start(width, height, "Relative native edit input", "Per-task native input divided by xnip input. Geometric means use equal task weights. Exact subsets differ across backends.")
    out += [text(28, 61, f"xnip = 1x | {stats['unit']} | log axis | higher means more native input", "small"),
            text(28, 82, "Filled bar: all tasks, including incompatible outputs. Orange diamond: both tools byte-exact.", "small"),
            text(873, 101, "All tasks"), text(990, 101, "Exact subset")]
    ticks = [0.5, 1, 2, 5, 10, 20, 50, 100]
    for tick in ticks:
        if low <= tick <= high:
            x = axis(tick)
            out.append(f'<line x1="{x:.2f}" y1="{top - 14}" x2="{x:.2f}" y2="{top + len(rows) * step}" stroke="{("#768b9a" if tick == 1 else "#e1e7ed")}" stroke-dasharray="{("4 3" if tick == 1 else "1 0")}"/>')
            out.append(text(x, top + len(rows) * step + 22, f"{tick:g}x", "small", "middle"))
    for index, row in enumerate(rows):
        y = top + index * step + 10
        x = axis(row["input_geomean_all"])
        start, end = sorted((axis(1), x))
        out += [text(28, y + 4, row["backend"]),
                f'<rect x="{start:.2f}" y="{y - 8}" width="{max(end - start, 1):.2f}" height="16" rx="3" fill="{COLORS[row["family"]]}"/>']
        if row["input_geomean_exact_pairs"] is not None:
            p = axis(row["input_geomean_exact_pairs"])
            out.append(f'<path d="M {p:.2f} {y - 7} l 7 7 l -7 7 l -7 -7 Z" fill="#efb23d" stroke="#654610"/>')
        out += [text(873, y + 4, f"{multiple(row['input_geomean_all'])} ({row['pairs']})"),
                text(990, y + 4, f"{multiple(row['input_geomean_exact_pairs'])} ({row['exact_pairs']})")]
    out += [text(28, height - 18, "Counts in parentheses are paired tasks. Exact subsets have different sizes; these bars are not a common-corpus quality ranking.", "small"), "</svg>"]
    return "\n".join(out) + "\n"


def overview_chart(stats, report):
    """Compact README view: selected profiles, honest cohorts, concrete tasks."""
    preferred = [
        ("xnip-edit", "xnip · exact edit"),
        ("opencode-edit", "OpenCode · edit"),
        ("codex-apply-patch", "Codex · apply_patch"),
        ("gemini-edit-context", "Gemini · context"),
        ("aider-diff-lf", "aider · diff / LF"),
        ("aider-whole-lf", "aider · whole / LF"),
    ]
    by_name = {row["backend"]: row for row in stats["backends"]}
    selected = [(by_name[name], label) for name, label in preferred if name in by_name]
    # Synthetic contract fixtures and custom runs still get a complete chart.
    if len(selected) < 2:
        selected = [(row, row["backend"]) for row in stats["backends"]]
    width, left, right, top, step = 1060, 250, 700, 190, 43
    panel_top = top + len(selected) * step + 47
    height = panel_top + 148
    values = [row[key] for row, _ in selected
              for key in ("input_geomean_all", "input_geomean_exact_pairs")
              if row[key] is not None]
    maximum = max(2, math.ceil(max(values, default=1) * 1.1))
    axis = lambda value: left + value / maximum * (right - left)
    out = svg_start(width, height, "Less repeated edit input",
        "Measured native input divided by same-task xnip input. Selected profiles from the full run. "
        "All-task geometric means include supplied inputs whose output bytes differ. "
        "Exact-pair geometric means use separate byte-exact cohorts for each profile.")
    out += [text(28, 60, "Reuse a known block instead of retransmitting unchanged lines.", "small")]
    cards = [
        (len(report["cases"]), "edit scenarios"),
        (len(report["backends"]), "measured profiles"),
        (report["repeats"], "repetitions / task"),
    ]
    for index, (number, label) in enumerate(cards):
        x = 28 + index * 334
        out += [f'<rect x="{x}" y="80" width="314" height="55" rx="9" fill="#f1f6f8"/>',
                f'<text x="{x + 16}" y="113" style="font-size:25px;font-weight:650">{number}</text>',
                text(x + 65, 112, label)]
    out += [text(28, 165, f"Native edit input / xnip input · {stats['unit']} · xnip = 1.00x", "small"),
            text(756, 165, "All tasks"), text(880, 165, "Exact pairs")]
    ticks = sorted({0, 1, maximum, *range(0, maximum + 1, max(1, math.ceil(maximum / 5)))})
    for tick in ticks:
        x = axis(tick)
        out.append(f'<line x1="{x:.2f}" y1="{top - 9}" x2="{x:.2f}" y2="{top + len(selected) * step - 10}" stroke="{("#8296a2" if tick == 1 else "#e4ebef")}" stroke-dasharray="{("4 4" if tick == 1 else "1 0")}"/>')
        out.append(text(x, top + len(selected) * step + 12, f"{tick}x", "small", "middle"))
    for index, (row, label) in enumerate(selected):
        y = top + index * step + 10
        all_value = row["input_geomean_all"]
        if all_value is not None:
            out.append(f'<rect x="{left}" y="{y - 9}" width="{max(axis(all_value) - left, 1):.2f}" height="18" rx="4" fill="{COLORS.get(row["family"], "#6c7785")}"/>')
        exact = row["input_geomean_exact_pairs"]
        if exact is not None:
            out.append(f'<circle cx="{axis(exact):.2f}" cy="{y}" r="5" fill="#fff" stroke="#b46b16" stroke-width="2.5"/>')
        out += [text(28, y + 4, label), text(756, y + 4, multiple(all_value)),
                text(880, y + 4, f"{multiple(exact)} ({row['exact_pairs']}/{row['pairs']})")]
    out += [text(28, panel_top - 10, "Bars: all supplied inputs. Circle: both outputs byte-exact in every repetition. Exact subsets differ.", "small")]
    pairs = {(row["scenario"], row["backend"]): row for row in stats["pairs"]}
    examples = [
        ("large-local-change", "One local change in a 2,000-line file", "aider-whole-lf", "aider whole / LF"),
        ("long-line", "One substring in a long line", "codex-apply-patch", "Codex apply_patch"),
    ]
    for index, (case, title, native, label) in enumerate(examples):
        x = 28 + index * 502
        out.append(f'<rect x="{x}" y="{panel_top}" width="482" height="82" rx="9" fill="#eef7f4"/>')
        out.append(text(x + 16, panel_top + 23, title))
        row = pairs.get((case, native))
        if row:
            out += [f'<text x="{x + 16}" y="{panel_top + 53}" style="font-size:23px;font-weight:650;fill:#007f73">{row["xnip_input"]:,}</text>',
                    text(x + 87, panel_top + 52, f"xnip vs {row['native_input']:,} {label}", "small"),
                    text(x + 16, panel_top + 70, f"Captured edit-input {stats['unit']}; same task, same expected bytes.", "small")]
        else:
            out.append(text(x + 16, panel_top + 53, "Scenario not present in this run.", "small"))
    out += [text(28, height - 33, "Equal task weights. Selected profiles shown; complete results include every measured profile.", "small"),
            text(28, height - 15, "Editing executors, not full agent sessions. Input only; aider returned text is unknown.", "small"),
            "</svg>"]
    return "\n".join(out) + "\n"


def scenario_chart(stats, report):
    names = [backend["name"] for backend in report["backends"]]
    cases = [case["name"] for case in report["cases"]]
    pairs = {(row["scenario"], row["backend"]): row for row in stats["pairs"]}
    left, top, cw, ch = 227, 213, 91, 34
    width, height = left + len(names) * cw + 28, top + len(cases) * ch + 72
    out = svg_start(width, height, "Input ratios by edit scenario", "Each cell is native input divided by xnip input. A star marks a native output that was not byte-exact in every repetition.")
    out += [text(28, 61, f"xnip = 1x | {stats['unit']} | darker = larger ratio (log color scale)", "small"),
            text(28, 82, "* Native file bytes differ from the task goal. Payload size alone is not an equivalent-outcome saving.", "small")]
    for index, name in enumerate(names):
        x = left + index * cw + cw / 2
        out.append(f'<text transform="translate({x:.2f},{top - 12}) rotate(-48)" font-size="11">{escape(name)}</text>')
    max_ratio = max(row["input_ratio"] for row in stats["pairs"])
    for i, case in enumerate(cases):
        y = top + i * ch
        out.append(text(28, y + 22, case))
        for j, name in enumerate(names):
            row = pairs.get((case, name))
            x = left + j * cw
            shade = 0 if row is None else max(0, math.log(max(1, row["input_ratio"])) / math.log(max(2, max_ratio)))
            rgb = tuple(round(a + (b - a) * shade) for a, b in zip((238, 245, 249), (30, 77, 111)))
            color = "#%02x%02x%02x" % rgb
            out.append(f'<rect x="{x}" y="{y}" width="{cw - 3}" height="{ch - 3}" rx="3" fill="{color}"/>')
            label = "n/a" if row is None else multiple(row["input_ratio"]) + ("" if row["native_byte_exact"] else "*")
            fill = "#fff" if shade > 0.55 else "#192b3a"
            out.append(f'<text x="{x + (cw - 3) / 2}" y="{y + 21}" text-anchor="middle" style="fill:{fill};font-size:12px">{escape(label)}</text>')
    out += [text(28, height - 32, "Every cell uses its own same-task xnip denominator. Values are not total agent tokens or billed usage.", "small"),
            text(28, height - 14, "Gemini context follows the native prompt; minimal is an executor-accepted alternative. Aider platform follows this run's operating system.", "small"), "</svg>"]
    return "\n".join(out) + "\n"


def correctness_chart(stats):
    rows = stats["backends"]
    left, top, step, bar_width = 255, 100, 36, 540
    height = top + len(rows) * step + 64
    count = max(row["pairs"] for row in rows)
    out = svg_start(1040, height, "Observed file results", "Strict byte-exact tasks and extra tasks that match only after CRLF to LF normalization. Final newline remains significant.")
    out += [text(28, 61, "Dark green: byte-exact. Pale green: additional LF-normalized matches. Final newline remains significant.", "small"),
            text(28, 80, "Tasks count only when every repetition agrees with the oracle; this is a fixed fixture set.", "small")]
    for i, row in enumerate(rows):
        y = top + i * step
        exact, normalized, n = row["byte_exact_cases"], row["lf_normalized_cases"], row["pairs"]
        out += [text(28, y + 18, row["backend"]),
                f'<rect x="{left}" y="{y + 2}" width="{bar_width}" height="22" rx="3" fill="#edf0f3"/>',
                f'<rect x="{left}" y="{y + 2}" width="{bar_width * normalized / count:.2f}" height="22" fill="#b8ded4"/>',
                f'<rect x="{left}" y="{y + 2}" width="{bar_width * exact / count:.2f}" height="22" fill="#007f73"/>',
                text(left + bar_width + 20, y + 18, f"{exact}/{n} exact | {normalized}/{n} LF")]
    out += [text(28, height - 24, "Line-ending policies and accepted encodings differ. These observations are not a general product quality score.", "small"), "</svg>"]
    return "\n".join(out) + "\n"


def render(report, stats):
    unit = stats["unit"]
    lines = ["# Native editing tools versus xnip", "",
        f"The real native editors and xnip edited fresh copies of the same {len(report['cases'])} fixtures. No model calls were performed. These results measure captured tool payloads and file behavior, not complete agent sessions or billed tokens.", "",
        f"Counter: {json.dumps(report['counter'], ensure_ascii=False, sort_keys=True)}. Repetitions per task/profile: {report['repeats']}.",
        f"Platform: {report['platform']}. xnip binary SHA-256: {report['xnip_binary_sha256']}.",
        "Raw calls, expected bytes, per-repetition hashes and source provenance: [results.json](results.json). Derived statistics: [relative-results.json](relative-results.json).", "",
        "## Relative input costs", "",
        f"Ratio = **native input {unit} / xnip input {unit} on the same task**. xnip is **1.00x**. A ratio of 2x means xnip emits half as much captured edit input in that task; it does not mean the complete agent uses half as many tokens.",
        "The aggregate is exp(mean(log(task ratio))): each task has equal weight. Three repeats check execution stability; they do not give a task three statistical votes.",
        "The all-task column includes native outputs with different file bytes, so it describes supplied payloads only. The exact column includes a task only when both tools produce the expected bytes in every repetition. Those subsets differ between backends and must not be compared as a common-corpus ranking.", "",
        "![Geometric mean input ratios](charts/input-ratios.svg)", "",
        "| Native backend / profile | All tasks input ratio | Byte-exact pairs input ratio | Exact pairs | All tasks except long-line |",
        "|---|---:|---:|---:|---:|"]
    for row in stats["backends"]:
        lines.append(f"| {row['backend']} | {multiple(row['input_geomean_all'])} | {multiple(row['input_geomean_exact_pairs'])} | {row['exact_pairs']}/{row['pairs']} | {multiple(row['input_geomean_without_long_line'])} |")
    common = stats["common_exact_scenarios"]
    lines += ["", f"Tasks byte-exact across every recorded profile: {len(common)}" + (": " + ", ".join(common) if common else f". There is no valid shared byte-exact corpus for a {len(report['backends'])}-profile overall ranking in this run."),
        "The last column is a sensitivity check on the same supplied-input metric: the long-line fixture is deliberately extreme and should not silently determine the conclusion.", "",
        "![Per-scenario input ratios](charts/scenario-ratios.svg)", "",
        "## Captured return and combined strings", "",
        "Returned ratio = native captured returned text / xnip captured returned text. Combined ratio = (native input + native captured returned text) / (xnip input + xnip captured returned text), counted independently before summation. The same equal-task geometric mean and exact-pair cohorts apply.",
        "These are observable strings only: schemas, initialize instructions, repair requests, host framing/structured fields and the full agent workflow are excluded. xnip MCP and Codex CLI capture text at different host boundaries; this is not a uniform provider usage measurement. Aider final model output is unknown, so every aider returned/combined ratio is unavailable, not zero.", "",
        "| Captured backend | Returned all tasks | Input + return all tasks | Returned exact pairs | Input + return exact pairs |",
        "|---|---:|---:|---:|---:|"]
    for row in stats["backends"]:
        if row["captured_returned_text_pairs"]:
            lines.append(f"| {row['backend']} | {multiple(row['captured_returned_text_geomean_all'])} | {multiple(row['input_plus_captured_returned_text_geomean_all'])} | {multiple(row['captured_returned_text_geomean_exact_pairs'])} | {multiple(row['input_plus_captured_returned_text_geomean_exact_pairs'])} |")
    lines += ["", "Compare the return and combined columns alongside input costs; the conclusion below is calculated from the captured exchange in this run.", "", "## What the data supports", ""]
    by_name = {row["backend"]: row for row in stats["backends"]}
    lookup = {(row["scenario"], row["backend"]): row for row in stats["pairs"]}
    for name in ("codex-apply-patch", "gemini-edit-context", "gemini-edit-minimal", "opencode-edit", "opencode-apply-patch", "aider-diff-lf", "aider-whole-lf"):
        if name in by_name:
            row = by_name[name]
            lines.append(f"- {name}: all-task edit-input geometric mean **{multiple(row['input_geomean_all'])}**; byte-exact paired subset **{multiple(row['input_geomean_exact_pairs'])}** ({row['exact_pairs']} tasks).")
    if ("long-line", "xnip-edit") in lookup:
        base = lookup[("long-line", "xnip-edit")]["xnip_input"]
        selected = [name for name in ("codex-apply-patch", "gemini-edit-context", "opencode-edit", "aider-whole-lf") if ("long-line", name) in lookup]
        values = "; ".join(f"{name} {lookup[('long-line', name)]['native_input']:,}" for name in selected)
        lines += ["", f"The long-line task uses {base:,} xnip input {unit}; {values}. Literal substring editing avoids retransmitting an unchanged long line. This is a workload property, not a universal reduction factor."]
    if "codex-apply-patch" in by_name and ("same-file-batch", "codex-apply-patch") in lookup:
        row = lookup[("same-file-batch", "codex-apply-patch")]
        lines += ["", f"On same-file-batch, Codex uses {row['native_input']} input {unit} versus xnip's {row['xnip_input']} ({multiple(row['input_ratio'])}). A single native patch can be cheaper than several xnip calls. xnip_apply is an available batch path measured in the separate representation suite; this native profile measures xnip_edit and does not establish an xnip_apply saving. xnip does not win every task."]
    if ("tiny-config", "opencode-edit") in lookup:
        row = lookup[("tiny-config", "opencode-edit")]
        ratio = row["input_plus_captured_returned_text_ratio"]
        if ratio is None:
            conclusion = "The combined captured exchange is unavailable."
        elif row["input_ratio"] > 1 and ratio < 1:
            conclusion = "xnip has smaller input, but its longer captured return reverses that advantage in this task."
        elif row["input_ratio"] < 1 and ratio > 1:
            conclusion = "OpenCode has smaller input, but its longer captured return reverses that advantage in this task."
        elif ratio > 1:
            conclusion = "xnip has the smaller combined captured exchange in this task."
        elif ratio < 1:
            conclusion = "OpenCode has the smaller combined captured exchange in this task."
        else:
            conclusion = "The combined captured exchanges are equal in this task."
        lines += ["", f"On tiny-config, OpenCode edit emits {row['native_input']} input + {row['native_captured_returned_text']} returned {unit}, versus xnip's {row['xnip_input']} + {row['xnip_captured_returned_text']}. OpenCode's combined captured exchange is {multiple(ratio)} of xnip's. {conclusion}"]
    lines += ["", "Gemini's context profile includes its prescribed surrounding context and an instruction. The minimal profile is accepted by the executor but does not follow that full prompt rule. OpenCode's short literal interface is correspondingly close to xnip on input size.",
        "The useful optimization target is less redundant context and fewer calls when the edit contract permits it. These fixtures do not establish total agent savings, model edit success, or repository-level correctness.", "",
        "## File behavior", "", "![Strict and normalized file results](charts/byte-exact.svg)", "",
        "| Backend / profile | Version | Byte-exact cases | LF-normalized cases | Stable outcomes |",
        "|---|---|---:|---:|---:|"]
    for backend, summary in zip(report["backends"], stats["backends"]):
        records = [row for case in report["cases"] for row in case["results"] if row["backend"] == backend["name"]]
        lines.append(f"| {backend['name']} | {backend['version']} | {summary['byte_exact_cases']}/{summary['pairs']} | {summary['lf_normalized_cases']}/{summary['pairs']} | {sum(row['stable_outcome'] for row in records)}/{len(records)} |")
    lines += ["", "Byte equality includes CRLF and final newline. LF normalization still distinguishes a missing final newline. Aider platform follows the current operating system; lf is an explicit separate configuration.",
        "Aider's canonical whole-file profile uses closed code fences. An accepted unclosed final fence that preserves missing EOF is tested separately; the canonical generator does not describe every accepted native encoding.",
        "Native success status alone never passes the file oracle. The fixtures cover existing UTF-8 text files; lifecycle, approvals, mandatory Read, post-edit formatting and model reasoning are outside scope.", "",
        "## Per-task absolute measurements", "",
        "<details><summary>Captured inputs and returned text for every task/profile</summary>", "",
        f"| Task | Backend | Input {unit} | Returned text {unit} | Input / xnip | Return / xnip | Input + return / xnip | Byte exact |",
        "|---|---|---:|---:|---:|---:|---:|---|"]
    for case in report["cases"]:
        for row in case["results"]:
            ratio = lookup[(case["name"], row["backend"])]["input_ratio"]
            output = row["returned_text_cost"][unit] if row["family"] != "aider" else "not captured"
            pair = lookup[(case["name"], row["backend"])]
            lines.append(f"| {case['name']} | {row['backend']} | {row['input_cost'][unit]} | {output} | {multiple(ratio)} | {multiple(pair['captured_returned_text_ratio'])} | {multiple(pair['input_plus_captured_returned_text_ratio'])} | {byte_exact(row)} |")
    lines += ["", "</details>", "", "Returned-text counts cover the captured native output/error, not a uniform full agent response. Aider's final model message is unavailable at its editing-kernel boundary; it is **not zero**. Adapter-return objects include instrumentation and duplicated fields, so combined observable-string ratios above exclude those objects and do not rank full agent-token costs.",
        "Warm MCP/Node/kernel timing excludes worker initialization. Codex CLI timing includes native process startup. Raw timings remain in JSON; these unequal scopes are not ranked as universal speed.", "",
        "## Matching and recovery probes", "",
        "| Probe | Backend | Reports success | File unchanged | Goal byte exact | Blocked model repairs |",
        "|---|---|---|---|---|---:|"]
    for case in report["probes"]:
        for row in case["results"]:
            lines.append(f"| {case['name']} | {row['backend']} | {row['native_reported_ok']} | {row['outcomes'][0]['unchanged']} | {byte_exact(row)} | {row['model_repair_requests_blocked']} |")
    lines += ["", "Probes observe uniqueness, stale context, whitespace and repair policies. Unsupported representations are omitted. Gemini repair requests are captured and blocked before inference; no repaired model outcome or billed cost is inferred.",
        "", "## Unmeasured products", "", "| Product | Reason |", "|---|---|"]
    for item in report["unavailable"]:
        lines.append(f"| {item['name']} | {str(item['reason']).replace('|', '/')} |")
    lines += ["", "See [method and limits](native-methodology.md) and [reproduction commands](README.md). The separate [representation report](representation-results.md) measures current xnip options and serialized formats, not native third-party execution.", ""]
    return "\n".join(lines)


def write_report(report, markdown, source_json):
    markdown, source_json = Path(markdown), Path(source_json)
    stats = paired_statistics(report)
    stats["source_json_sha256"] = hashlib.sha256(source_json.read_bytes()).hexdigest()
    markdown.parent.mkdir(parents=True, exist_ok=True)
    charts = markdown.parent / "charts"
    charts.mkdir(parents=True, exist_ok=True)
    for name, svg in (("overview", overview_chart(stats, report)),
                      ("input-ratios", ratio_chart(stats)),
                      ("scenario-ratios", scenario_chart(stats, report)),
                      ("byte-exact", correctness_chart(stats))):
        (charts / (name + ".svg")).write_text(svg, encoding="utf8")
    (markdown.parent / "relative-results.json").write_text(
        json.dumps(stats, ensure_ascii=False, indent=2) + "\n", encoding="utf8")
    markdown.write_text(render(report, stats), encoding="utf8")
