"""Run the host and SDK integration matrix and retain actual test evidence.

Prepare official DSH hosts and the pinned SDK dependencies, then run
python3 tests/native_e2e.py. Each suite loads real tools and checks the registry,
disk bytes or installation lifecycle; model generation stays at zero.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
VERSIONS = ("0.2.1-rc.2", "0.2.1-rc.5")
SUITES = (
    ("provider", "tests/dsh_native_contract.mjs", 2),
    ("bytes", "tests/dsh_native_bytes.mjs", 13),
    ("rewind", "tests/dsh_native_rewind.mjs", 2),
    ("packaged", "tests/dsh_native_packaged.mjs", 2),
)
PI_SUITES = (
    ("registry-and-tools", "tests/pi_native_contract.mjs", 10),
    ("bytes", "tests/pi_native_bytes.mjs", 13),
    ("packaged-and-coexistence", "tests/pi_native_packaged.mjs", 1),
)
GEMINI_SUITES = (
    ("registry-and-tools", "tests/gemini_native_contract.mjs", 10),
    ("bytes", "tests/gemini_native_bytes.mjs", 13),
    ("packaged-and-restoration", "tests/gemini_native_packaged.mjs", 1),
)
CLINE_SUITES = (
    ("registry-and-tools", "tests/cline_native_contract.mjs", 10),
    ("bytes", "tests/cline_native_bytes.mjs", 13),
    ("packaged-and-restoration", "tests/cline_native_packaged.mjs", 1),
)
NODE_SDKS = {
    "pi": ("1.3.0", PI_SUITES),
    "gemini": ("0.27.2", GEMINI_SUITES),
    "cline": ("0.2.0", CLINE_SUITES),
    "opencode": ("1.19.3", (
        ("registry-permissions-and-tools", "tests/opencode_native_contract.mjs", 11),
        ("bytes", "tests/opencode_native_bytes.mjs", 13),
        ("packaged-and-restoration", "tests/opencode_native_packaged.mjs", 1),
    )),
}
PYTHON_SUITES = {
    "openhands": ("1.60.1", "target/native-hosts/openhands-1.60.1/.venv", "tests/openhands_native_contract.py", 24),
    "aider": ("5dc9490", "target/native-hosts/aider-5dc9490/aider-venv", "tests/aider_native_contract.py", 24),
    "python-agents": ("deepagents-langgraph-pydantic-ai", "target/native-hosts/python-agents/.venv", "tests/python_agents_contract.py", 49),
    "agents-sdk": ("0.24.0", "target/native-hosts/agent-sdks-openai-0.24.0-autogen-0.7.5/.venv", "tests/agents_sdk_contract.py", 21),
    "autogen": ("0.7.5", "target/native-hosts/agent-sdks-openai-0.24.0-autogen-0.7.5/.venv", "tests/autogen_contract.py", 21),
    "smolagents": ("1.26.0", "target/native-hosts/smolagents-1.26.0/.venv", "tests/smolagents_contract.py", 22),
    "profiles": ("official-client-profiles", None, "tests/host_profiles_contract.py", 33),
    "google-adk": ("2.13.0", "target/native-hosts/google-adk-2.13.0/.venv", "tests/google_adk_contract.py", 20),
    "crewai": ("1.15.36", "target/native-hosts/crewai-1.15.36/.venv", "tests/crewai_contract.py", 18),
}


def execute(command, environment, log, input_text=None):
    result = subprocess.run(
        command, cwd=ROOT, env=environment, capture_output=True, text=True,
        encoding="utf-8", timeout=300, input=input_text,
    )
    output = result.stdout + result.stderr
    log.write_text(output, encoding="utf-8")
    if result.returncode:
        raise RuntimeError(f"{' '.join(command)} exited {result.returncode}\n{output}")
    return output


def require_tap(output, expected):
    fields = ("tests", "suites", "pass", "fail", "cancelled", "skipped", "todo")
    counts = {}
    for name in fields:
        matches = re.findall(rf"^# {name} (\d+)[ \t]*\r?$", output, re.MULTILINE)
        if len(matches) != 1:
            raise AssertionError(f"TAP result requires one {name!r} count; found {len(matches)}")
        counts[name] = int(matches[0])
    footer = r"\r?\n".join(rf"# {name} \d+[ \t]*" for name in fields)
    if re.search(rf"^{footer}\r?\n# duration_ms [^\r\n]+\s*\Z", output, re.MULTILINE) is None:
        raise AssertionError("TAP counts must form the complete final result summary")
    required = {"tests": expected, "pass": expected, "fail": 0, "cancelled": 0, "skipped": 0, "todo": 0}
    if any(counts[name] != value for name, value in required.items()):
        raise AssertionError(f"Expected {expected} completed tests, got {counts}")
    return {name: counts[name] for name in ("tests", "pass", "fail", "skipped")}


def require_unittest(output, expected):
    summaries = list(re.finditer(r"^Ran (\d+) tests? in [^\r\n]+\r?$", output, re.MULTILINE))
    if (len(summaries) != 1 or summaries[0].group(1) != str(expected)
            or output[summaries[0].end():].strip() != "OK"):
        raise AssertionError(f"Expected {expected} completed unittest cases\n{output}")
    return {"tests": expected, "pass": expected, "fail": 0, "skipped": 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", action="append", choices=VERSIONS)
    parser.add_argument("--host", action="append", choices=("dsh", *NODE_SDKS, *PYTHON_SUITES))
    parser.add_argument("--hosts-dir", type=Path, default=ROOT / "target/native-hosts")
    args = parser.parse_args()
    versions = list(dict.fromkeys(args.version or VERSIONS))
    hosts = list(dict.fromkeys(args.host or ("dsh", *NODE_SDKS, *PYTHON_SUITES)))
    environment = dict(os.environ, XNIP_AUTO_UPDATE="0", XNIP_TEST_PYTHON=sys.executable)
    node = shutil.which("node")
    cargo = shutil.which("cargo")
    if node is None or cargo is None:
        raise RuntimeError("Activate Node and Cargo before running native E2E")
    binary = ROOT / "target/debug" / ("xnip.exe" if os.name == "nt" else "xnip")
    evidence = ROOT / "target/native-e2e"
    evidence.mkdir(parents=True, exist_ok=True)
    result_path = evidence / "results.json"
    result_path.unlink(missing_ok=True)
    execute([cargo, "build", "--locked", "--offline"], environment, evidence / "build.log")
    binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    pinned = evidence / "bin" / binary_hash / binary.name
    pinned.parent.mkdir(parents=True, exist_ok=True)
    if not pinned.exists():
        shutil.copy2(binary, pinned)
    assert hashlib.sha256(pinned.read_bytes()).hexdigest() == binary_hash
    binary = pinned
    environment.update(XNIP_TEST_BINARY=str(binary), XNIP_DSH_NODE=node)
    capability_output = execute(
        [str(binary), "native", "serve"], environment, evidence / "startup.log",
        input_text='{"id":1,"protocol":1,"action":"capabilities"}\n',
    )
    capabilities = json.loads(capability_output)["result"]
    assert "utf8" in capabilities["content_encodings"]
    report = {"binary_sha256": binary_hash, "binary": str(binary), "versions": []}
    report["capabilities"] = capabilities
    for version in versions if "dsh" in hosts else []:
        host = args.hosts_dir.resolve() / f"dsh-{version}"
        manifest = host / "node_modules/@deepseek-ai/dsh/package.json"
        installed = json.loads(manifest.read_text(encoding="utf-8"))
        if installed["version"] != version:
            raise AssertionError(f"Host {host} resolves {installed['version']}, expected {version}")
        environment["XNIP_DSH_NATIVE_HOST"] = str(host)
        directory = evidence / version
        directory.mkdir(exist_ok=True)
        checks = []
        for name, file, expected in SUITES:
            output = execute([node, "--test-reporter=tap", file], environment, directory / f"{name}.log")
            checks.append({"suite": name, **require_tap(output, expected)})
            print(f"DSH {version}: {name} {expected}/{expected}", flush=True)
        output = execute(
            [cargo, "test", "--test", "dsh_native_lifecycle_contract", "--locked", "--offline",
             "--", "--ignored", "--nocapture"],
            environment, directory / "lifecycle.log",
        )
        result = re.search(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored", output)
        if result is None or result.groups() != ("1", "0", "0"):
            raise AssertionError("The real-host lifecycle test must execute and pass")
        checks.append({"suite": "lifecycle", "tests": 1, "pass": 1, "phases": 5})
        environment["XNIP_NPM_DSH_EVIDENCE"] = str(directory / "npm-installed-lifecycle.json")
        output = execute(
            [sys.executable, "tests/npm_dsh_lifecycle.py"], environment,
            directory / "npm-installed-lifecycle.log",
        )
        checks.append({"suite": "npm-installed-lifecycle", **require_unittest(output, 3)})
        print(f"DSH {version}: npm-installed-lifecycle 3/3", flush=True)
        report["versions"].append({"host": version, "checks": checks})
        print(f"DSH {version}: lifecycle 5/5 phases", flush=True)
    for host, (version, suites) in NODE_SDKS.items():
        if host not in hosts:
            continue
        directory = evidence / f"{host}-{version}"
        directory.mkdir(exist_ok=True)
        checks = []
        for name, file, expected in suites:
            output = execute([node, "--test-reporter=tap", file], environment, directory / f"{name}.log")
            checks.append({"suite": name, **require_tap(output, expected)})
            print(f"{host} {version}: {name} {expected}/{expected}", flush=True)
        report[host] = {"host": version, "checks": checks}
    for host, (version, venv, file, expected) in PYTHON_SUITES.items():
        if host not in hosts:
            continue
        interpreter = ROOT / venv / ("Scripts/python.exe" if os.name == "nt" else "bin/python") if venv else Path(sys.executable)
        directory = evidence / f"{host}-{version}"
        directory.mkdir(exist_ok=True)
        output = execute(
            [str(interpreter), file],
            environment, directory / "registry-tools-and-bytes.log",
        )
        checks = [{"suite": "registry-tools-and-bytes", **require_unittest(output, expected)}]
        report[host] = {"host": version, "checks": checks}
        print(f"{host} {version}: registry-tools-and-bytes {expected}/{expected}", flush=True)
    report["status"] = "passed"
    result_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"E2E evidence: {evidence}")


if __name__ == "__main__":
    main()
