#!/usr/bin/env python3
"""Prepare verified npm artifacts; private by default, explicit --public for one six-target release. Never publish."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tarfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PARENT_NAME = "xnip-cli"
PLATFORMS = {
    "linux-x64": ("x86_64-unknown-linux-musl", "xnip-linux-x64-musl"),
    "linux-arm64": ("aarch64-unknown-linux-musl", "xnip-linux-arm64-musl"),
    "darwin-x64": ("x86_64-apple-darwin", "xnip-darwin-x64"),
    "darwin-arm64": ("aarch64-apple-darwin", "xnip-darwin-arm64"),
    "win32-x64": ("x86_64-pc-windows-msvc", "xnip-win32-x64"),
    "win32-arm64": ("aarch64-pc-windows-msvc", "xnip-win32-arm64"),
}
MAX_EXECUTABLE = 128 * 1024 * 1024
REPOSITORY = {"type":"git", "url":"https://github.com/SeaLoong/xnip.git"}
PUBLICATION_PRIVATE = "private-local"
PUBLICATION_PUBLIC = "public-release"
PARENT_FILES = ["bin/", "lib/", "integrations/", "native-manifest.json",
                "README.md", "LICENSE", "THIRD_PARTY_LICENSES.md"]

def digest(data):
    return hashlib.sha256(data).hexdigest()

def binary_build_identity(data, version, target):
    # A format marker is build metadata, not a signature. Checksums must still
    # come from trusted build artifacts. It prevents accidentally relabeling an
    # older or wrong-target executable when preparing foreign-platform packages.
    markers = set(re.findall(rb"\x00XNIP_BUILD_ID_V1:([0-9A-Za-z.+-]+)\|([0-9A-Za-z_-]+)\x00", data))
    if markers != {(version.encode(), target.encode())}:
        raise ValueError("binary build identity does not match package version/target")

def executable_identity(data, platform, allow_dynamic=False):
    if not 0 < len(data) <= MAX_EXECUTABLE:
        raise ValueError("invalid executable size")
    arch = platform.split("-")[1]
    if platform.startswith("win32"):
        if data[:2] != b"MZ" or len(data) < 64:
            raise ValueError("not a PE executable")
        offset = struct.unpack_from("<I", data, 60)[0]
        if offset + 6 > len(data) or data[offset:offset+4] != b"PE\0\0":
            raise ValueError("invalid PE header")
        if struct.unpack_from("<H", data, offset+4)[0] != (0x8664 if arch == "x64" else 0xaa64):
            raise ValueError("PE architecture mismatch")
    elif platform.startswith("darwin"):
        if data[:4] != b"\xcf\xfa\xed\xfe" or len(data) < 32:
            raise ValueError("not a little-endian Mach-O 64 executable")
        if struct.unpack_from("<I", data, 4)[0] != (0x1000007 if arch == "x64" else 0x100000c):
            raise ValueError("Mach-O architecture mismatch")
    else:
        if data[:6] != b"\x7fELF\x02\x01" or len(data) < 64:
            raise ValueError("not a little-endian ELF 64 executable")
        if struct.unpack_from("<H", data, 18)[0] != (62 if arch == "x64" else 183):
            raise ValueError("ELF architecture mismatch")
        offset = struct.unpack_from("<Q", data, 32)[0]
        width, count = struct.unpack_from("<HH", data, 54)
        if width < 56 or count > 1024 or offset + width*count > len(data):
            raise ValueError("invalid ELF program headers")
        dynamic = False
        for index in range(count):
            header = offset + width*index
            kind = struct.unpack_from("<I", data, header)[0]
            if kind == 3:
                dynamic = True
                if not allow_dynamic:
                    raise ValueError("Linux payload requires an interpreter; libc-independent package is unsafe")
            if kind == 2:
                start = struct.unpack_from("<Q", data, header+8)[0]
                size = struct.unpack_from("<Q", data, header+32)[0]
                if start+size > len(data) or size % 16:
                    raise ValueError("invalid ELF dynamic table")
                for at in range(start, start+size, 16):
                    if struct.unpack_from("<q", data, at)[0] == 1:
                        dynamic = True
                        if not allow_dynamic:
                            raise ValueError("Linux payload has a shared dependency")
        if dynamic:
            return "dynamic ELF (host-local only)"
    return "PE" if platform.startswith("win32") else "Mach-O" if platform.startswith("darwin") else "static ELF"

def read_archive(path, executable):
    allowed = {executable, "README.md", "LICENSE", "THIRD_PARTY_LICENSES.md"}
    if path.suffix == ".zip":
        with zipfile.ZipFile(path) as archive:
            members = archive.infolist()
            if len(members) != 4 or {m.filename for m in members} != allowed:
                raise ValueError("unexpected ZIP inventory: " + path.name)
            for item in members:
                kind = (item.external_attr >> 16) & 0o170000
                if item.is_dir() or kind not in (0, 0o100000) or item.file_size > MAX_EXECUTABLE:
                    raise ValueError("unsafe ZIP member")
            return {name: archive.read(name) for name in allowed}
    with tarfile.open(path, "r:gz") as archive:
        members = archive.getmembers()
        if len(members) != 4 or {m.name for m in members} != allowed:
            raise ValueError("unexpected tar inventory: " + path.name)
        if any(not m.isfile() or m.size > MAX_EXECUTABLE for m in members):
            raise ValueError("unsafe tar member")
        return {name: archive.extractfile(name).read() for name in allowed}

def write_json(path, data):
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False)+"\n", encoding="utf-8")

def npm_command():
    node = shutil.which("node")
    npm = shutil.which("npm")
    if not node or not npm:
        raise ValueError("Node.js and npm are required for local packing")
    npm_path = Path(npm)
    candidates = [npm_path.parent/"node_modules/npm/bin/npm-cli.js", npm_path.resolve()]
    cli = next((p for p in candidates if p.is_file() and p.name == "npm-cli.js"), None)
    if cli is None:
        raise ValueError("cannot locate npm-cli.js; use a standard Node/npm installation")
    return [node, str(cli)]

def release_provenance(dist, archive_name, raw, version, target):
    sidecar = dist/(archive_name+".build.json")
    data = json.loads(sidecar.read_text(encoding="utf-8"))
    if (not isinstance(data, dict) or type(data.get("schema_version")) is not int or data["schema_version"] != 1
            or data.get("version") != version or data.get("target") != target
            or not isinstance(data.get("source_commit"), str)
            or not re.fullmatch(r"[a-f0-9]{40}", data["source_commit"])
            or data.get("archive_sha256") != digest(raw)):
        raise ValueError("invalid release provenance: " + sidecar.name)
    return data["source_commit"]

def prepare(dist, output, pack=False, host_binary=None, public=False, source_commit=None):
    version = tomllib.loads((ROOT/"Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    template = json.loads((ROOT/"npm/xnip/package.json").read_text(encoding="utf-8"))
    if template.get("private") is not True or template.get("name") != PARENT_NAME:
        raise ValueError(f"source template requires name:{PARENT_NAME} and private:true")
    if template.get("scripts") != {"prepack":"node lib/validate.cjs"} or template.get("files") != PARENT_FILES:
        raise ValueError("source template must retain the file allowlist and validation-only prepack script")
    if public and (dist is None or host_binary is not None):
        raise ValueError("public preparation requires all six release archives via --dist; host-local builds are forbidden")
    if source_commit is not None and (not public or not re.fullmatch(r"[a-f0-9]{40}", source_commit)):
        raise ValueError("--source-commit requires --public and a lowercase 40-hex commit")
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.+-]+)?", version):
        raise ValueError("Cargo package version must be a valid npm-compatible version")
    for relative in ["integrations/dsh/index.mjs", "integrations/dsh/cordis.patch.yml",
                     "integrations/pi/index.mjs", "npm/xnip/lib/validate.cjs",
                     "npm/xnip/lib/native.cjs", "npm/xnip/bin/xnip.cjs", "npm/README.md",
                     "LICENSE", "THIRD_PARTY_LICENSES.md"]:
        if not (ROOT/relative).is_file():
            raise ValueError("required package source missing: " + relative)
    command = npm_command() if pack else None
    output = output.resolve()
    if output.exists():
        raise ValueError("output must be a new directory; existing artifacts are never overwritten")
    binaries = {}
    release_commit = source_commit
    publication = PUBLICATION_PUBLIC if public else PUBLICATION_PRIVATE
    if host_binary is not None:
        host_os = "win32" if sys.platform == "win32" else "darwin" if sys.platform == "darwin" else "linux"
        identity = json.loads(subprocess.check_output([shutil.which("node") or "node", "-p",
            "JSON.stringify({platform:process.platform,arch:process.arch})"], text=True))
        host_key = host_os + "-" + identity["arch"]
        if host_key not in PLATFORMS:
            raise ValueError("unsupported local host")
        target, name = PLATFORMS[host_key]
        env = {**os.environ, "XNIP_AUTO_UPDATE":"0"}
        actual = subprocess.check_output([str(host_binary.resolve()), "--version"], env=env, text=True).strip()
        doctor = subprocess.check_output([str(host_binary.resolve()), "doctor"], env=env, text=True)
        reported = re.search(r"^  target:\s+(\S+)\s*$", doctor, re.MULTILINE)
        actual_target = reported.group(1) if reported else None
        allowed_targets = {target}
        if host_os == "linux":
            allowed_targets.add(target.replace("-musl", "-gnu"))
        if actual != "xnip " + version or actual_target not in allowed_targets:
            raise ValueError("local binary version/target does not match this package")
        target = actual_target
        if host_os == "linux" and target.endswith("-gnu"):
            name = "xnip-" + host_key + "-gnu-local"
        executable = "xnip.exe" if host_os == "win32" else "xnip"
        data = host_binary.read_bytes()
        kind = executable_identity(data, host_key, allow_dynamic=target.endswith("-gnu"))
        binary_build_identity(data, version, target)
        contents = {executable:data, **{n:(ROOT/n).read_bytes() for n in ["README.md","LICENSE","THIRD_PARTY_LICENSES.md"]}}
        binaries[host_key] = (contents, {"package":name,"target":target,"binary":executable,
            "sha256":digest(data),"size":len(data),"format":kind,"source":"local-host-build"})
    for platform, (target, name) in ([] if host_binary is not None else PLATFORMS.items()):
        executable = "xnip.exe" if platform.startswith("win32") else "xnip"
        archive_name = f"xnip-v{version}-{target}" + (".zip" if platform.startswith("win32") else ".tar.gz")
        archive = dist/archive_name
        if not archive.is_file() or archive.stat().st_size > 64*1024*1024:
            raise ValueError("archive missing or too large: " + archive_name)
        raw = archive.read_bytes()
        checksum = (dist/(archive_name+".sha256")).read_text(encoding="utf-8").strip().split()
        if len(checksum) != 2 or checksum[1].lstrip("*") != archive_name or digest(raw) != checksum[0].lower():
            raise ValueError("checksum mismatch: " + archive_name)
        commit = release_provenance(dist, archive_name, raw, version, target) if public else None
        if public:
            if release_commit is not None and commit != release_commit:
                raise ValueError("release archives must come from the same source commit")
            release_commit = commit
        contents = read_archive(archive, executable)
        identity = executable_identity(contents[executable], platform)
        binary_build_identity(contents[executable], version, target)
        binaries[platform] = (contents, {"package":name, "target":target, "binary":executable,
            "sha256":digest(contents[executable]), "size":len(contents[executable]), "format":identity,
            "archive_sha256":digest(raw), **({"source_commit":commit} if public else {})})
    if public and set(binaries) != set(PLATFORMS):
        raise ValueError("public release requires exactly six distribution platforms")
    release = ({"schema_version":1, "version":version, "source_commit":release_commit,
        "archives":{key:{"package":entry["package"],"target":entry["target"],
            "archive_sha256":entry["archive_sha256"],"sha256":entry["sha256"]}
            for key,(_,entry) in binaries.items()}} if public else None)
    output.mkdir(parents=True)
    parent = output/"packages/xnip"
    shutil.copytree(ROOT/"npm/xnip", parent)
    manifest = {"schema_version":2, "version":version, "publication":publication,
        "localOnly":host_binary is not None, "repository":REPOSITORY, "release":release, "platforms":{}}
    packages = []
    for platform, (contents, entry) in binaries.items():
        directory = output/"packages"/entry["package"]
        directory.mkdir()
        platform_os, arch = platform.split("-")
        metadata = {"name":entry["package"], "version":version, "private":not public, "license":"MIT",
            "repository":REPOSITORY, "scripts":{"prepack":"node lib/validate.cjs"},
            "description":f"xnip native executable for {entry['target']}", "os":[platform_os],
            "cpu":[arch], "exports":{"./package.json":"./package.json"},
            "files":[entry["binary"],"binary.json","lib/validate.cjs","LICENSE","THIRD_PARTY_LICENSES.md"]}
        if public:
            metadata["publishConfig"] = {"access":"public"}
        # Full Linux distributions are verified static ELF. A GNU host-local
        # tarball is explicitly localOnly, bundled, and uses a distinct name.
        write_json(directory/"package.json", metadata)
        write_json(directory/"binary.json", {"schema_version":2, "version":version,
            "publication":publication,"localOnly":host_binary is not None,"release":release, **entry})
        (directory/"lib").mkdir()
        shutil.copyfile(ROOT/"npm/xnip/lib/validate.cjs", directory/"lib/validate.cjs")
        for name in [entry["binary"],"LICENSE","THIRD_PARTY_LICENSES.md"]:
            (directory/name).write_bytes(contents[name])
        (directory/entry["binary"]).chmod(0o755)
        manifest["platforms"][platform] = entry
        packages.append(directory)
    for integration in ["dsh","pi"]:
        source = ROOT/"integrations"/integration
        if not source.is_dir():
            raise ValueError("integration source not ready: " + integration)
        shutil.copytree(source, parent/"integrations"/integration)
    for name in ["LICENSE","THIRD_PARTY_LICENSES.md"]:
        shutil.copyfile(ROOT/name, parent/name)
    shutil.copyfile(ROOT/"npm/README.md", parent/"README.md")
    template["version"] = version
    template["private"] = not public
    template["repository"] = REPOSITORY
    if public:
        template["publishConfig"] = {"access":"public"}
    template["optionalDependencies"] = {entry["package"]:version for _,entry in binaries.values()}
    write_json(parent/"package.json", template)
    write_json(parent/"native-manifest.json", manifest)
    if host_binary is not None:
        # A private local tarball is self-contained; no unpublished platform package
        # or registry authentication is needed for a local harness installation.
        host_package = next(iter(manifest["platforms"].values()))["package"]
        (parent/"node_modules").mkdir()
        shutil.copytree(output/"packages"/host_package, parent/"node_modules"/host_package)
        template["bundledDependencies"] = [host_package]
        write_json(parent/"package.json", template)
    packages.append(parent)
    tarballs = []
    if pack:
        packed = output/"tarballs"
        packed.mkdir()
        for directory in packages:
            result = subprocess.run([*command,"pack","--json","--pack-destination",str(packed)],
                cwd=directory, check=True, capture_output=True, text=True, encoding="utf-8",
                env={**os.environ,"XNIP_AUTO_UPDATE":"0"}, timeout=60)
            record = json.loads(result.stdout)[0]
            tarballs.append({"name":record["name"],"version":version,
                "filename":"tarballs/"+record["filename"],
                "integrity":record["integrity"],"size":record["size"],
                "sha256":digest((packed/record["filename"]).read_bytes())})
    report = {"schema_version":2,"version":version,
        "publication":PUBLICATION_PUBLIC if public else "disabled-private-packages",
        "source_commit":release_commit if public else None,"repository":REPOSITORY,
        "input_directory":str(dist.resolve()) if dist else None, "local_only":host_binary is not None,"output_directory":str(output),
        "manifest":manifest,"tarballs":tarballs}
    write_json(output/"preparation.json", report)
    return report

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    inputs = parser.add_mutually_exclusive_group(required=True)
    inputs.add_argument("--dist", type=Path, help="all six release archives and checksums")
    inputs.add_argument("--host-binary", type=Path, help="private host-only local validation package")
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--pack", action="store_true", help="create tarballs locally; never publishes")
    parser.add_argument("--public", action="store_true", help="prepare a public release from all six same-commit --dist archives and build provenance sidecars")
    parser.add_argument("--source-commit", help="require release provenance to match this 40-hex source commit (only with --public)")
    args = parser.parse_args()
    try:
        result = prepare(args.dist.resolve() if args.dist else None, args.out, args.pack, args.host_binary,
            public=args.public, source_commit=args.source_commit)
    except (ValueError,OSError,subprocess.SubprocessError) as error:
        parser.exit(1, f"npm preparation failed: {error}\n")
    print(json.dumps({"version":result["version"],"output":result["output_directory"],
        "platforms":len(result["manifest"]["platforms"]),"local_tarballs":len(result["tarballs"]),
        "publication":result["publication"]}))
if __name__ == "__main__":
    main()
