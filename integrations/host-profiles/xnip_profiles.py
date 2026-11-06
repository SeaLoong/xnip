#!/usr/bin/env python3
"""Generate reviewable host MCP configuration files with an explicit working directory.

The output is a new directory. Applications keep ownership of their settings,
tool permissions, server approval and removal lifecycle.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import tomllib


HOST_FILES = {
    "codex": "config.toml",
    "claude-code": ".mcp.json",
    "cursor": ".cursor/mcp.json",
    "vscode": ".vscode/mcp.json",
    "copilot": ".vscode/mcp.json",
    "copilot-cli": "mcp-config.json",
    "gemini-cli": ".gemini/settings.json",
    "qwen-code": ".qwen/settings.json",
    "roo-code": ".roo/mcp.json",
    "cline-gui": "cline_mcp_settings.json",
    "windsurf": "mcp_config.json",
    "goose": "config.yaml",
    "continue": ".continue/mcpServers/xnip.yaml",
    "zed": "settings.json",
}


def render(host: str, executable: Path, project: Path) -> str:
    """Return the host's documented configuration shape for one xnip server."""
    server = {"command": str(executable),
              "args": ["mcp", "--cwd", str(project), "--receipts", "text"],
              "env": {"XNIP_AUTO_UPDATE": "0"}}
    if host == "codex":
        quote = lambda value: json.dumps(value, ensure_ascii=False)
        return ("[mcp_servers.xnip]\n"
                f"command = {quote(server['command'])}\n"
                f"args = {quote(server['args'])}\n"
                f"cwd = {quote(str(project))}\n"
                "env = { XNIP_AUTO_UPDATE = \"0\" }\n")
    if host in {"vscode", "copilot"}:
        document = {"servers": {"xnip": {"type": "stdio", **server, "cwd": str(project)}}}
    elif host == "goose":
        document = {"extensions": {"xnip": {"name": "xnip", "type": "stdio",
                    "cmd": server["command"], "args": server["args"],
                    "envs": server["env"], "cwd": str(project), "enabled": True,
                    "timeout": 300}}}
    elif host == "continue":
        document = {"name": "xnip", "version": "1.0.0", "schema": "v1",
                    "mcpServers": [{"name": "xnip", **server, "cwd": str(project)}]}
    elif host == "zed":
        document = {"context_servers": {"xnip": server}}
    else:
        if host == "copilot-cli":
            server = {"type": "stdio", **server, "tools": ["*"]}
        elif host == "roo-code":
            server = {**server, "disabled": False, "alwaysAllow": []}
        elif host == "cline-gui":
            server = {**server, "disabled": False, "autoApprove": []}
        document = {"mcpServers": {"xnip": server}}
    # JSON is also YAML 1.2, accepted by Goose and Continue's YAML loaders.
    return json.dumps(document, ensure_ascii=False, indent=2) + "\n"


def launch(host: str, filename: Path) -> dict:
    """Extract the actual launch from an artifact, for inspection and testing."""
    text = filename.read_text(encoding="utf-8")
    if host == "codex":
        server = tomllib.loads(text)["mcp_servers"]["xnip"]
    else:
        document = json.loads(text)
        if host in {"vscode", "copilot"}:
            server = document["servers"]["xnip"]
        elif host == "goose":
            server = document["extensions"]["xnip"]
            return {"command": server["cmd"], "args": server["args"],
                    "env": server["envs"], "cwd": server["cwd"]}
        elif host == "continue":
            server = next(item for item in document["mcpServers"] if item["name"] == "xnip")
        elif host == "zed":
            server = document["context_servers"]["xnip"]
        else:
            server = document["mcpServers"]["xnip"]
    return {key: server[key] for key in ("command", "args", "env", "cwd") if key in server}


def generate(executable: Path, project: Path, output: Path, hosts=None) -> dict:
    executable, project, output = executable.resolve(strict=True), project.resolve(strict=True), output.resolve()
    if not project.is_dir():
        raise ValueError("project must select an existing directory")
    selected = list(dict.fromkeys(hosts or HOST_FILES))
    for host in selected:
        if host not in HOST_FILES:
            raise ValueError(f"Choose a supported host: {host}")
    output.mkdir(parents=True, exist_ok=False)
    entries = {}
    for host in selected:
        filename = output / host / HOST_FILES[host]
        filename.parent.mkdir(parents=True, exist_ok=True)
        filename.write_text(render(host, executable, project), encoding="utf-8")
        entries[host] = {"file": str(filename), "launch": launch(host, filename)}
    return {"project": str(project), "output": str(output), "profiles": entries}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    command = commands.add_parser("generate", help="Write a new directory of isolated configuration artifacts")
    command.add_argument("--executable", type=Path, required=True)
    command.add_argument("--project", type=Path, required=True)
    command.add_argument("--output", type=Path, required=True)
    command.add_argument("--host", choices=HOST_FILES, action="append")
    inspect = commands.add_parser("inspect", help="Print the xnip launch selected from a generated profile")
    inspect.add_argument("--host", choices=HOST_FILES, required=True)
    inspect.add_argument("--file", type=Path, required=True)
    args = parser.parse_args()
    result = generate(args.executable, args.project, args.output, args.host) if args.command == "generate" else launch(args.host, args.file)
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
