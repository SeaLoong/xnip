#!/usr/bin/env python3
"""Reproducible launcher for the pinned, unchanged Goose edit/MCP client sources.

python benches/native/goose_adapter.py --setup  # isolated sources and cargo build
python benches/native/goose_adapter.py --check  # verify only, without downloads/build
python benches/native/goose_adapter.py          # persistent JSONL describe/execute/bridge

The source download and first build are setup costs. Rust execution never calls
an LLM or initializes a Goose provider/configuration. No global installation or
root Cargo.lock changes are performed. Source bodies are included unchanged.
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
import time
import urllib.request

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
PINS_PATH = HERE / "goose_sources.json"
PINS = json.loads(PINS_PATH.read_text(encoding="utf-8"))
CACHE = ROOT / "target/native-bench" / ("goose-v" + PINS["version"])
CRATE = HERE / "goose_adapter"


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def source_directory():
    return CACHE / "source"


def verify_sources(download=False):
    source = source_directory()
    for entry in PINS["files"]:
        destination = source / entry["path"]
        if not destination.is_file():
            if not download:
                raise RuntimeError(f"missing pinned source: {destination}; run --setup")
            url = (PINS["repository"].replace("github.com", "raw.githubusercontent.com")
                   + "/" + PINS["commit"] + "/" + entry["path"])
            failure = None
            for attempt in range(3):
                try:
                    with urllib.request.urlopen(url, timeout=30) as response:
                        data = response.read()
                    if sha256(data) != entry["sha256"]:
                        raise RuntimeError(f"official source hash mismatch: {entry['path']}")
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    # Never overwrite an existing cache entry, including a concurrent writer.
                    try:
                        with destination.open("xb") as output:
                            output.write(data)
                    except FileExistsError:
                        pass
                    break
                except (OSError, ValueError) as error:
                    failure = error
                    if attempt < 2:
                        time.sleep(0.25 * (attempt + 1))
            else:
                raise RuntimeError(f"cannot acquire official Goose source: {entry['path']}: {failure}")
        if sha256(destination.read_bytes()) != entry["sha256"]:
            raise RuntimeError(f"untrusted Goose source cache: {destination}")
    return source


def build_fingerprint(toolchain):
    files = [PINS_PATH, CRATE / "Cargo.toml", CRATE / "Cargo.lock", CRATE / "build.rs"]
    files.extend(sorted((CRATE / "src").rglob("*.rs")))
    if not (CRATE / "Cargo.lock").is_file():
        raise RuntimeError("the committed independent Goose Cargo.lock is required")
    digest = hashlib.sha256()
    for path in files:
        digest.update(str(path.relative_to(HERE)).encode())
        digest.update(b"\0" + path.read_bytes())
    digest.update((toolchain or "default").encode())
    digest.update(str(source_directory()).encode())
    return digest.hexdigest()


def prepare(check=False, toolchain=None):
    source = verify_sources(download=not check)
    target = CACHE / "cargo-target"
    binary = target / "debug" / ("xnip-native-goose-bench.exe" if os.name == "nt" else "xnip-native-goose-bench")
    stamp_path = CACHE / "build.json"
    fingerprint = build_fingerprint(toolchain)
    try:
        stamp = json.loads(stamp_path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        stamp = {}
    valid = (binary.is_file() and stamp.get("fingerprint") == fingerprint
             and stamp.get("binary_sha256") == sha256(binary.read_bytes()))
    if not valid:
        if check:
            raise RuntimeError("Goose adapter binary is missing/stale; run --setup")
        if not shutil.which("cargo"):
            raise RuntimeError("cargo is required for the optional native Goose benchmark")
        command = ["cargo", *(["+" + toolchain] if toolchain else []), "build", "--locked",
                   "--manifest-path", str(CRATE / "Cargo.toml")]
        environment = {**os.environ, "CARGO_TARGET_DIR": str(target), "GOOSE_NATIVE_SOURCE_DIR": str(source)}
        result = subprocess.run(command, cwd=ROOT, env=environment, stdout=sys.stderr, stderr=sys.stderr)
        if result.returncode:
            raise RuntimeError(f"isolated Goose build failed with exit {result.returncode}")
        compiler = subprocess.check_output(["rustc", *(["+" + toolchain] if toolchain else []), "--version"], text=True).strip()
        stamp = {"fingerprint": fingerprint, "binary_sha256": sha256(binary.read_bytes()), "compiler": compiler}
        stamp_path.write_text(json.dumps(stamp, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    return {"status": "available", "binary": str(binary), "source": str(source),
            "commit": PINS["commit"], "version": PINS["version"],
            "dependency_lock_sha256": sha256((CRATE / "Cargo.lock").read_bytes()), **stamp}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--setup", action="store_true")
    group.add_argument("--check", action="store_true")
    parser.add_argument("--toolchain", help="optional existing Rust toolchain selector, e.g. 1.99.0")
    args = parser.parse_args()
    metadata = prepare(args.check, args.toolchain)
    if args.setup or args.check:
        print(json.dumps(metadata, ensure_ascii=False, separators=(",", ":")))
        return
    os.environ["GOOSE_NATIVE_BINARY_SHA256"] = metadata["binary_sha256"]
    os.environ["GOOSE_NATIVE_LOCK_SHA256"] = metadata["dependency_lock_sha256"]
    os.environ["GOOSE_NATIVE_COMPILER"] = metadata["compiler"]
    os.execv(metadata["binary"], [metadata["binary"]])


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Goose native adapter unavailable: {error}", file=sys.stderr)
        raise SystemExit(1)
