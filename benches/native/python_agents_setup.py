#!/usr/bin/env python3
"""Prepare official DeepAgents, LangGraph and PydanticAI SDKs with hashed dependencies."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import venv

ROOT = Path(__file__).resolve().parents[2]
DEFAULT = ROOT / "target/native-hosts/python-agents"
LOCK = Path(__file__).with_name("python_agents_requirements.txt")
SDK_VERSIONS = {"deepagents": "0.7.34", "langgraph": "1.2.20", "langchain-core": "1.7.6",
                "pydantic-ai-slim": "2.66.0", "mcp": "2.3.0"}


def python_path(directory):
    return directory / ".venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")


def adapter_sources():
    sources = {}
    for project in ["native-python", "python-agents"]:
        for path in sorted((ROOT / "integrations" / project).rglob("*")):
            if path.is_file() and path.suffix in {".py", ".toml"}:
                sources[str(path.relative_to(ROOT))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return sources


def inspect_host(python):
    script = r'''
import base64,hashlib,importlib.metadata as m,json,re,sys
from pathlib import Path
from packaging.requirements import Requirement
packages={re.sub(r'[-_.]+','-',d.metadata['Name']).lower():d.version for d in m.distributions()}
expected={}
for line in Path(sys.argv[1]).read_text().splitlines():
 if re.match(r'^[A-Za-z0-9_.-]+==',line):
  r=Requirement(line.removesuffix('\\').strip())
  if r.marker is None or r.marker.evaluate():expected[r.name]=next(iter(r.specifier)).version
sources={}
for name in ['deepagents','langgraph','langchain-core','pydantic-ai-slim','mcp','xnip-python-agents','xnip-native-client']:
 d=m.distribution(name)
 for file in d.files:
  if str(file).endswith('.py'):
   digest=hashlib.sha256(d.locate_file(file).read_bytes()).digest()
   if file.hash is not None and file.hash.mode=='sha256':
    assert base64.urlsafe_b64encode(digest).decode().rstrip('=')==file.hash.value, str(file)
   sources[str(file)]=digest.hex()
print(json.dumps(dict(packages=packages,expected=expected,sources=sources)))
'''
    result = subprocess.run([str(python), "-c", script, str(LOCK)], check=True,
                            capture_output=True, text=True)
    return json.loads(result.stdout)


def prepare(directory=DEFAULT, check=False):
    directory = Path(directory).resolve()
    python = python_path(directory)
    if not check:
        if sys.version_info < (3, 11):
            raise RuntimeError("The Python agent SDK integrations require Python 3.11 or later")
        if not python.is_file():
            venv.EnvBuilder(with_pip=True).create(directory / ".venv")
        subprocess.run([str(python), "-m", "pip", "install", "--disable-pip-version-check",
                        "--require-hashes", "-r", str(LOCK)], check=True)
        # Build local packages from disposable copies so setuptools artifacts
        # stay outside the repository's integration sources.
        with tempfile.TemporaryDirectory(prefix="xnip-agent-install-", dir=directory) as temporary:
            stage = Path(temporary)
            projects = []
            for name in ["native-python", "python-agents"]:
                destination = stage / name
                shutil.copytree(ROOT / "integrations" / name, destination,
                                ignore=shutil.ignore_patterns("__pycache__", "*.egg-info", "build"))
                projects.append(str(destination))
            subprocess.run([str(python), "-m", "pip", "install", "--disable-pip-version-check",
                            "--no-deps", "--no-build-isolation", "--force-reinstall", *projects], check=True)
    observed = inspect_host(python)
    for project in ["native-python", "python-agents"]:
        for source in (ROOT / "integrations" / project).rglob("*.py"):
            name = source.relative_to(ROOT / "integrations" / project).as_posix()
            assert observed["sources"].get(name) == hashlib.sha256(source.read_bytes()).hexdigest(), f"Installed adapter differs from source: {name}"
    for name, version in {**observed["expected"], **SDK_VERSIONS}.items():
        normalized = name.lower().replace("_", "-")
        assert observed["packages"].get(normalized) == version, f"{name}: SDK dependency version"
    record = {"python": str(python), "sdks": SDK_VERSIONS, "adapters": adapter_sources(),
              "lock_sha256": hashlib.sha256(LOCK.read_bytes()).hexdigest(), **observed}
    receipt = directory / "native-host.json"
    if check:
        assert json.loads(receipt.read_text()) == record, "Prepared SDK or adapter bytes changed; run setup"
    else:
        receipt.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return record


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--destination", type=Path, default=DEFAULT)
    options = parser.parse_args()
    result = prepare(options.destination, options.check)
    print(json.dumps({"status": "available", "python": result["python"], "sdks": result["sdks"],
                      "lock_sha256": result["lock_sha256"]}))
