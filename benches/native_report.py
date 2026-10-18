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



def profile_transport(report, row):
    backend = next((item for item in report["backends"] if item["name"] == row["backend"]), {})
    for source in (row, backend, backend.get("metadata", {})):
        transport = source.get("transport")
        if transport is not None:
            return transport
    if row["family"] == "xnip":
        return "cli-stdin" if "-cli-" in row["backend"] else "mcp"
    return "cli-stdin" if row["family"] == "codex" else "native-function"


def guard_cohort(report, row):
    """Use the declared contract; infer only historical profile conventions."""
    backend = next((item for item in report["backends"] if item["name"] == row["backend"]), {})
    for source in (row, backend, backend.get("metadata", {})):
        cohort = source.get("contract_cohort")
        if cohort is not None:
            if cohort not in {"guarded", "snapshot", "native"}:
                raise ValueError("unknown contract cohort: " + str(cohort))
            return cohort
    return "snapshot" if row["family"] == "aider" and "whole" in row["backend"] else "guarded"


def _frontier_profile(report, case, row):
    unit = report["counter"]["unit"]
    backend = next((item for item in report["backends"] if item["name"] == row["backend"]), {})
    captured = row["family"] in {"xnip", "codex", "gemini", "opencode"}
    transport = profile_transport(report, row)
    supplied = row["input_cost"][unit]
    returned = row.get("returned_text_cost", {}).get(unit) if captured else None
    serialized = ((row.get("full_serialized_mcp_cost") or {}).get(unit)
                  if row["family"] == "xnip" and transport == "mcp" else None)
    invocation = (row.get("invocation_cost") or {}).get(unit)
    request = (row.get("serialized_tool_request_cost") or {}).get(unit)
    for value in (supplied, returned, serialized, invocation, request):
        if value is not None and (value < 0 or not math.isfinite(value)):
            raise ValueError("frontier costs must be nonnegative finite numbers")
    if supplied is None or supplied <= 0:
        raise ValueError("positive frontier input cost required in task " + case["name"])
    return {"scenario": case["name"], "backend": row["backend"], "family": row["family"],
        "transport": transport, "contract_cohort": guard_cohort(report, row), "byte_exact": byte_exact(row),
        "input": supplied, "captured_returned_text": returned, "serialized_mcp_result": serialized,
        "invocation": invocation, "serialized_tool_request": request,
        "input_plus_captured_returned_text": supplied + returned if returned is not None else None,
        "input_plus_serialized_mcp_result": supplied + serialized if serialized is not None else None,
        **{key: row.get(key, backend.get(key)) for key in ("operation", "preconditions", "selected_contract")}}


def _frontier_pool(profiles):
    eligible = sorted((row for row in profiles if row["byte_exact"]), key=lambda row: row["backend"])
    def best(metric):
        known = [row for row in eligible if row[metric] is not None]
        return min(known, key=lambda row: (row[metric], row["backend"])) if known else None
    return {"eligible_profiles": [row["backend"] for row in eligible],
        "best_input": best("input"),
        "best_input_plus_captured_returned_text": best("input_plus_captured_returned_text")}


def frontier_statistics(report):
    """Derive observed minima; do not feed an oracle-selected strategy to execution."""
    profiles, tasks, comparisons = [], [], []
    cohorts = ("guarded", "snapshot", "native")
    allowed = {"guarded": {"guarded"}, "snapshot": {"guarded", "snapshot"}, "native": {"native"}}
    for case in report["cases"]:
        records = [_frontier_profile(report, case, row) for row in case["results"]]
        if len({row["backend"] for row in records}) != len(records):
            raise ValueError("duplicate backend in task " + case["name"])
        profiles.extend(records)
        transports = sorted({row["transport"] for row in records if row["family"] == "xnip"})
        native = {cohort: _frontier_pool(row for row in records
            if row["family"] != "xnip" and row["contract_cohort"] in allowed[cohort]) for cohort in cohorts}
        xnip = {transport: {cohort: _frontier_pool(row for row in records
            if row["family"] == "xnip" and row["transport"] == transport and row["contract_cohort"] in allowed[cohort])
            for cohort in cohorts} for transport in transports}
        task = {"scenario": case["name"], "native": native, "xnip": xnip, "comparisons": []}
        for transport in transports:
            for cohort in cohorts:
                if not any(row["contract_cohort"] in allowed[cohort] and
                        (row["family"] != "xnip" or row["transport"] == transport) for row in records):
                    continue
                comparison = {"scenario": case["name"], "contract_cohort": cohort, "xnip_transport": transport}
                for metric, winner in (("input", "best_input"),
                        ("input_plus_captured_returned_text", "best_input_plus_captured_returned_text")):
                    a, b = native[cohort][winner], xnip[transport][cohort][winner]
                    comparison[metric + "_ratio"] = a[metric] / b[metric] if a is not None and b is not None else None
                task["comparisons"].append(comparison)
                comparisons.append(comparison)
        tasks.append(task)
    summaries = []
    for transport, cohort in sorted({(row["xnip_transport"], row["contract_cohort"]) for row in comparisons}):
        rows = [row for row in comparisons if row["contract_cohort"] == cohort and row["xnip_transport"] == transport]
        summary = {"contract_cohort": cohort, "xnip_transport": transport}
        for metric in ("input", "input_plus_captured_returned_text"):
            pairs = [row for row in rows if row[metric + "_ratio"] is not None]
            summary[metric + "_pairs"] = len(pairs)
            summary[metric + "_geomean"] = geomean(row[metric + "_ratio"] for row in pairs)
        summaries.append(summary)
    return {"schema_version": 1, "unit": report["counter"]["unit"],
        "selection_scope": "best observed among supplied profiles that are byte-exact in every recorded repetition; not a global optimum or an executed strategy",
        "contract_scope": "guarded pools accept guarded profiles only; snapshot-enabled pools accept guarded and snapshot profiles on both sides; winning profiles retain their actual contract; xnip MCP and CLI transports never share a minimum",
        "native_transport_scope": "native executor-input minima may span native transports; each winning profile retains its transport",
        "aggregation": "equal-task geometric means of native best / xnip best within the same candidate-permission pool and xnip transport",
        "returned_text_scope": "known captured text only; aider final returned text is always unknown",
        "serialized_mcp_scope": "independently counted canonical serialized full JSON-RPC reply when recorded; unknown for other products and historical rows; not host/provider usage",
        "invocation_scope": "CLI invocation arguments and serialized tool-request envelopes remain separate; neither is added to executor input; schema/setup costs are excluded",
        "tasks": tasks, "cohorts": summaries, "profiles": profiles}


def receipt_comparisons(report):
    """Compare captured JSON-RPC reply modes for matched MCP strategy profiles."""
    unit, pairs = report["counter"]["unit"], []
    def cost(row, field):
        return ((row or {}).get(field) or {}).get(unit)
    def equal_cost(a, b):
        return a == b if a is not None and b is not None else None
    def equal_calls(a, b, field):
        calls_a, calls_b = (a or {}).get("calls"), (b or {}).get("calls")
        if not calls_a or not calls_b or any(field not in call for call in (*calls_a, *calls_b)):
            return None
        return [call[field] for call in calls_a] == [call[field] for call in calls_b]
    for case in report["cases"]:
        rows = {row["backend"]: row for row in case["results"]}
        if len(rows) != len(case["results"]):
            raise ValueError("duplicate backend in task " + case["name"])
        for text_row in case["results"]:
            name = text_row["backend"]
            if text_row["family"] != "xnip" or text_row.get("receipt_mode") != "text" or not name.endswith("-text"):
                continue
            if profile_transport(report, text_row) != "mcp":
                continue
            default_name = name.removesuffix("-text")
            default = rows.get(default_name)
            if default is not None and (default["family"] != "xnip" or
                    profile_transport(report, default) != "mcp" or default.get("receipt_mode", "structured") != "structured"):
                raise ValueError("receipt comparison requires a structured MCP counterpart: " + default_name)
            if default is not None and guard_cohort(report, default) != guard_cohort(report, text_row):
                raise ValueError("receipt comparison requires matching strategy contracts: " + default_name)
            before, after = cost(default, "full_serialized_mcp_cost"), cost(text_row, "full_serialized_mcp_cost")
            for value in (before, after):
                if value is not None and (value < 0 or not math.isfinite(value)):
                    raise ValueError("receipt costs must be nonnegative finite numbers")
            exact = default is not None and byte_exact(default) and byte_exact(text_row)
            inputs_identical = equal_calls(default, text_row, "input")
            returned_identical = equal_calls(default, text_row, "returned_text")
            qualified = (exact and before is not None and before > 0 and after is not None
                and inputs_identical is True and returned_identical is True)
            default_input, text_input = cost(default, "input_cost"), cost(text_row, "input_cost")
            default_returned, text_returned = cost(default, "returned_text_cost"), cost(text_row, "returned_text_cost")
            pairs.append({"scenario": case["name"], "default_backend": default_name, "text_backend": name,
                "contract_cohort": guard_cohort(report, text_row), "both_byte_exact": exact,
                "default_input": default_input, "text_input": text_input,
                "input_cost_equal": equal_cost(default_input, text_input),
                "inputs_identical": inputs_identical,
                "default_captured_returned_text": default_returned, "text_captured_returned_text": text_returned,
                "captured_returned_text_cost_equal": equal_cost(default_returned, text_returned),
                "returned_text_identical": returned_identical,
                "default_full_serialized_mcp_reply": before, "text_full_serialized_mcp_reply": after,
                "full_reply_ratio": after / before if qualified else None,
                "saved_full_reply": before - after if qualified else None})
    def summary(rows):
        measured = [row for row in rows if row["full_reply_ratio"] is not None]
        before = sum(row["default_full_serialized_mcp_reply"] for row in measured) if measured else None
        after = sum(row["text_full_serialized_mcp_reply"] for row in measured) if measured else None
        return {"paired_count": len(measured), "task_count": len({row["scenario"] for row in measured}),
            "observed_pairs": len(rows),
            "full_reply_ratio_geomean": geomean((row["full_reply_ratio"] for row in measured), allow_zero=True),
            "default_full_reply_sum": before, "text_full_reply_sum": after,
            "saved_full_reply_sum": before - after if before is not None else None,
            **{field + "_count": sum(row[field] is True for row in measured) for field in
                ("input_cost_equal", "inputs_identical", "captured_returned_text_cost_equal", "returned_text_identical")}}
    profiles = [{"default_backend": default, "text_backend": text,
        "contract_cohort": cohort,
        **summary([row for row in pairs if row["default_backend"] == default and row["text_backend"] == text and row["contract_cohort"] == cohort])}
        for default, text, cohort in sorted({(row["default_backend"], row["text_backend"], row["contract_cohort"]) for row in pairs})]
    return {"schema_version": 1, "unit": unit,
        "scope": "canonical serialized full JSON-RPC reply only, structured versus text receipt mode; not provider billing or complete agent usage",
        "ratio": "text receipt full reply / structured receipt full reply on the same task and strategy",
        "qualification": "both profiles byte-exact in every recorded repetition, both full-reply costs captured, and actual captured input and returned-text sequences identical; missing calls or different content exclude the effect ratio and savings",
        "aggregation": "equal task/profile-pair geometric mean; sums include the same qualified pairs; repeats are not extra votes",
        "equality_scope": "cost equality and actual captured-string equality are separate; absent calls or costs remain unknown",
        "pairs": pairs, "profiles": profiles, "overall": summary(pairs)}


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
        "common_exact_scenarios": common_exact, "backends": summaries, "pairs": pairs,
        "frontier": frontier_statistics(report), "receipt_modes": receipt_comparisons(report)}


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
    label_width = max((len(name) for name in names), default=0) * 6.5
    left, cw, ch = 227, 91, 34
    top = max(213, math.ceil(112 + label_width * math.sin(math.radians(48))))
    right_padding = max(180, math.ceil(label_width * math.cos(math.radians(48)) + 24 - cw / 2))
    width, height = left + len(names) * cw + right_padding, top + len(cases) * ch + 72
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



def _cohort_label(cohort):
    return "snapshot-enabled" if cohort == "snapshot" else cohort


def frontier_chart(frontier):
    """Separate xnip transports while preserving native executor-input candidates."""
    comparisons = [row for task in frontier["tasks"] for row in task["comparisons"]]
    transports = ["mcp", "cli-stdin"]
    transports += sorted({row["xnip_transport"] for row in comparisons} - set(transports))
    panels = [[row for row in comparisons if row["xnip_transport"] == transport] for transport in transports]
    panel_width, top, step = 754, 153, 31
    width, height = 28 + len(panels) * panel_width, top + max(1, *(len(rows) for rows in panels)) * step + 89
    values = [row[key] for row in comparisons for key in ("input_ratio", "input_plus_captured_returned_text_ratio") if row[key] is not None]
    low, high = min(0.5, min(values, default=1) / 1.2), max(2, max(values, default=1) * 1.2)
    out = svg_start(width, height, "Best observed editing frontier",
        "Separate xnip MCP and CLI panels compare byte-exact native and xnip minima within the same allowed-contract pool. "
        "Guarded pools admit guarded profiles only; snapshot-enabled pools admit guarded and snapshot profiles on both sides. "
        "Native executor-input candidates may span native transports. Invocation overhead is separate. "
        "Input and known-text exchange winners are independent; this is not a global optimum or an executed strategy.")
    out += [text(28, 61, f"Native best / xnip best | {frontier['unit']} | log axis | xnip = 1x | each minimum uses every recorded byte-exact repetition", "small"),
        text(28, 82, "Circle: input. Diamond: input + known text. Guarded: guarded only. Snapshot-enabled: guarded + whole writes. Aider return unknown.", "small"),
        text(28, 103, "Native candidates span native transports. CLI invocation and request envelopes are separate costs; no timing ranking is made.", "small")]
    for panel, (transport, rows) in enumerate(zip(transports, panels)):
        base = 28 + panel * panel_width
        left, right = base + 240, base + 546
        axis = lambda value: left + math.log(value / low) / math.log(high / low) * (right - left)
        title = {"mcp": "xnip MCP", "cli-stdin": "xnip CLI stdin"}.get(transport, "xnip " + transport)
        out += [f'<rect x="{base - 6}" y="116" width="{panel_width - 14}" height="{height - 174}" rx="7" fill="#f7f9fa"/>',
            text(base + 8, 136, title), text(base + 571, 136, "Input", "small"),
            text(base + 651, 136, "+ text", "small")]
        for tick in (0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50, 100, 500):
            if low <= tick <= high:
                x = axis(tick)
                out += [f'<line x1="{x:.2f}" y1="{top - 5}" x2="{x:.2f}" y2="{height - 66}" stroke="{("#7e929e" if tick == 1 else "#e2e9ed")}" stroke-dasharray="{("4 3" if tick == 1 else "1 0")}"/>',
                    text(x, height - 48, f"{tick:g}x", "small", "middle")]
        for index, row in enumerate(rows):
            y = top + index * step + 12
            color = {"guarded": "#007f73", "snapshot": "#b67418", "native": "#6c7785"}[row["contract_cohort"]]
            out.append(text(base + 8, y + 4, row["scenario"] + " · " + _cohort_label(row["contract_cohort"]), "small"))
            if row["input_ratio"] is not None:
                out.append(f'<circle cx="{axis(row["input_ratio"]):.2f}" cy="{y}" r="5" fill="{color}"/>')
            if row["input_plus_captured_returned_text_ratio"] is not None:
                x = axis(row["input_plus_captured_returned_text_ratio"])
                out.append(f'<path d="M {x:.2f} {y - 6} l 6 6 l -6 6 l -6 -6 Z" fill="#fff" stroke="{color}" stroke-width="2"/>')
            out += [text(base + 571, y + 4, multiple(row["input_ratio"]), "small"),
                text(base + 651, y + 4, multiple(row["input_plus_captured_returned_text_ratio"]), "small")]
        if not rows:
            out.append(text(base + 8, top + 22, "No profiles recorded for this xnip transport.", "small"))
    out += [text(28, height - 26, "Best observed supplied profiles only. Missing comparisons remain n/a. Winners can differ between input and known-text exchange.", "small"),
        text(28, height - 10, "Full serialized JSON-RPC replies are a separate captured layer when recorded; no frontier choice is replayed by the runner.", "small"), "</svg>"]
    return "\n".join(out) + "\n"


def readme_frontier_summary(frontier):
    """Four input-only cohorts; counts are task weights, not calls or repeats."""
    groups = []
    for transport in ("mcp", "cli-stdin"):
        for cohort in ("guarded", "snapshot"):
            summary = next((row for row in frontier["cohorts"]
                if row["xnip_transport"] == transport and row["contract_cohort"] == cohort), {})
            ratios = [next((row["input_ratio"] for row in task["comparisons"]
                if row["xnip_transport"] == transport and row["contract_cohort"] == cohort), None)
                for task in frontier["tasks"]]
            known = [ratio for ratio in ratios if ratio is not None]
            ties = sum(math.isclose(ratio, 1.0) for ratio in known)
            wins = sum(ratio > 1 and not math.isclose(ratio, 1.0) for ratio in known)
            groups.append({"xnip_transport": transport, "contract_cohort": cohort,
                "input_geomean": summary.get("input_geomean"), "input_pairs": len(known),
                "task_count": len(ratios), "wins": wins, "ties": ties,
                "losses": len(known) - wins - ties, "missing": len(ratios) - len(known)})
    return groups


def readme_frontier_chart(frontier, report):
    """A compact input-only view; full task and exchange views remain separate."""
    rows = readme_frontier_summary(frontier)
    width, height, left, right, top, step = 1000, 414, 280, 624, 148, 43
    values = [row["input_geomean"] for row in rows if row["input_geomean"] is not None]
    low, high = min(0.5, min(values, default=1) / 1.3), max(2, max(values, default=1) * 1.3)
    axis = lambda value: left + math.log(value / low) / math.log(high / low) * (right - left)
    out = svg_start(width, height, "Best observed editing input",
        "Four xnip transport and candidate-permission groups compare the geometric mean of native best "
        "executor input divided by xnip best executor input on byte-exact tasks. Guarded-only and snapshot-enabled "
        "(guarded plus snapshot) pools apply identical candidate permissions on both sides. Above one means less xnip input. "
        "Counts show xnip-shorter, tied, native-shorter and missing tasks. Native candidates may span transports; "
        "CLI invocation overhead is extra. This is not a full agent usage or global optimum result.")
    task_count = len(frontier["tasks"])
    out += [text(28, 61, f"{task_count} tasks · {report.get('repeats', '?')} repetitions / profile · {frontier['unit']} · equal task weights", "small"),
        text(28, 82, "Native best / xnip best: above 1x means less xnip input. Only all-repetition byte-exact profiles qualify.", "small"),
        text(28, 103, "Same candidate permissions on both sides; separate xnip transports. Native candidates span transports.", "small"),
        text(28, 131, "xnip transport / contract"), text(663, 131, "Input ratio"),
        text(786, 131, "W / T / L / ?")]
    for tick in (0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50, 100, 500, 1000):
        if low <= tick <= high:
            x = axis(tick)
            out += [f'<line x1="{x:.2f}" y1="{top - 12}" x2="{x:.2f}" y2="{top + len(rows) * step - 14}" stroke="{("#7e929e" if tick == 1 else "#e2e9ed")}" stroke-dasharray="{("4 3" if tick == 1 else "1 0")}"/>',
                text(x, 329, f"{tick:g}x", "small", "middle")]
    for index, row in enumerate(rows):
        y = top + index * step + 5
        label = ("MCP" if row["xnip_transport"] == "mcp" else "CLI stdin") + " · " + _cohort_label(row["contract_cohort"])
        color = "#007f73" if row["contract_cohort"] == "guarded" else "#b67418"
        ratio = row["input_geomean"]
        if ratio is not None:
            start, end = sorted((axis(1), axis(ratio)))
            out += [f'<rect x="{start:.2f}" y="{y - 8}" width="{max(end - start, 1):.2f}" height="16" rx="3" fill="{color}"/>',
                f'<circle cx="{axis(ratio):.2f}" cy="{y}" r="4" fill="{color}"/>']
        out += [text(28, y + 4, label), text(663, y + 4, f"{multiple(ratio)} ({row['input_pairs']}/{row['task_count']})"),
            text(786, y + 4, f"{row['wins']} / {row['ties']} / {row['losses']} / {row['missing']}")]
    out += [text(28, 354, "W / T / L / ? = xnip shorter / tie / native shorter / missing comparison. Counts use tasks, not repetitions.", "small"),
        text(28, 372, "Guarded: old content/context required. Snapshot-enabled: guarded edits + whole-file writes. Matching policies differ.", "small"),
        text(28, 390, "Input only: CLI invocation and request envelopes are extra. Schemas/setup and full agent usage are excluded.", "small"),
        text(28, 408, "Aider returned text is unknown; no returned-text or full-reply cost participates in this input chart.", "small"), "</svg>"]
    return "\n".join(out) + "\n"


def frontier_markdown(frontier):
    unit = frontier["unit"]
    def winner(pool, key, metric):
        row = pool[key]
        return "n/a" if row is None else f"{row[metric]:,} ({row['backend']}; {row['transport']}; {row.get('selected_contract') or row['contract_cohort']})"
    def cost(value):
        return "not captured" if value is None else f"{value:,}"
    lines = ["## Best observed frontier", "",
        "For each task, candidate-permission pool and xnip transport, select the smallest measured input and independently the smallest input plus known captured returned text. Only profiles whose files are byte-exact in every recorded repetition qualify. This is the **best observed** among supplied profiles, not a global optimum, a model success rate or an editing policy replayed by the runner.",
        "Both sides use identical candidate permissions. The **guarded** pool accepts guarded profiles only, requiring old-content/context preconditions. The **snapshot-enabled** pool permits whole-file writes and accepts both guarded and snapshot profiles on each side. Its winner may therefore be a guarded edit. Winning records and profile cost rows retain their actual contract. Matching strictness can still differ; the probes record those differences.",
        "**xnip MCP and CLI minima stay separate.** Native executor-input candidates may span native transports; each winning profile below retains its transport. This compares executor inputs, not uniform invocation costs. CLI argument invocation and serialized tool-request envelopes are reported separately and never added to executor input. Schema/setup costs are outside these minima. CLI cold-start and warm MCP times are not ranked together.",
        "Three cost layers stay distinct: supplied input, known captured returned text, and the canonical serialized full JSON-RPC reply when recorded. The last includes structured fields and can duplicate text; it is available only for recorded xnip MCP replies. Unknown data stays unavailable. Aider returned text is always unknown and never contributes a zero-cost exchange winner.", "",
        "![Best observed frontier with equal candidate permissions by xnip transport](charts/frontier.svg)", "",
        "| xnip transport | Candidate permissions | Input geometric mean native / xnip | Paired tasks | Input + known text geometric mean | Paired tasks |",
        "|---|---|---:|---:|---:|---:|"]
    for row in frontier["cohorts"]:
        lines.append(f"| {row['xnip_transport']} | {_cohort_label(row['contract_cohort'])} | {multiple(row['input_geomean'])} | {row['input_pairs']} | {multiple(row['input_plus_captured_returned_text_geomean'])} | {row['input_plus_captured_returned_text_pairs']} |")
    lines += ["", "Each task has equal weight in these geometric means. Input and known-text exchange can have different winning profiles and task subsets; compare the paired-task counts.", "",
        f"| Task | xnip transport | Candidate permissions | Native best input {unit} (profile; transport; actual contract) | xnip best input {unit} (profile; transport; actual contract) | Native / xnip | Native best input + text | xnip best input + text | Native / xnip |",
        "|---|---|---|---:|---:|---:|---:|---:|---:|"]
    for task in frontier["tasks"]:
        for comparison in task["comparisons"]:
            cohort, transport = comparison["contract_cohort"], comparison["xnip_transport"]
            native, xnip = task["native"][cohort], task["xnip"][transport][cohort]
            lines.append(f"| {task['scenario']} | {transport} | {_cohort_label(cohort)} | {winner(native, 'best_input', 'input')} | {winner(xnip, 'best_input', 'input')} | {multiple(comparison['input_ratio'])} | {winner(native, 'best_input_plus_captured_returned_text', 'input_plus_captured_returned_text')} | {winner(xnip, 'best_input_plus_captured_returned_text', 'input_plus_captured_returned_text')} | {multiple(comparison['input_plus_captured_returned_text_ratio'])} |")
    lines += ["", "<details><summary>Three captured cost layers and separate invocation overhead for every supplied profile</summary>", "",
        f"| Task | Profile | Transport | Actual profile contract | Byte-exact eligible | Input {unit} | Known returned text {unit} | Full JSON-RPC reply {unit} | Input + full JSON-RPC reply {unit} | Invocation arguments {unit} | Serialized tool request {unit} |",
        "|---|---|---|---|---|---:|---:|---:|---:|---:|---:|"]
    for row in frontier["profiles"]:
        lines.append(f"| {row['scenario']} | {row['backend']} | {row['transport']} | {row.get('selected_contract') or row['contract_cohort']} | {row['byte_exact']} | {cost(row['input'])} | {cost(row['captured_returned_text'])} | {cost(row['serialized_mcp_result'])} | {cost(row['input_plus_serialized_mcp_result'])} | {cost(row['invocation'])} | {cost(row['serialized_tool_request'])} |")
    return lines + ["", "</details>", "",
        "Input plus full JSON-RPC reply is counted independently from input plus captured text. Invocation arguments and serialized tool requests can contain overlapping input data and must not be blindly summed with it. Neither layer includes provider framing or establishes billed usage. These derived minima do not change or replace raw executions.", ""]


def receipt_markdown(receipts):
    unit = receipts["unit"]
    def cost(value):
        return "not captured" if value is None else f"{value:,}"
    def equality(value):
        return "unknown" if value is None else str(value)
    lines = ["## Structured versus text-only MCP receipts", "",
        "Matched MCP strategy profiles execute the same task with default structured receipts and text-only receipts. This comparison counts the canonical serialized **full JSON-RPC reply** captured by the harness. A smaller reply reduces this captured layer only; it does not establish provider billing or complete agent savings. Catalog/schema costs are measured separately and are excluded here.",
        "Only task/profile pairs that are byte-exact in every recorded repetition, have both full-reply counts, and have identical captured inputs and returned text enter the ratios and sums. Missing calls or different content leave the receipt-mode effect unavailable. Text/default is below 1x when the text-only reply is smaller. Input and known returned-text counts remain separate; equal counts alone do not prove identical content.", ""]
    if not receipts["pairs"]:
        return lines + ["No matched text-only receipt profiles were recorded in this run.", ""]
    lines += [f"| Strategy | Qualified pairs | Input cost equal | Known text cost equal | Full reply text/default geometric mean | Structured reply sum {unit} | Text reply sum {unit} | Saved {unit} |",
        "|---|---:|---:|---:|---:|---:|---:|---:|"]
    for row in [*receipts["profiles"], {"default_backend": "All strategy pairs", **receipts["overall"]}]:
        lines.append(f"| {row['default_backend']} | {row['paired_count']}/{row['observed_pairs']} | {row['input_cost_equal_count']}/{row['paired_count']} | {row['captured_returned_text_cost_equal_count']}/{row['paired_count']} | {multiple(row['full_reply_ratio_geomean'])} | {cost(row['default_full_reply_sum'])} | {cost(row['text_full_reply_sum'])} | {cost(row['saved_full_reply_sum'])} |")
    lines += ["", f"The aggregate covers {receipts['overall']['paired_count']} task/profile pairs across {receipts['overall']['task_count']} distinct tasks. Each pair has equal geometric-mean weight. Sums and saved amounts use exactly those qualified pairs.", "",
        "<details><summary>Per-task structured and text-only receipt captures</summary>", "",
        f"| Task | Strategy | Both byte-exact | Input structured / text {unit} | Input cost equal / content identical | Known text structured / text {unit} | Known text cost equal / content identical | Full reply structured / text {unit} | Text/default | Saved {unit} |",
        "|---|---|---|---:|---|---:|---|---:|---:|---:|"]
    for row in receipts["pairs"]:
        lines.append(f"| {row['scenario']} | {row['default_backend']} | {row['both_byte_exact']} | {cost(row['default_input'])} / {cost(row['text_input'])} | {equality(row['input_cost_equal'])} / {equality(row['inputs_identical'])} | {cost(row['default_captured_returned_text'])} / {cost(row['text_captured_returned_text'])} | {equality(row['captured_returned_text_cost_equal'])} / {equality(row['returned_text_identical'])} | {cost(row['default_full_serialized_mcp_reply'])} / {cost(row['text_full_serialized_mcp_reply'])} | {multiple(row['full_reply_ratio'])} | {cost(row['saved_full_reply'])} |")
    return lines + ["", "</details>", "",
        "This receipt comparison does not change input-frontier selection. Aider returned text remains unknown and is outside these matched xnip MCP captures.", ""]


def render(report, stats):
    unit = stats["unit"]
    lines = ["# Native editing tools versus xnip", "",
        f"The real native editors and xnip edited fresh copies of the same {len(report['cases'])} fixtures. No model calls were performed. These results measure captured tool payloads and file behavior, not complete agent sessions or billed tokens.", "",
        f"Counter: {json.dumps(report['counter'], ensure_ascii=False, sort_keys=True)}. Repetitions per task/profile: {report['repeats']}.",
        f"Platform: {report['platform']}. xnip binary SHA-256: {report['xnip_binary_sha256']}.",
        "Raw calls, expected bytes, per-repetition hashes and source provenance: [results.json](results.json). Derived statistics: [relative-results.json](relative-results.json).",
        "Raw execution provenance stays unchanged; derived JSON separately records the sampled and current report-renderer fingerprints.", "",
        *frontier_markdown(stats.get("frontier") or frontier_statistics(report)),
        *receipt_markdown(stats.get("receipt_modes") or receipt_comparisons(report)),
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
    stats["report_generator_sha256"] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    stats["recorded_generator_sha256"] = report.get("adapter_fingerprints", {}).get("benches/native_report.py")
    markdown.parent.mkdir(parents=True, exist_ok=True)
    charts = markdown.parent / "charts"
    charts.mkdir(parents=True, exist_ok=True)
    for name, svg in (("overview", overview_chart(stats, report)),
                      ("input-ratios", ratio_chart(stats)),
                      ("scenario-ratios", scenario_chart(stats, report)),
                      ("byte-exact", correctness_chart(stats)),
                      ("frontier", frontier_chart(stats["frontier"])),
                      ("readme-frontier", readme_frontier_chart(stats["frontier"], report))):
        (charts / (name + ".svg")).write_text(svg, encoding="utf8")
    (markdown.parent / "relative-results.json").write_text(
        json.dumps(stats, ensure_ascii=False, indent=2) + "\n", encoding="utf8")
    markdown.write_text(render(report, stats), encoding="utf8")
