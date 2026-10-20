#!/usr/bin/env python3
"""Mandatory native-host acceptance of a six-target public npm preparation.

Uses local tarballs only with npm --offline --ignore-scripts. Never publishes or
authenticates. Unlike optional development tests, missing prerequisites fail.
"""
from __future__ import annotations
import argparse
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("npm_prepare", ROOT/"npm/prepare.py")
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)
from npm_contract import Rpc


def require(condition, message):
    if not condition:
        raise ValueError(message)


def smoke(prepared, source_commit, expected_platform=None):
    prepared = Path(prepared).resolve()
    require(re.fullmatch(r"[a-f0-9]{40}", source_commit), "expected a lowercase 40-hex source commit")
    report = json.loads((prepared/"preparation.json").read_text(encoding="utf8"))
    require(report.get("schema_version") == 2 and report.get("publication") == "public-release"
        and report.get("local_only") is False and report.get("source_commit") == source_commit,
        "prepared artifact is not the expected public release")
    version = report["version"]
    manifest = report["manifest"]
    require(manifest.get("release", {}).get("source_commit") == source_commit
        and set(manifest.get("platforms", {})) == set(prepare.PLATFORMS),
        "incomplete or wrong-commit release manifest")
    node = shutil.which("node")
    require(node is not None, "Node.js is required; release smoke never skips")
    host = json.loads(subprocess.check_output([node,"-p",
        "JSON.stringify({platform:process.platform,arch:process.arch})"], text=True))
    key = host["platform"]+"-"+host["arch"]
    require(key in prepare.PLATFORMS, "unsupported native host " + key)
    expected_platform = expected_platform or os.environ.get("XNIP_EXPECT_PLATFORM")
    require(expected_platform is None or expected_platform == key,
        "expected native platform "+str(expected_platform)+", actual Node platform "+key)
    payload = manifest["platforms"][key]
    target, name = prepare.PLATFORMS[key]
    require(payload["target"] == target and payload["package"] == name,
        "host platform does not match release target")
    records = report.get("tarballs", [])
    require(len(records) == 7 and {entry["name"] for entry in records}
        == {"xnip", *(value[1] for value in prepare.PLATFORMS.values())},
        "release must contain exactly seven distinct npm tarballs")
    selected = {}
    for entry in records:
        require(entry.get("version") == version, "tarball version mismatch")
        relative = Path(entry["filename"])
        require(not relative.is_absolute() and relative.parts[0] == "tarballs"
            and ".." not in relative.parts, "tarball path must stay inside prepared tarballs")
        file = prepared/relative
        require(not file.is_symlink() and file.is_file(), "tarball must be an ordinary file")
        data = file.read_bytes()
        require(len(data) == entry["size"] and hashlib.sha256(data).hexdigest() == entry["sha256"],
            "tarball size/SHA256 mismatch: "+entry["name"])
        integrity = entry.get("integrity", "")
        require(integrity.startswith("sha512-") and
            base64.b64decode(integrity[7:], validate=True) == hashlib.sha512(data).digest(),
            "tarball integrity mismatch: "+entry["name"])
        if entry["name"] in {"xnip", name}:
            selected[entry["name"]] = file
    with tempfile.TemporaryDirectory(prefix="xnip-public-release-smoke-") as temporary:
        root = Path(temporary)
        consumer = root/"consumer"
        result = subprocess.run([*prepare.npm_command(),"install","--prefix",str(consumer),
            "--cache",str(root/"npm-cache"),"--offline","--ignore-scripts","--no-audit","--no-fund",
            str(selected["xnip"]),str(selected[name])],capture_output=True,text=True,timeout=120)
        require(result.returncode == 0, "offline npm install failed: "+result.stderr[-4000:])
        package = consumer/"node_modules/xnip"
        native = consumer/"node_modules"/name
        for directory in [package,native]:
            pkg = json.loads((directory/"package.json").read_text(encoding="utf8"))
            require(pkg.get("private") is False and pkg.get("repository") == prepare.REPOSITORY
                and pkg.get("version") == version, "installed public package identity mismatch")
            check = subprocess.run([node,str(directory/"lib/validate.cjs")],
                capture_output=True,text=True,timeout=20)
            require(check.returncode == 0, "installed prepack validation failed: "+check.stderr[-4000:])
        require(json.loads((native/"binary.json").read_text(encoding="utf8"))["release"]["source_commit"]
            == source_commit, "installed native provenance mismatch")
        work = root/"workspace"; work.mkdir()
        env = {**os.environ,"XNIP_AUTO_UPDATE":"0","XNIP_NPM_CACHE_DIR":str(root/"exec-cache"),
            "XNIP_UPDATE_STATE_DIR":str(root/"update-state")}
        launcher = package/"bin/xnip.cjs"
        def run(*args):
            return subprocess.run([node,str(launcher),*args],cwd=work,env=env,
                capture_output=True,text=True,timeout=20)
        actual = run("--version")
        require(actual.returncode == 0 and actual.stdout.strip() == "xnip "+version and not actual.stderr,
            "installed CLI version failed: "+actual.stderr)
        doctor = run("doctor")
        require(doctor.returncode == 0 and not doctor.stderr
            and re.search(r"^  target:\s+"+re.escape(target)+r"\s*$", doctor.stdout, re.MULTILINE),
            "installed doctor target failed: "+doctor.stdout+doctor.stderr)
        rpc = Rpc(launcher,work,env)
        try:
            tools = rpc.request("tools/list", {})["result"]["tools"]
            require({tool["name"] for tool in tools} ==
                {"xnip_edit","xnip_apply","xnip_peek","xnip_find","xnip_recover"},
                "MCP compact discovery failed")
            written = rpc.request("tools/call", {"name":"xnip_edit","arguments":{
                "file":"a.txt","operation":"create","text":"before\r\nEOF"}})
            require("error" not in written and not written["result"].get("isError"),
                "MCP create failed")
            edited = rpc.request("tools/call", {"name":"xnip_edit","arguments":{
                "file":"a.txt","old":"before","text":"after"}})
            require("error" not in edited and not edited["result"].get("isError")
                and (work/"a.txt").read_bytes() == b"after\r\nEOF", "MCP byte-preserving edit failed")
        finally:
            diagnostics = rpc.close()
        require(rpc.child.returncode == 0 and not diagnostics, "MCP EOF shutdown/stderr failed")
        require(not (root/"update-state").exists(), "npm child must not enable native auto-update")
    return {"ok":True,"platform":key,"version":version,"source_commit":source_commit,
        "binary_sha256":payload["sha256"],"installation":"offline-local-tarballs-ignore-scripts",
        "checks":["seven-tarball-integrity","installed-public-provenance","version","doctor","real-stdio-MCP","EOF"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepared",required=True,type=Path)
    parser.add_argument("--source-commit",required=True)
    parser.add_argument("--expected-platform",default=os.environ.get("XNIP_EXPECT_PLATFORM"),
        help="require this real Node platform/arch; defaults to XNIP_EXPECT_PLATFORM")
    args = parser.parse_args()
    try:
        result = smoke(args.prepared,args.source_commit,args.expected_platform)
    except (OSError,ValueError,subprocess.SubprocessError,KeyError) as error:
        parser.exit(1,"npm release smoke failed: "+str(error)+"\n")
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
