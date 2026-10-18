#!/usr/bin/env python3
"""Prepare or verify the isolated, pinned aider editing benchmark runtime.

Fresh setup (requires git and uv on PATH):
    python benches/native/setup_aider.py
Verification only; never downloads or installs:
    python benches/native/setup_aider.py --check

All checkouts, virtualenv packages, uv cache, and any downloaded Python runtime
live below target/native-bench (or --directory). Existing virtualenvs are only
verified. A failed or incomplete environment is left for inspection; this script
never deletes, resets, or repairs an existing checkout/environment implicitly.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from aider_adapter import PINNED_COMMIT, UPSTREAM

ROOT = Path(__file__).resolve().parents[2]


def run(command, *, environment=None, capture=False, input_text=None, timeout=600):
    result = subprocess.run(
        [str(part) for part in command], env=environment, text=True,
        encoding="utf-8", input=input_text,
        stdout=subprocess.PIPE if capture else sys.stderr,
        stderr=subprocess.PIPE if capture else sys.stderr,
        timeout=timeout,
    )
    if result.returncode:
        detail = (result.stderr or result.stdout or "").strip() if capture else ""
        raise RuntimeError(f"command failed ({result.returncode}): {command[0]} {detail}")
    return result


def validate_source(source):
    if not (source / ".git").exists():
        raise RuntimeError(f"expected the isolated git checkout at {source}")
    revision = run(["git", "-C", source, "rev-parse", "HEAD"], capture=True).stdout.strip()
    if revision != PINNED_COMMIT:
        raise RuntimeError(f"existing aider revision {revision} differs from pin {PINNED_COMMIT}")
    run(["git", "-C", source, "diff", "--exit-code", "HEAD", "--"], capture=True)
    origin = run(["git", "-C", source, "remote", "get-url", "origin"], capture=True).stdout.strip()
    if origin.rstrip("/").removesuffix(".git") != UPSTREAM:
        raise RuntimeError(f"existing checkout origin is not the official upstream: {origin}")


def validate_environment(python, adapter, source, environment):
    # -I ignores inherited PYTHONPATH and user site packages. The editable source
    # is installed in this virtualenv, so it remains importable in isolated mode.
    probe = (
        "import pathlib,runpy,sys,aider; "
        "assert sys.version_info[:2] == (3,13), 'Python 3.13 is required'; "
        "assert pathlib.Path(aider.__file__).resolve().parents[1] == "
        "pathlib.Path(sys.argv[2]).resolve(), 'aider is not the isolated checkout'; "
        "runpy.run_path(sys.argv[1],run_name='__main__')"
    )
    result = run(
        [python, "-I", "-c", probe, adapter, source], capture=True,
        environment=environment, input_text='{"action":"describe"}\n', timeout=90,
    )
    metadata = json.loads(result.stdout)
    if metadata.get("status") != "available" or metadata.get("revision") != PINNED_COMMIT:
        raise RuntimeError(f"native aider validation failed: {metadata}")
    return metadata


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, default=ROOT / "target/native-bench")
    parser.add_argument("--python", default="3.13", help="Python 3.13 interpreter selector for uv")
    parser.add_argument("--uv", default="uv", help="uv executable path")
    parser.add_argument("--check", action="store_true", help="validate only; no downloads/installations")
    args = parser.parse_args()
    directory = args.directory.resolve()
    source, venv = directory / "aider", directory / "aider-venv"
    python = venv / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    adapter = Path(__file__).with_name("aider_adapter.py")
    environment = {**os.environ, "GIT_TERMINAL_PROMPT": "0",
                   "UV_CACHE_DIR": str(directory / "uv-cache"),
                   "UV_PYTHON_INSTALL_DIR": str(directory / "python"),
                   "UV_NO_PROGRESS": "1", "UV_LINK_MODE": "copy",
                   "PYTHONDONTWRITEBYTECODE": "1"}
    for key in ("PYTHONHOME", "PYTHONPATH", "VIRTUAL_ENV"):
        environment.pop(key, None)

    existed = venv.exists()
    if args.check or existed:
        if not python.is_file():
            raise RuntimeError(f"existing/missing environment needs inspection: {venv}")
        validate_source(source)
    else:
        if not shutil.which("git"):
            raise RuntimeError("git must be installed before preparing this optional benchmark")
        if not shutil.which(args.uv):
            raise RuntimeError("uv must be installed before preparing this optional benchmark")
        if source.exists():
            validate_source(source)
        else:
            directory.mkdir(parents=True, exist_ok=True)
            run(["git", "init", "--quiet", source], environment=environment)
            run(["git", "-C", source, "remote", "add", "origin", UPSTREAM + ".git"],
                environment=environment)
            run(["git", "-C", source, "fetch", "--quiet", "--depth", "1", "origin", PINNED_COMMIT],
                environment=environment)
            run(["git", "-C", source, "checkout", "--quiet", "--detach", "FETCH_HEAD"],
                environment=environment)
            validate_source(source)
        run([args.uv, "venv", "--quiet", "--no-config", "--no-project", "--python", args.python, venv],
            environment=environment)
        # Upstream pyproject reads its committed requirements.txt, including the
        # exact dependency pins; --python always targets this isolated virtualenv.
        run([args.uv, "pip", "install", "--quiet", "--no-config", "--python", python, "--editable", source],
            environment=environment)

    metadata = validate_environment(python, adapter, source, environment)
    # Verify the installed upstream dependency pins without requiring uv for an
    # already prepared runtime. The full metadata excludes optional extras.
    dependency_probe = (
        "from importlib.metadata import distribution,version; "
        "from packaging.requirements import Requirement; "
        "reqs=[Requirement(r) for r in distribution('aider-chat').requires or []]; "
        "bad=[str(r) for r in reqs if (not r.marker or r.marker.evaluate({'extra':''})) "
        "and version(r.name) not in r.specifier]; "
        "assert not bad, 'incompatible dependencies: '+repr(bad)"
    )
    run([python, "-I", "-c", dependency_probe], environment=environment, capture=True, timeout=30)
    print(json.dumps({
        "status": "verified" if existed or args.check else "prepared",
        "python": str(python), "adapter": str(adapter), "source": str(source),
        "revision": metadata["revision"], "version": metadata["version"],
        "distribution_version": metadata["distribution_version"],
        "source_hash": metadata["source_hash"],
        "requirements_sha256": hashlib.sha256((source / "requirements.txt").read_bytes()).hexdigest(),
        "network_used_for_verification": False,
    }, ensure_ascii=False, separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"aider setup: {error}", file=sys.stderr)
        raise SystemExit(1)
