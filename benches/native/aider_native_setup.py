#!/usr/bin/env python3
"""Prepare the pinned Aider scripting runtime and verify real Coder dispatch."""
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
HERE = Path(__file__).resolve().parent
COMMIT = "5dc9490bb35f9729ef2c95d00a19ccd30c26339c"
REPOSITORY = "https://github.com/Aider-AI/aider"
DEFAULT = ROOT / "target/native-hosts/aider-5dc9490"
LOCK = HERE / "aider_requirements.txt"
UPSTREAM_REQUIREMENTS = HERE / "aider_native/upstream-requirements.txt"


def python_path(directory=DEFAULT):
    return Path(directory) / "aider-venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")


def run(command, **kwargs):
    return subprocess.run([str(part) for part in command], check=True, capture_output=True,
                          text=True, encoding="utf-8", **kwargs)


def prepare(directory=DEFAULT, check=False):
    directory = Path(directory).resolve()
    source = directory / "aider"
    python = python_path(directory)
    if not check:
        if sys.version_info[:2] != (3, 13):
            raise RuntimeError("Run Aider setup with Python 3.13")
        directory.mkdir(parents=True, exist_ok=True)
        if not source.exists():
            run(["git", "init", "--quiet", source])
            run(["git", "-C", source, "remote", "add", "origin", REPOSITORY + ".git"])
            run(["git", "-C", source, "fetch", "--quiet", "--depth", "1", "origin", COMMIT])
            run(["git", "-C", source, "checkout", "--quiet", "--detach", "FETCH_HEAD"])
    revision = run(["git", "-C", source, "rev-parse", "HEAD"]).stdout.strip()
    assert revision == COMMIT, "Aider checkout revision differs from the pinned SDK"
    run(["git", "-C", source, "diff", "--exit-code", "HEAD", "--"])
    assert (source / "requirements.txt").read_bytes() == UPSTREAM_REQUIREMENTS.read_bytes()
    if not check:
        if not python.is_file():
            venv.EnvBuilder(with_pip=True).create(directory / "aider-venv")
        else:
            run([python, "-m", "ensurepip", "--upgrade"])
        run([python, "-m", "pip", "install", "--disable-pip-version-check", "--require-hashes",
             "-r", LOCK])
        run([python, "-m", "pip", "install", "--disable-pip-version-check", "--no-deps",
             "--no-build-isolation", "--editable", source])
        with tempfile.TemporaryDirectory(dir=directory, prefix="local-sdk-packages-") as stage:
            projects = []
            for name in ["native-python", "aider-native"]:
                target = Path(stage) / name
                target.mkdir()
                for file in (ROOT / "integrations" / name).iterdir():
                    if file.is_file():
                        shutil.copy2(file, target / file.name)
                projects.append(target)
            run([python, "-m", "pip", "install", "--disable-pip-version-check", "--no-deps",
                 "--no-build-isolation", "--upgrade", *projects])
    probe = r'''
from pathlib import Path
import hashlib,importlib.metadata as m,json,sys,tempfile
root,source=map(Path,sys.argv[1:])
sys.path.insert(0,str(root/'tests'))
import aider
assert Path(aider.__file__).resolve().parents[1]==source
from aider_native_runtime import AiderFixture,diff_response
from aider.coders import __all__ as coders
from packaging.requirements import Requirement
lock=root/'benches/native/aider_requirements.txt'
expected={}
for line in lock.read_text().splitlines():
 if not line or line[0].isspace() or line.startswith('#'):continue
 r=Requirement(line.rstrip('\\').strip())
 if not r.marker or r.marker.evaluate():expected[r.name]=m.version(r.name)
 for spec in r.specifier:
  if (not r.marker or r.marker.evaluate()) and spec.operator=='==':assert m.version(r.name)==spec.version
with tempfile.TemporaryDirectory(dir=root/'target',prefix='aider-setup-check-') as folder:
 cwd=Path(folder);(cwd/'a.txt').write_text('old\n')
 runtime=AiderFixture(cwd,native=False,files=['a.txt'])
 try:
  result=runtime.apply(diff_response('a.txt','old\n','new\n'))
  assert not result['errors'] and (cwd/'a.txt').read_text()=='new\n'
  assert runtime.model_call_count==0
 finally:runtime.close()
sources={str(p.relative_to(source)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((source/'aider').rglob('*.py'))}
local_packages={}
for package,module,directory in [('xnip-native-client','xnip_native_client.py','native-python'),('xnip-aider','xnip_aider.py','aider-native')]:
 d=m.distribution(package);content=d.locate_file(module).read_bytes()
 assert content==(root/'integrations'/directory/module).read_bytes(),'Run setup to refresh installed local SDK packages'
 local_packages[package]={'version':d.version,'module':module,'sha256':hashlib.sha256(content).hexdigest()}
print(json.dumps(dict(version=aider.__version__,distribution_version=m.version('aider-chat'),
    packages=expected,sources=sources,local_packages=local_packages,
    formats=sorted({c.edit_format for c in coders if c.edit_format}),model_calls=0)))
'''
    result = run([python, "-c", probe, ROOT, source])
    # InputOutput may print terminal capability diagnostics during construction.
    observed = json.loads(result.stdout.splitlines()[-1])
    record = {"commit": COMMIT, "repository": REPOSITORY,
              "lock_sha256": hashlib.sha256(LOCK.read_bytes()).hexdigest(), **observed}
    receipt = directory / "native-host.json"
    if check:
        assert json.loads(receipt.read_text()) == record, "Aider host bytes/dependencies changed; run setup"
    else:
        receipt.write_text(json.dumps(record, sort_keys=True, indent=2) + "\n")
    return record


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, default=DEFAULT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    record = prepare(args.destination, args.check)
    print(json.dumps({"status": "available", "version": record["version"], "commit": COMMIT,
                      "python": str(python_path(args.destination)), "model_calls": 0,
                      "lock_sha256": record["lock_sha256"]}))
