#!/usr/bin/env python3
"""Prepare pinned official CLIs used for profile-parser verification."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
INPUTS = Path(__file__).with_name("host_profiles_clis")
HOST = ROOT / "target/native-hosts/profile-clis"
VERSIONS = {"codex": "0.167.0", "claude-code": "2.1.318", "copilot-cli": "1.0.106"}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def commands(directory=HOST):
    modules = Path(directory).resolve() / "node_modules"
    node = shutil.which("node")
    if node is None:
        raise RuntimeError("Activate Node before preparing profile CLIs")
    return {
        "codex": [node, str(modules / "@openai/codex/bin/codex.js")],
        "claude-code": [str(modules / "@anthropic-ai/claude-code/bin/claude.exe")],
        "copilot-cli": [node, str(modules / "@github/copilot/npm-loader.js")],
    }


def prepare(directory=HOST, check=False):
    directory = Path(directory).resolve()
    if not check:
        directory.mkdir(parents=True, exist_ok=True)
        for name in ("package.json", "package-lock.json"):
            shutil.copyfile(INPUTS / name, directory / name)
        npm = shutil.which("npm")
        if npm is None:
            raise RuntimeError("Activate npm before preparing profile CLIs")
        subprocess.run([npm, "ci", "--prefix", str(directory), "--ignore-scripts",
                        "--no-audit", "--no-fund", "--prefer-offline",
                        "--registry=https://registry.npmjs.org", "--strict-ssl=true"], check=True)
        # This audited wrapper script only places the optional platform binary.
        subprocess.run([shutil.which("node"), str(directory / "node_modules/@anthropic-ai/claude-code/install.cjs")], check=True)
    for name in ("package.json", "package-lock.json"):
        if (directory / name).read_bytes() != (INPUTS / name).read_bytes():
            raise AssertionError(f"Prepared CLI {name} differs from its frozen input")
    records = {}
    for host, command in commands(directory).items():
        result = subprocess.run([*command, "--version"], check=True, capture_output=True, text=True,
                                env={**os.environ, "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
                                     "COPILOT_TELEMETRY_DISABLED": "true"}, timeout=30)
        if VERSIONS[host] not in result.stdout:
            raise AssertionError(f"Unexpected {host} version")
        records[host] = {"version": VERSIONS[host], "version_output": result.stdout.strip(),
                         "entry_sha256": sha(Path(command[-1]).read_bytes())}
    package = directory / "node_modules/@continuedev/config-yaml/package.json"
    if json.loads(package.read_text())["version"] != "1.23.0":
        raise AssertionError("Unexpected Continue configuration parser version")
    records["continue-schema"] = {"version": "1.23.0", "package_sha256": sha(package.read_bytes())}
    record = {"lock_sha256": sha((INPUTS / "package-lock.json").read_bytes()), "hosts": records}
    receipt = directory / "native-host.json"
    if check:
        if json.loads(receipt.read_text()) != record:
            raise AssertionError("Official CLI entry bytes changed; run setup")
    else:
        receipt.write_text(json.dumps(record, indent=2) + "\n")
    return record


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--destination", type=Path, default=HOST)
    args = parser.parse_args()
    print(json.dumps(prepare(args.destination, args.check), indent=2))
