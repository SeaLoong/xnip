#!/usr/bin/env python3
"""Prepare frozen Google ADK MCP tools with pip and wheel source receipts."""
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
VERSION = "2.13.0"
DEFAULT = ROOT / "target/native-hosts/google-adk-2.13.0"
LOCK = Path(__file__).with_name("google_adk_requirements.txt")


def python_path(directory):
    return Path(directory) / ".venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")


def inspect_host(python):
    script = r'''
import base64,hashlib,importlib.metadata as m,json,re,sys
from pathlib import Path
from packaging.requirements import Requirement
packages={re.sub(r'[-_.]+','-',d.metadata['Name']).lower():d.version for d in m.distributions()}
expected={}
for line in Path(sys.argv[1]).read_text().splitlines():
 if re.match(r'^[A-Za-z0-9_.-]+==',line):
  req=Requirement(line.removesuffix('\\').strip())
  if req.marker is None or req.marker.evaluate():expected[req.name]=next(iter(req.specifier)).version
sources={}
for name in ['google-adk','mcp']:
 d=m.distribution(name)
 for f in d.files:
  if str(f).endswith('.py'):
   digest=hashlib.sha256(d.locate_file(f).read_bytes()).digest()
   if f.hash is not None and f.hash.mode=='sha256':
    assert base64.urlsafe_b64encode(digest).decode().rstrip('=')==f.hash.value, 'Official wheel source changed: '+str(f)
   sources[str(f)]=digest.hex()
print(json.dumps(dict(packages=packages,sources=sources,expected=expected)))
'''
    result = subprocess.run([str(python), "-c", script, str(LOCK)], check=True, capture_output=True, text=True)
    return json.loads(result.stdout)


def prepare(directory=DEFAULT, check=False):
    directory = Path(directory).resolve()
    python = python_path(directory)
    if not check:
        if sys.version_info < (3, 11):
            raise RuntimeError("Google ADK requires Python 3.11 or later")
        if not python.is_file():
            venv.EnvBuilder(with_pip=True).create(directory / ".venv")
        subprocess.run([str(python), "-m", "pip", "install", "--disable-pip-version-check", "--require-hashes", "-r", str(LOCK)], check=True)
    observed = inspect_host(python)
    expected = observed.pop("expected")
    for name, version in expected.items():
        if observed["packages"].get(re.sub(r"[-_.]+", "-", name).lower()) != version:
            raise AssertionError(f"Frozen package version differs: {name}")
    record = {"sdk_version": VERSION, "python": str(python),
              "lock_sha256": hashlib.sha256(LOCK.read_bytes()).hexdigest(), **observed}
    receipt = directory / "native-host.json"
    if check:
        if json.loads(receipt.read_text()) != record:
            raise AssertionError("Google ADK source/dependency receipt changed; run setup")
    else:
        receipt.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return record


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, default=DEFAULT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    result = prepare(args.destination, args.check)
    print(json.dumps({"version": result["sdk_version"], "python": result["python"], "lock_sha256": result["lock_sha256"]}))
