"""Actual configured MCP processes and official CLI configuration readers."""
from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import tempfile
import threading

ROOT = Path(__file__).resolve().parents[1]


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


profiles = module("xnip_profiles", ROOT / "integrations/host-profiles/xnip_profiles.py")
setup = module("host_profiles_setup", ROOT / "benches/native/host_profiles_setup.py")


class ProfileMcp:
    def __init__(self, configuration, project):
        self.diagnostics = tempfile.TemporaryFile()
        self.process = subprocess.Popen(
            [configuration["command"], *configuration["args"]], cwd=project,
            env={**os.environ, **configuration.get("env", {}), "XNIP_AUTO_UPDATE": "0"},
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.diagnostics,
            bufsize=0,
        )
        self.next_id = 0
        self.messages = queue.Queue()
        self.reader = threading.Thread(target=self._receive, daemon=True)
        self.reader.start()
        try:
            self.initialize = self.request("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "xnip-host-profiles", "version": "1"}})
            self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})
            self.catalog = self.request("tools/list", {})["tools"]
        except Exception:
            self.close()
            raise

    def _receive(self):
        try:
            for line in self.process.stdout:
                self.messages.put((json.loads(line), len(line)))
        except Exception as error:
            self.messages.put(error)
        finally:
            self.messages.put(EOFError("Configured MCP process closed"))

    def _send(self, request):
        serialized = (json.dumps(request, ensure_ascii=False, separators=(",", ":")) + "\n").encode("utf-8")
        self.last_request_bytes = len(serialized)
        self.process.stdin.write(serialized)
        self.process.stdin.flush()

    def request(self, method, params):
        self.next_id += 1
        self.last_request = {"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params}
        self._send(self.last_request)
        while True:
            item = self.messages.get(timeout=20)
            if isinstance(item, BaseException):
                raise item
            reply, wire_bytes = item
            if reply.get("id") == self.next_id:
                self.last_response = reply
                self.last_response_bytes = wire_bytes
                if "error" in reply:
                    raise RuntimeError(reply["error"])
                return reply["result"]

    def call(self, name, arguments):
        return self.request("tools/call", {"name": name, "arguments": arguments})

    def close(self):
        if not self.process.stdin.closed:
            self.process.stdin.close()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)
        self.reader.join(timeout=5)
        self.process.stdout.close()
        self.diagnostics.close()
        if self.process.returncode:
            raise RuntimeError(f"Configured MCP subprocess exited {self.process.returncode}")

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def verify_cli(host, configuration, directory):
    """Ask the real CLI to read one isolated server entry; no agent session."""
    directory = Path(directory).resolve()
    directory.mkdir(parents=True, exist_ok=True)
    command = setup.commands()[host]
    environment = {**os.environ, "XNIP_AUTO_UPDATE": "0"}
    if host == "codex":
        # Only this explicitly supplied server is returned; user/server secrets
        # from unrelated Codex configuration are outside the captured result.
        quote = lambda value: json.dumps(value, ensure_ascii=False)
        entry = ("mcp_servers.xnip_profile_verification={command=" + quote(configuration["command"])
                 + ",args=" + quote(configuration["args"]) + ",env={XNIP_AUTO_UPDATE=\"0\"}}")
        argv = [*command, "-c", entry, "mcp", "get", "xnip_profile_verification", "--json"]
    elif host == "claude-code":
        (directory / ".mcp.json").write_text(json.dumps({"mcpServers": {"xnip": configuration}}))
        environment.update(CLAUDE_CONFIG_DIR=str(directory / "claude-state"),
                           CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1", DISABLE_AUTOUPDATER="1")
        argv = [*command, "--setting-sources", "project", "mcp", "get", "xnip"]
    else:
        (directory / "mcp-config.json").write_text(json.dumps({"mcpServers": {"xnip": {"type": "stdio", **configuration, "tools": ["*"]}}}))
        environment.update(COPILOT_HOME=str(directory), COPILOT_TELEMETRY_DISABLED="true")
        argv = [*command, "mcp", "get", "xnip", "--json"]
    result = subprocess.run(argv, cwd=directory, env=environment, capture_output=True, text=True, timeout=30)
    if result.returncode:
        raise AssertionError(f"{host} rejected its isolated xnip configuration (exit {result.returncode})")
    if host == "claude-code":
        if configuration["command"] not in result.stdout or "Pending approval" not in result.stdout:
            raise AssertionError("Claude project entry must remain under its ordinary server-approval policy")
        return {"host": host, "version": setup.VERSIONS[host], "scope": "official CLI project-config reader",
                "server_status": "pending-user-approval", "model_calls": 0}
    actual = json.loads(result.stdout)
    transport = actual["transport"] if host == "codex" else actual["xnip"]
    if transport["command"] != configuration["command"] or transport["args"] != configuration["args"]:
        raise AssertionError(f"{host} selected a different server launch")
    return {"host": host, "version": setup.VERSIONS[host], "scope": "official CLI config reader", "model_calls": 0}


def verify_continue(filename):
    """Use Continue's published schema to validate the generated server block."""
    script = """
import {createRequire} from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
const require=createRequire(path.join(process.argv[1], 'package.json'));
const {configYamlSchema}=await import(pathToFileURL(require.resolve('@continuedev/config-yaml')).href);
const config=configYamlSchema.parse(JSON.parse(fs.readFileSync(process.argv[2], 'utf8')));
if(config.mcpServers.length!==1 || config.mcpServers[0].name!=='xnip') throw Error('Expected one xnip server');
console.log(JSON.stringify({host:'continue',version:'1.23.0',scope:'official published configuration schema',model_calls:0}));
"""
    result = subprocess.run([shutil.which("node"), "--input-type=module", "-e", script,
                             str(setup.HOST), str(filename)],
                            check=True, capture_output=True, text=True, timeout=20)
    return json.loads(result.stdout)
