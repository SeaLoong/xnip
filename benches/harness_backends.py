"""Real DSH/Pi/Goose executor profiles and isolated host->xnip exchange lanes.

This module constructs inputs and records actual worker replies. It never edits a
fixture in place of a native tool; the only external write is an explicit stale-
observation perturbation, recorded separately from model calls.
"""
from __future__ import annotations
import hashlib
import json
from pathlib import Path
import time
from edit_cost import dump


def text_content(value):
    if isinstance(value, str):
        return value
    if isinstance(value, list):
        return "\n".join(item.get("text", "") for item in value
                         if isinstance(item, dict) and item.get("type") == "text")
    return None


def check_worker(reply):
    if reply.get("adapter_error") or reply.get("status") == "unavailable":
        raise RuntimeError("native harness infrastructure failed: " + dump(reply))


def tool_catalog(info, family, mode=None):
    if family == "dsh":
        info = next((item for item in info.get("modes", []) if item["mode"] == mode), info)
    tools = info.get("tools", [])
    return list(tools.values()) if isinstance(tools, dict) else tools


def model_catalog(tools):
    """Core model fields only; API output schemas/UI annotations stay separate."""
    return [{"name": tool["name"], "description": tool.get("description", ""),
             "parameters": tool.get("parameters", tool.get("schema", tool.get("inputSchema", {}))),
             **({"deferLoading": tool["deferLoading"]} if "deferLoading" in tool else {})}
            for tool in tools]


class HarnessBackend:
    def __init__(self, name, family, mode, operation, metadata, worker, counter):
        self.name, self.family, self.mode, self.operation = name, family, mode, operation
        self.metadata, self.worker, self.counter = metadata, worker, counter
        self.format = self.line_endings = self.receipt_mode = None
        self.contract_cohort = "snapshot" if operation == "write" else "guarded"
        self.transport = "native-function"
        self.supports_old_guard = operation != "write"
        self.policy_profile = mode if family == "dsh" else "native-editor-kernel"
        self.input_conformance = ("executor-accepted literal substring; str editor whole-line prompt rule not enforced"
            if family == "dsh" and mode.startswith("str-") else "supplied native edit/write contract")
        self.path_configuration = "absolute (required)" if mode.startswith("str-") else "relative to fixture cwd"
        # DSH policies are not pooled with each other or with an external MCP process.
        self.frontier_eligible = family != "dsh"
        self.frontier_exclusion = None if self.frontier_eligible else "DSH policy-specific lane; inspect policy frontiers"
        self.returned_text_boundary_known = family != "goose"
        self.frontier_group = "main" if self.frontier_eligible else "dsh-" + mode

    def reset_case(self, root):
        if self.family != "goose":
            reply = self.worker.request({"action": "reset", "root": str(root), **({"mode": self.mode} if self.family == "dsh" else {})})
            check_worker(reply)
        # Goose EditTools have no session state; bridge requests close their child.

    def setup(self, root):
        if self.family == "dsh":
            info = self.worker.request({"action": "describe-context", "root": str(root), "mode": self.mode})
            check_worker(info)
            if not info.get("ok"):
                raise RuntimeError("native context setup failed: " + dump(info))
            tools = tool_catalog(info, self.family, self.mode)
        else:
            info, tools = self.metadata, tool_catalog(self.metadata, self.family)
        prompt = info.get("prompt")
        if prompt is None and self.family == "pi":
            prompt = [{"name": tool["name"], **{key: tool[key] for key in
                ("promptSnippet", "promptGuidelines") if key in tool}} for tool in tools]
        return {"tool_definitions": tools,
            "model_schema_projection": model_catalog(tools) if self.family != "goose" else None,
            "prompt": prompt, "server_instructions": info.get("server_instructions"),
            "runtime_context": info.get("runtime_context"), "prompt_assembly": info.get("prompt_assembly"),
            "scope": "real native editor-kernel catalog/prompt contributions; full default AgentLoop catalog not constructed",
            "catalog_composition": "native editor fixture only; no claim of complete native+xnip coexistence catalog",
            "public_name_scope": "actual native executor names; Goose provider projection not executed" if self.family == "goose" else "actual native tool registry names",
            "full_jsonrpc_envelope_captured": False}

    def invoke(self, root, tool, arguments, role="edit"):
        reply = self.worker.request({"action": "execute", "root": str(root),
            "mode": self.mode, "tool": tool, "arguments": arguments})
        check_worker(reply)
        visible = reply.get("model_visible_result")
        known = visible is not None
        returned = text_content(visible)
        if returned is None:
            returned = reply.get("output") or reply.get("error") or ""
        if not isinstance(returned, str):
            returned = dump(returned)
        call = {"tool": tool, "input": arguments, "native_result": reply,
            "returned_text": returned, "model_visible_result": visible,
            "returned_text_boundary_known": known,
            "ok": reply.get("ok", False), "elapsed_ms": reply.get("elapsed_ms"),
            "setup_elapsed_ms": reply.get("setup_elapsed_ms", 0),
            "timing_scope": "warm official executor; context setup recorded separately",
            "role": role, "policy_profile": self.policy_profile,
            "model_tool_input_projection": {"name": tool, "arguments": arguments},
            "model_input_projection_scope": "actual tool identity/arguments; provider envelope not captured"}
        if self.family == "goose":
            call["raw_mcp_tool_result"] = reply.get("native_result", reply.get("result"))
        return call

    def read(self, root, name):
        if self.mode.startswith("str-"):
            return self.invoke(root, "str_replace_editor",
                {"command": "view", "path": str(root / name), "view_range": [1, 1]}, "required-read")
        return self.invoke(root, "read",
            {"file_path": name, "offset": 1, "limit": 1}, "required-read")

    def execute(self, root, files, expected, edits, purpose, probe=False):
        setup_started = time.perf_counter()
        setup = self.setup(root)
        setup_ms = (time.perf_counter() - setup_started) * 1000
        calls = []
        grouped = {}
        for edit in edits:
            grouped.setdefault(edit["file"], []).append(edit)
        for name, changes in grouped.items():
            if self.family == "dsh" and self.mode.endswith("observed"):
                calls.append(self.read(root, name))
                if not calls[-1]["ok"]:
                    break
            if self.operation == "write":
                args = {"path": name, "content": expected[name]} if self.family != "dsh" else {
                    "file_path": name, "content": expected[name]}
                calls.append(self.invoke(root, "write", args))
            elif self.family == "pi":
                pairs = ([{"oldText": files[name], "newText": expected[name]}]
                         if any(edit.get("all") for edit in changes) and not probe else
                         [{"oldText": edit["old"], "newText": edit["text"]} for edit in changes])
                calls.append(self.invoke(root, "edit", {"path": name, "edits": pairs}))
            else:
                # The single-edit interfaces get real separate calls. A legal
                # complete-file guard expresses a global change when no all flag exists.
                for edit in changes:
                    old, new = edit["old"], edit["text"]
                    if edit.get("all") and (self.family == "goose" or self.mode.startswith("str-")) and not probe:
                        old, new = files[name], expected[name]
                    if self.family == "goose":
                        tool, args = "edit", {"path": name, "before": old, "after": new}
                    elif self.mode.startswith("str-"):
                        tool, args = "str_replace_editor", {"command": "str_replace",
                            "path": str(root / name), "old_str": old, "new_str": new}
                    else:
                        tool, args = "edit", {"file_path": name, "old_string": old,
                            "new_string": new, **({"replace_all": True} if edit.get("all") else {})}
                    calls.append(self.invoke(root, tool, args))
                    if not calls[-1]["ok"]:
                        break
            if not calls[-1]["ok"]:
                break
        if calls:
            calls[0]["host_setup"] = setup
            calls[0]["setup_elapsed_ms"] += setup_ms
        return calls

    def policy_probe(self, root, case):
        name = next(iter(case["files"]))
        calls = []
        kind = case["policy_probe"]
        if kind != "unread":
            calls.append(self.read(root, name))
        if kind == "stale":
            (root / name).write_bytes(case["external_text"].encode("utf8"))
        if kind == "new-session":
            self.reset_case(root)
        edit = case["edits"][0]
        if self.mode.startswith("str-"):
            tool, args = "str_replace_editor", {"command": "str_replace", "path": str(root / name),
                "old_str": edit["old"], "new_str": edit["text"]}
        else:
            tool, args = "edit", {"file_path": name, "old_string": edit["old"], "new_string": edit["text"]}
        calls.append(self.invoke(root, tool, args))
        calls[-1]["control_events"] = ([{"type": "external-fixture-write", "path": name,
            "sha256": hashlib.sha256(case["external_text"].encode("utf8")).hexdigest()}] if kind == "stale" else
            [{"type": "dispose-and-create-new-session"}] if kind == "new-session" else [])
        return calls


class HostBridgeBackend(HarnessBackend):
    def __init__(self, family, cohort, metadata, worker, counter, executable, plan_builder, choose_plan):
        super().__init__(family + "-xnip-mcp-" + cohort, family, "xnip-mcp", "bridge", metadata, worker, counter)
        self.contract_cohort, self.transport = cohort, "host-mcp-bridge"
        self.executable = str(Path(executable).resolve())
        self.plan_builder, self.choose_plan = plan_builder, choose_plan
        self.frontier_eligible = False
        self.frontier_exclusion = "host exchange lane; never pooled with editor-input frontier"
        self.frontier_group = "host-exchange"
        self.returned_text_boundary_known = family != "goose"
        self.receipt_mode = "text"

    def setup(self, root):
        request = {"action": "describe-context" if self.family == "dsh" else "describe",
            "root": str(root), "mode": "xnip-mcp", "xnipExecutable": self.executable}
        if self.family == "goose":
            request = {"action": "bridge", "root": str(root), "command": self.executable,
                "arguments": ["mcp", "--receipts", "text"], "calls": []}
        reply = self.worker.request(request)
        check_worker(reply)
        if not reply.get("ok"):
            raise RuntimeError("host bridge setup failed: " + dump(reply))
        tools = reply.get("tools", [])
        if isinstance(tools, dict):
            tools = list(tools.values()) if "tools" not in tools else tools["tools"]
        instructions = reply.get("server_instructions")
        if instructions is None:
            instructions = (reply.get("server_info") or {}).get("instructions")
        return {"tool_definitions": tools, "model_schema_projection": model_catalog(tools) if self.family != "goose" else None,
            "prompt": reply.get("prompt"), "server_instructions": instructions,
            "runtime_context": reply.get("runtime_context"), "prompt_assembly": reply.get("prompt_assembly"),
            "scope": "real host bridge tool definitions and prompt contributions; no provider request/billing",
            "public_name_scope": "Goose MCP client raw names; extension-manager/provider names not executed" if self.family == "goose" else "real host public tool names",
            "full_jsonrpc_envelope_captured": False}

    def execute(self, root, files, expected, edits, purpose, probe=False):
        setup_started = time.perf_counter()
        self.last_setup = self.setup(root)
        setup_ms = (time.perf_counter() - setup_started) * 1000
        plans = self.plan_builder(files, expected, edits, self.contract_cohort == "snapshot", probe, self.counter)
        selected, selection = self.choose_plan(plans, self.counter)
        calls = []
        if self.family == "goose":
            reply = self.worker.request({"action": "bridge", "root": str(root), "command": self.executable,
                "arguments": ["mcp", "--receipts", "text"],
                "calls": [{"name": name, "arguments": args} for name, args in selected["calls"]]})
            check_worker(reply)
            if not reply.get("ok"):
                raise RuntimeError("Goose native client bridge failed: " + dump(reply))
            for planned, item in zip(selected["calls"], reply.get("calls", [])):
                name, args = planned
                result = item["result"]
                calls.append({"tool": name, "input": args, "native_result": item,
                    "raw_mcp_tool_result": result, "returned_text": text_content(result.get("content", [])),
                    "model_visible_result": None, "returned_text_boundary_known": False,
                    "ok": item["ok"], "elapsed_ms": item.get("elapsed_ms"),
                    "role": "edit", "public_name_scope": self.last_setup["public_name_scope"]})
            if len(calls) != len(selected["calls"]):
                raise RuntimeError("Goose bridge did not return every submitted call")
        else:
            for name, args in selected["calls"]:
                call = self.invoke(root, name, args)
                if self.family == "dsh":
                    call["tool"] = "mcp__xnip__" + name
                elif call["native_result"].get("metadata", {}).get("public_tool_name"):
                    call["tool"] = call["native_result"]["metadata"]["public_tool_name"]
                call["model_tool_input_projection"] = {"name": call["tool"], "arguments": args}
                calls.append(call)
                if not call["ok"]:
                    break
        if calls:
            calls[0].update(selection=selection, setup_elapsed_ms=setup_ms,
                host_setup=self.last_setup, timing_scope="real host->xnip bridge; initialization separate")
        return calls

    def invoke(self, root, tool, arguments, role="edit"):
        reply = self.worker.request({"action": "execute", "root": str(root),
            "mode": "xnip-mcp", "tool": tool, "arguments": arguments,
            "xnipExecutable": self.executable, "options": {"xnipExecutable": self.executable, "receipts": "text"}})
        check_worker(reply)
        visible = reply.get("model_visible_result")
        text = text_content(visible)
        return {"tool": tool, "input": arguments, "native_result": reply,
            "returned_text": text if text is not None else reply.get("output") or reply.get("error") or "",
            "model_visible_result": visible, "returned_text_boundary_known": visible is not None,
            "ok": reply.get("ok", False), "elapsed_ms": reply.get("elapsed_ms"),
            "setup_elapsed_ms": reply.get("setup_elapsed_ms", 0), "role": role}


def register_harnesses(Worker, scope, workers, args, counter, metadata, unavailable, plan_builder, choose_plan, root):
    native, exchanges = [], []
    enabled = set(filter(None, args.harnesses.split(",")))
    selected = getattr(args, "only_profiles", None)
    if selected:
        requested = set(selected.split(","))
        enabled = {family for family in enabled if any(name.startswith(family + "-") for name in requested)}
    commands = {"dsh": ["node", str(root / "benches/native/dsh_adapter.mjs")],
        "pi": ["node", str(root / "benches/native/pi_adapter.mjs")],
        "goose": [__import__("sys").executable, str(root / "benches/native/goose_adapter.py")]}
    for family, command in commands.items():
        if family not in enabled:
            continue
        try:
            worker = Worker(command)
            workers.append(worker)
            scope.callback(worker.close)
            info = worker.request({"action": "describe", "xnipExecutable": str(args.xnip.resolve())})
            check_worker(info)
            if info.get("status") != "available":
                raise RuntimeError(dump(info))
            metadata[family] = info
            profiles = ([("fs-observed", "edit"), ("fs-observed", "write"), ("fs-bare", "edit"),
                         ("fs-bare", "write"), ("str-observed", "edit"), ("str-bare", "edit")]
                        if family == "dsh" else [("native", "edit"), ("native", "write")])
            for mode, operation in profiles:
                if family == "dsh" and not any(item["mode"] == mode and item.get("status") == "available" for item in info.get("modes", [])):
                    unavailable.append({"name": family + "-" + mode, "reason": "official runtime mode unavailable"})
                    continue
                native.append(HarnessBackend(f"{family}-{mode}-{operation}", family, mode, operation, info, worker, counter))
            if not args.skip_host_exchange:
                exchanges.extend(HostBridgeBackend(family, cohort, info, worker, counter, args.xnip,
                    plan_builder, choose_plan) for cohort in ("guarded", "snapshot"))
        except (OSError, RuntimeError, __import__("queue").Empty) as error:
            unavailable.append({"name": family, "reason": str(error)[:1500]})
    return native, exchanges


def policy_probe_cases():
    name = "src/example.txt"
    return [
        {"name": "dsh-read-before-edit", "policy_probe": "unread", "files": {name: "value=1\n"},
         "expected": {name: "value=2\n"}, "edits": [{"file": name, "old": "value=1\n", "text": "value=2\n"}],
         "purpose": "Actual unread mutation; observation policies may reject, bare kernels may accept."},
        {"name": "dsh-stale-after-read", "policy_probe": "stale", "files": {name: "value=1\n"},
         "external_text": "external-version=2\n", "expected": {name: "external-version=2\n"},
         "edits": [{"file": name, "old": "external-version=2\n", "text": "value=3\n"}],
         "purpose": "Real read, recorded external write, then matching-current-old mutation with stale observed version."},
        {"name": "dsh-observation-new-session", "policy_probe": "new-session", "files": {name: "value=1\n"},
         "expected": {name: "value=2\n"}, "edits": [{"file": name, "old": "value=1\n", "text": "value=2\n"}],
         "purpose": "A real disposal/new Session must not inherit another Session's observations."},
    ]
