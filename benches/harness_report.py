"""Independent host-bridge cost layers; no model/provider usage is inferred."""
from __future__ import annotations
import json
from pathlib import Path


def exchange_cases(report):
    native = [{**case, "results": [row for row in case["results"] if
        row["family"] in {"dsh", "pi", "goose"} and row.get("host_setup") is not None]}
        for case in report.get("cases", [])]
    return [*native, *report.get("host_exchanges", [])]


def summarize_host_exchanges(report):
    return {"schema_version": 1, "counter": report["counter"],
        "scope": "real native editor-kernel and host->xnip MCP projections; no provider requests or billing",
        "disclosure_redaction": {"source":"results.json#/disclosure_redaction",
            "scope":report.get("disclosure_redaction", {}).get("scope"),
            "cost_scope":report.get("disclosure_redaction", {}).get("cost_scope"),
            "original_values_retained":False},
        "case_count": len({case["name"] for case in exchange_cases(report)}),
        "records": [{"scenario": case["name"], "backend": row["backend"], "host": row["family"],
            "contract_cohort": row["contract_cohort"], "selected_contract": row.get("selected_contract"),
            "lane": "xnip-bridge" if row.get("transport") == "host-mcp-bridge" else "native-editor",
            "byte_exact": bool(row["outcomes"]) and all(item["byte_exact"] for item in row["outcomes"]),
            "input_cost": row["input_cost"], "read_call_count": row.get("read_call_count", 0),
            "edit_call_count": row.get("edit_call_count", len(row.get("calls", []))),
            "layers": row.get("host_layer_costs"), "setup": row.get("host_setup"),
            "returned_text_boundary_known": row.get("returned_text_boundary_known", False),
            "source_provenance": row.get("source_provenance"),
            "calls": row.get("calls"), "outcomes": row["outcomes"]}
            for case in exchange_cases(report) for row in case["results"]]}


def host_exchange_markdown(report):
    measured = summarize_host_exchanges(report)
    if not measured["records"]:
        return []
    unit = report["counter"]["unit"]
    def cost(value):
        return "not captured" if value is None else str(value.get(unit))
    lines = ["## Host MCP exchange layers", "",
        "These lanes retain native editor-kernel catalogs and execute xnip through the official host transport/bridge. They are separate from the editor-input frontier. "
        "Public names, core schema projections, server instructions, actual model-rendered content, raw MCP tool-result DTOs and "
        "internal native values remain independent layers. No provider request, billing or complete AgentLoop was run. "
        "API catalogs and internal DTOs must not be charged as model content; unknown projections stay unknown. "
        "Schemas/instructions are setup contributions, not repeated N times without a recorded model-request schedule. Native editor-only and xnip bridge-only fixture catalogs are disclosed; complete default AgentLoop, coexistence and replacement compositions have not been executed.", "",
        f"| Task | Host/profile | Byte exact | Executor input {unit} | Model schema projection | Prompt contributions | Server instructions | Model rendered content | Raw MCP result DTO | Full JSON-RPC envelope |",
        "|---|---|---|---:|---:|---:|---:|---:|---:|---:|"]
    for row in measured["records"]:
        layers = row["layers"] or {}
        lines.append(f"| {row['scenario']} | {row['backend']} | {row['byte_exact']} | {cost(row['input_cost'])} | "
            f"{cost(layers.get('model_schema_projection'))} | {cost(layers.get('prompt_contributions'))} | "
            f"{cost(layers.get('server_instructions'))} | {cost(layers.get('model_rendered_content'))} | "
            f"{cost(layers.get('raw_mcp_tool_result'))} | {cost(layers.get('full_jsonrpc_reply'))} |")
    lines += ["", "Goose's lane executes its official MCP client with raw server tool names. Its extension-manager/provider name and "
        "model-result projection are not executed and remain unavailable. DSH/Pi lanes execute the official native bridge result "
        "projection with fixture session contexts; they do not reproduce a paid/full agent session.", "",
        "Complete captured definitions, instructions, calls and independent layer counts are in "
        "[host-exchange-results.json](host-exchange-results.json).", ""]
    return lines


def write_host_exchanges(report, directory):
    if not any(case["results"] for case in exchange_cases(report)):
        return
    Path(directory, "host-exchange-results.json").write_text(
        json.dumps(summarize_host_exchanges(report), ensure_ascii=False, indent=2) + "\n", encoding="utf8")
