#!/usr/bin/env python3
"""Prepare the frozen OpenAI Agents and AutoGen used by native tool E2E.

python benches/native/agent_sdks_setup.py
python benches/native/agent_sdks_setup.py --check
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import venv

ROOT = Path(__file__).resolve().parents[2]
VERSION = "openai-0.24.0-autogen-0.7.5"
DEFAULT = ROOT / "target/native-hosts" / f"agent-sdks-{VERSION}"
LOCK = Path(__file__).with_name("agent_sdks_requirements.txt")


def python_path(directory):
    return directory / ".venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")


def inspect_host(python):
    command = r'''
import base64,hashlib,importlib.metadata as m,json,re,sys
from pathlib import Path
from packaging.requirements import Requirement
packages={re.sub(r'[-_.]+','-',d.metadata['Name']).lower():d.version for d in m.distributions()}
expected={}
for line in Path(sys.argv[1]).read_text().splitlines():
 if re.match(r'^[A-Za-z0-9_.-]+==',line):
  requirement=Requirement(line.removesuffix('\\').strip())
  if requirement.marker is None or requirement.marker.evaluate():
   expected[requirement.name]=next(iter(requirement.specifier)).version
sources={}
for name in ['openai-agents','autogen-agentchat','autogen-ext','autogen-core']:
 d=m.distribution(name)
 for f in d.files:
  if str(f).endswith('.py'):
   digest=hashlib.sha256(d.locate_file(f).read_bytes()).digest()
   if f.hash is not None and f.hash.mode=='sha256':
    assert base64.urlsafe_b64encode(digest).decode().rstrip('=')==f.hash.value, 'Official wheel source changed: '+str(f)
   sources[str(f)]=digest.hex()
print(json.dumps(dict(packages=packages,sources=sources,expected=expected)))
'''
    result = subprocess.run([str(python), "-c", command, str(LOCK)], check=True, capture_output=True, text=True)
    return json.loads(result.stdout)


def prepare(directory=DEFAULT, check=False):
    directory = Path(directory).resolve()
    python = python_path(directory)
    if not check:
        if sys.version_info < (3, 10):
            raise RuntimeError("Agent SDK preparation requires Python 3.10 or later")
        if not python.is_file():
            venv.EnvBuilder(with_pip=True).create(directory / ".venv")
        subprocess.run([str(python), "-m", "pip", "install", "--disable-pip-version-check",
                        "--require-hashes", "-r", str(LOCK)], check=True)
    observed = inspect_host(python)
    expected = observed.pop("expected")
    for name, version in expected.items():
        normalized = re.sub(r"[-_.]+", "-", name).lower()
        if observed["packages"].get(normalized) != version:
            raise AssertionError(f"{name}: expected {version}, found {observed['packages'].get(normalized)}")
    record = {"sdk_version": VERSION, "python": str(python),
              "lock_sha256": hashlib.sha256(LOCK.read_bytes()).hexdigest(), **observed}
    receipt = directory / "native-host.json"
    if check:
        previous = json.loads(receipt.read_text())
        if previous != record:
            raise AssertionError("Agent SDK source/dependency receipt changed; run setup")
    else:
        receipt.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return record


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--destination", type=Path, default=DEFAULT)
    args = parser.parse_args()
    result = prepare(args.destination, args.check)
    print(json.dumps({"status": "available", "version": result["sdk_version"],
                      "python": result["python"], "lock_sha256": result["lock_sha256"]}))
