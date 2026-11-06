#!/usr/bin/env python3
"""Prepare the universal, hash-locked CrewAI environment used by tests."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import venv

ROOT = Path(__file__).resolve().parents[2]
VERSION = "1.15.36"
DEFAULT = ROOT / "target/native-hosts" / ("crewai-" + VERSION)
LOCK = Path(__file__).with_name("crewai_requirements.txt")


def interpreter(directory):
    import os
    return Path(directory) / ".venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")


def prepare(directory=DEFAULT, check=False):
    directory = Path(directory).resolve()
    python = interpreter(directory)
    if not check:
        if not python.is_file():
            venv.EnvBuilder(with_pip=True).create(directory / ".venv")
        subprocess.run([str(python), "-m", "pip", "install", "--disable-pip-version-check",
                        "--require-hashes", "-r", str(LOCK)], check=True)
    script = r'''
import importlib.metadata as m,json,hashlib
from packaging.requirements import Requirement
from pathlib import Path
lock=Path(__import__('sys').argv[1])
for line in lock.read_text().splitlines():
 if line and not line.startswith((' ','#','--')):
  req=Requirement(line.rstrip(' \\'))
  if req.marker is None or req.marker.evaluate():
   assert m.version(req.name) in req.specifier,(req.name,m.version(req.name))
sources={}
d=m.distribution('crewai')
for f in d.files:
 if str(f).endswith('.py'):
  sources[str(f)]=hashlib.sha256(d.locate_file(f).read_bytes()).hexdigest()
print(json.dumps({'version':m.version('crewai'),'sources':sources}))
'''
    result = subprocess.run([str(python), "-c", script, str(LOCK)], capture_output=True, text=True, check=True)
    record = {"lock_sha256": hashlib.sha256(LOCK.read_bytes()).hexdigest(), **json.loads(result.stdout)}
    assert record["version"] == VERSION
    receipt = directory / "native-host.json"
    if check:
        if json.loads(receipt.read_text()) != record:
            raise AssertionError("CrewAI source/dependency receipt changed; run setup")
    else:
        receipt.write_text(json.dumps(record, indent=2) + "\n")
    return record


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--destination", type=Path, default=DEFAULT)
    args = parser.parse_args()
    result = prepare(args.destination, args.check)
    print(json.dumps({"version": result["version"], "lock_sha256": result["lock_sha256"]}))
