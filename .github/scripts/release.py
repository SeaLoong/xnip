#!/usr/bin/env python3
"""Plan and stamp release artifacts without third-party Python dependencies."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib


SEMVER = re.compile(
    r"(?P<major>0|[1-9][0-9]*)\.(?P<minor>0|[1-9][0-9]*)\."
    r"(?P<patch>0|[1-9][0-9]*)"
    r"(?:-(?P<pre>[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+(?P<build>[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
)
SHA = re.compile(r"(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})")
EXACT_RELEASE_PATHS = {
    "Cargo.toml", "Cargo.lock", "build.rs", "Cross.toml",
    "rust-toolchain", "rust-toolchain.toml", "install.sh", "install.ps1",
    "LICENSE", "THIRD_PARTY_LICENSES.md", "xtask/Cargo.toml",
    "docs/SKILL.md", "docs/apply-format.md", "docs/error-codes.md",
    "docs/examples.md", "docs/patch.md", "docs/recovery.md", "docs/edit.md",
    ".github/scripts/release.py", ".github/scripts/publish_release.py",
    ".github/workflows/build-release.yml",
    ".github/workflows/release.yml",
}
RELEASE_PREFIXES = (
    ".cargo/", "integrations/", ".github/skills/xnip-editing/", "xtask/src/",
)
VERSION_LINE = re.compile(
    r"""(?m)^[ \t]*version[ \t]*=[ \t]*(?P<quote>["'])(?P<value>[^"'\r\n]*)(?P=quote)[ \t]*(?:#[^\r\n]*)?\r?$"""
)


class ReleaseError(Exception):
    """An invalid release plan must fail the workflow, never publish guesses."""


def semver(value: str) -> re.Match[str]:
    match = SEMVER.fullmatch(value)
    if match is None:
        raise ReleaseError(f"invalid semantic version: {value!r}")
    if match["pre"] and any(
        part.isdigit() and len(part) > 1 and part.startswith("0")
        for part in match["pre"].split(".")
    ):
        raise ReleaseError(f"numeric prerelease identifier has a leading zero: {value!r}")
    return match


def git(root: Path, *args: str) -> bytes:
    result = subprocess.run(
        ["git", "-C", str(root), *args], capture_output=True, check=False,
    )
    if result.returncode:
        message = result.stderr.decode("utf-8", errors="replace").strip()
        raise ReleaseError(f"git {' '.join(args[:2])} failed: {message}")
    return result.stdout


def commit(root: Path, value: str) -> str:
    if SHA.fullmatch(value) is None:
        raise ReleaseError("commit must be a complete 40- or 64-character hexadecimal SHA")
    resolved = git(root, "rev-parse", "--verify", f"{value}^{{commit}}")
    return resolved.decode("ascii").strip()


def package_version(raw: bytes) -> str:
    try:
        document = tomllib.loads(raw.decode("utf-8"))
    except (UnicodeError, tomllib.TOMLDecodeError) as error:
        raise ReleaseError(f"invalid Cargo manifest: {error}") from error
    package = document.get("package", {})
    if not isinstance(package, dict) or package.get("name") != "xnip" or not isinstance(package.get("version"), str):
        raise ReleaseError("root manifest must contain package xnip with a literal version")
    version = package["version"]
    semver(version)
    return version


def release_relevant(path: str) -> bool:
    if path.startswith("src/"):
        return not path.endswith("/contract_tests.rs")
    return path in EXACT_RELEASE_PATHS or path.startswith(RELEASE_PREFIXES)


def development_version(current: str, suffix: str) -> str:
    parsed = semver(current)
    patch = int(parsed["patch"]) + (0 if parsed["pre"] else 1)
    version = f"{parsed['major']}.{parsed['minor']}.{patch}-dev.{suffix}"
    semver(version)
    return version


def has_stable_release_tag(root: Path, current: str, head: str) -> bool:
    """An atomic branch+stable-tag push belongs to the tagged release workflow."""
    if semver(current)["pre"]:
        return False
    reference = f"refs/tags/v{current}"
    # Use an exact ref-name comparison: similarly named tags and refs below the
    # same prefix must not suppress an unrelated development release.
    references = git(root, "for-each-ref", "--format=%(refname) %(objectname)", reference)
    for line in references.splitlines():
        name, object_id = line.split(b" ", 1)
        if name == reference.encode("ascii"):
            return commit(root, object_id.decode("ascii")) == head
    return False


def plan(root: Path, before: str, head: str, run_number: int) -> dict[str, object]:
    if run_number < 1:
        raise ReleaseError("run number must be positive")
    if SHA.fullmatch(before) is None:
        raise ReleaseError("before must be a complete hexadecimal SHA")
    resolved_head = commit(root, head)
    if set(before) == {"0"}:
        changed = git(root, "ls-tree", "-rz", "--name-only", resolved_head)
    else:
        resolved_before = commit(root, before)
        changed = git(
            root, "diff", "--no-renames", "--name-only", "-z",
            resolved_before, resolved_head, "--",
        )
    paths = [
        value.decode("utf-8", errors="surrogateescape")
        for value in changed.split(b"\0") if value
    ]
    current = package_version(git(root, "show", f"{resolved_head}:Cargo.toml"))
    version = development_version(current, f"{run_number}.g{resolved_head[:12]}")
    return {
        "publish": any(release_relevant(path) for path in paths)
        and not has_stable_release_tag(root, current, resolved_head),
        "prerelease": True,
        "version": version,
        "tag": f"v{version}",
    }


def version_info(root: Path, tag: str | None, local: bool, head: str | None) -> dict[str, object]:
    current = package_version((root / "Cargo.toml").read_bytes())
    if tag is not None and tag != f"v{current}":
        raise ReleaseError(f"tag {tag!r} does not match Cargo version v{current}")
    if local:
        if tag is not None:
            raise ReleaseError("--local and --tag are mutually exclusive")
        resolved_head = commit(root, head) if head else git(
            root, "rev-parse", "--verify", "HEAD^{commit}",
        ).decode("ascii").strip()
        timestamp = datetime.now(timezone.utc).strftime("%Y%m%d%H%M%S")
        current = development_version(current, f"local.{timestamp}.g{resolved_head[:12]}")
    return {"version": current, "tag": f"v{current}", "prerelease": bool(semver(current)["pre"])}


def value_span(text: str, start: int, end: int, expected: str) -> tuple[int, int]:
    matches = list(VERSION_LINE.finditer(text, start, end))
    if len(matches) != 1 or matches[0]["value"] != expected:
        raise ReleaseError("version must be a unique quoted assignment in its package block")
    return matches[0].span("value")


def stamp(root: Path, version: str) -> dict[str, object]:
    semver(version)
    manifest_path, lock_path = root / "Cargo.toml", root / "Cargo.lock"
    manifest_bytes, lock_bytes = manifest_path.read_bytes(), lock_path.read_bytes()
    current = package_version(manifest_bytes)
    manifest = manifest_bytes.decode("utf-8")
    try:
        lock = lock_bytes.decode("utf-8")
        lock_document = tomllib.loads(lock)
    except (UnicodeError, tomllib.TOMLDecodeError) as error:
        raise ReleaseError(f"invalid Cargo lockfile: {error}") from error

    manifest_headers = list(re.finditer(r"(?m)^\[[^\r\n]+\][ \t]*(?:#[^\r\n]*)?\r?$", manifest))
    package_headers = [item for item in manifest_headers if item.group().split("#", 1)[0].strip() == "[package]"]
    if len(package_headers) != 1:
        raise ReleaseError("manifest must have exactly one root package block")
    header = package_headers[0]
    block_end = next((item.start() for item in manifest_headers if item.start() > header.start()), len(manifest))
    manifest_span = value_span(manifest, header.end(), block_end, current)

    lock_packages = lock_document.get("package", [])
    if not isinstance(lock_packages, list) or any(not isinstance(item, dict) for item in lock_packages):
        raise ReleaseError("lockfile package must be an array of package tables")
    roots = [
        item for item in lock_packages
        if item.get("name") == "xnip" and "source" not in item
    ]
    if len(roots) != 1 or roots[0].get("version") != current:
        raise ReleaseError("lockfile must have exactly one local xnip package matching the manifest")
    lock_headers = list(re.finditer(r"(?m)^\[\[package\]\][ \t]*(?:#[^\r\n]*)?\r?$", lock))
    candidates: list[tuple[int, int]] = []
    for index, item in enumerate(lock_headers):
        end = lock_headers[index + 1].start() if index + 1 < len(lock_headers) else len(lock)
        block = tomllib.loads(lock[item.start():end])
        packages = block.get("package", [])
        if len(packages) == 1 and packages[0].get("name") == "xnip" and "source" not in packages[0]:
            candidates.append(value_span(lock, item.end(), end, current))
    if len(candidates) != 1:
        raise ReleaseError("cannot locate a unique local xnip lockfile block")
    lock_span = candidates[0]

    # Validate both documents before either write; untouched text keeps its exact bytes.
    updated_manifest = (manifest[:manifest_span[0]] + version + manifest[manifest_span[1]:]).encode("utf-8")
    updated_lock = (lock[:lock_span[0]] + version + lock[lock_span[1]:]).encode("utf-8")
    if current != version:
        manifest_path.write_bytes(updated_manifest)
        lock_path.write_bytes(updated_lock)
    return {"version": version, "tag": f"v{version}"}


def output_options(parser: argparse.ArgumentParser) -> None:
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--output", type=Path, help="Append GitHub Actions outputs to this file")
    group.add_argument("--github-output", action="store_true", help="Append to the GITHUB_OUTPUT environment path")


def emit(result: dict[str, object], args: argparse.Namespace) -> None:
    destination = args.output
    if args.github_output:
        value = os.environ.get("GITHUB_OUTPUT")
        if not value:
            raise ReleaseError("GITHUB_OUTPUT is not set")
        destination = Path(value)
    if destination is not None:
        with destination.open("a", encoding="utf-8", newline="\n") as stream:
            for name in ("publish", "version", "tag", "prerelease"):
                if name in result:
                    value = result[name]
                    stream.write(f"{name}={str(value).lower() if isinstance(value, bool) else value}\n")
    print(json.dumps(result, separators=(",", ":"), ensure_ascii=True))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    plan_parser = commands.add_parser("plan", help="Plan an automatic prerelease from an entire push range")
    plan_parser.add_argument("--before", required=True)
    plan_parser.add_argument("--head", required=True)
    plan_parser.add_argument("--run-number", type=int, required=True)
    plan_parser.add_argument("--root", type=Path, default=Path.cwd())
    output_options(plan_parser)
    version_parser = commands.add_parser("version", help="Read or validate the exact Cargo release version")
    version_parser.add_argument("--tag")
    version_parser.add_argument("--local", action="store_true")
    version_parser.add_argument("--head")
    version_parser.add_argument("--root", type=Path, default=Path.cwd())
    output_options(version_parser)
    stamp_parser = commands.add_parser("stamp", help="Stamp an isolated build checkout's manifest and lockfile")
    stamp_parser.add_argument("--version", required=True)
    stamp_parser.add_argument("--root", type=Path, default=Path.cwd())
    output_options(stamp_parser)
    args = parser.parse_args(argv)
    try:
        if args.command == "plan":
            result = plan(args.root, args.before, args.head, args.run_number)
        elif args.command == "version":
            result = version_info(args.root, args.tag, args.local, args.head)
        else:
            result = stamp(args.root, args.version)
        emit(result, args)
    except (ReleaseError, OSError) as error:
        print(f"release: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
