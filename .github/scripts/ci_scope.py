#!/usr/bin/env python3
"""Select CI scopes from Git history; never trust webhook file lists."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys


SHA = re.compile(r"(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})")
INTEGRATION_DOCS = {
    "docs/SKILL.md", "docs/apply-format.md", "docs/error-codes.md",
    "docs/examples.md", "docs/patch.md", "docs/recovery.md", "docs/edit.md",
}
EXECUTABLE_DOCS = {
    "docs/apply-format.md", "docs/examples.md", "docs/cli.md", "docs/mcp.md",
}
FULL_PATHS = {
    ".github/scripts/ci_scope.py", "tests/ci_scope_contract.py",
    ".github/scripts/verified_ci.cjs", "tests/verified_ci_contract.cjs",
}
RUST_PATHS = {
    "build.rs", "Cross.toml", "deny.toml", "rust-toolchain",
    "rust-toolchain.toml", "install.sh", "install.ps1",
    "tests/install_sh.py", "tests/install_ps1.ps1",
}
RELEASE_SCRIPTS = {".github/scripts/release.py", ".github/scripts/publish_release.py"}
RUST_PREFIXES = ("src/", ".cargo/", "xtask/", "installer/")
INTEGRATION_PREFIXES = ("integrations/", ".github/skills/", "xtask/")


class ScopeError(Exception):
    """An uncertain comparison must fail instead of silently skipping checks."""


def git(root: Path, *args: str, input_bytes: bytes | None = None) -> bytes:
    result = subprocess.run(
        ["git", "-C", str(root), *args], input=input_bytes,
        capture_output=True, check=False,
    )
    if result.returncode:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise ScopeError(f"git {' '.join(args[:2])} failed: {detail}")
    return result.stdout


def commit(root: Path, value: object, field: str) -> str:
    if not isinstance(value, str) or SHA.fullmatch(value) is None:
        raise ScopeError(f"{field} must be a complete hexadecimal commit SHA")
    return git(root, "rev-parse", "--verify", f"{value}^{{commit}}").decode("ascii").strip()


def classify(paths: list[str]) -> dict[str, bool]:
    """Keep allow-listed cheap changes narrow; unknown inputs require Rust."""
    full = any(path in FULL_PATHS or path.startswith(".github/workflows/") for path in paths)
    integrations = any(
        path in INTEGRATION_DOCS or path.startswith(INTEGRATION_PREFIXES)
        for path in paths
    )
    rust = False
    for path in paths:
        if (
            path in RUST_PATHS or path in EXECUTABLE_DOCS
            or path.startswith(RUST_PREFIXES)
            or ("/" not in path and path.startswith("Cargo"))
            or (path.startswith("benches/") and path.endswith(".rs"))
        ):
            rust = True
        elif (
            path in INTEGRATION_DOCS or path.startswith(INTEGRATION_PREFIXES)
            or path.startswith("docs/") or path.lower().endswith(".md")
            or path in RELEASE_SCRIPTS
            or (path.startswith(("tests/", "benches/")) and path.endswith(".py"))
            or ("/" not in path and path.upper().startswith(("LICENSE", "COPYING", "NOTICE")))
        ):
            continue
        else:
            rust = True
    return {"rust": rust or full, "integrations": integrations or full, "full": full}


def comparison_paths(root: Path, event: dict, event_name: str, head: str) -> list[str]:
    if event_name == "push":
        before = event.get("before")
        if not isinstance(before, str) or SHA.fullmatch(before) is None:
            raise ScopeError("push.before must be a complete hexadecimal commit SHA")
        if set(before) == {"0"}:
            # Hashing with the repository's object format also supports SHA-256 repos.
            base = git(root, "hash-object", "-w", "-t", "tree", "--stdin", input_bytes=b"")
            base = base.decode("ascii").strip()
        else:
            base = commit(root, before, "push.before")
    else:
        pull_request = event.get("pull_request")
        if not isinstance(pull_request, dict):
            raise ScopeError("pull_request payload is missing")
        base_info, head_info = pull_request.get("base"), pull_request.get("head")
        if not isinstance(base_info, dict) or not isinstance(head_info, dict):
            raise ScopeError("pull_request base/head payload is missing")
        base_sha = commit(root, base_info.get("sha"), "pull_request.base.sha")
        head_sha = commit(root, head_info.get("sha"), "pull_request.head.sha")
        base = git(root, "merge-base", base_sha, head_sha).decode("ascii").strip()
    changed = git(root, "diff", "--no-renames", "--name-only", "-z", base, head, "--")
    return [os.fsdecode(path) for path in changed.split(b"\0") if path]


def scope(
    root: Path, event: dict, event_name: str, sha: str, ref: str = "",
    force_all: bool = False,
) -> dict[str, bool]:
    head = commit(root, sha, "GITHUB_SHA")
    if not event_name:
        raise ScopeError("GITHUB_EVENT_NAME is missing")
    if force_all or event_name not in {"push", "pull_request"} or ref.startswith("refs/tags/"):
        return {"rust": True, "integrations": True, "full": True}
    result = classify(comparison_paths(root, event, event_name, head))
    repository = event.get("repository")
    default_branch = repository.get("default_branch") if isinstance(repository, dict) else None
    if (
        event_name == "push" and result["rust"]
        and isinstance(default_branch, str) and default_branch
        and ref == f"refs/heads/{default_branch}"
    ):
        # Default-branch runtime checks also validate the baseline consumed by releases.
        return {"rust": True, "integrations": True, "full": True}
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--event", type=Path, required=True, help="GitHub event JSON file")
    parser.add_argument("--output", type=Path, required=True, help="GITHUB_OUTPUT file")
    parser.add_argument("--root", type=Path, default=Path.cwd(), help="checked-out Git repository")
    parser.add_argument("--force-all", action="store_true", help="require every validation group")
    args = parser.parse_args()
    try:
        event = json.loads(args.event.read_text(encoding="utf-8"))
        if not isinstance(event, dict):
            raise ScopeError("GitHub event must be a JSON object")
        result = scope(
            args.root, event, os.environ.get("GITHUB_EVENT_NAME", ""),
            os.environ.get("GITHUB_SHA", ""), os.environ.get("GITHUB_REF", ""),
            args.force_all,
        )
        rendered = "".join(f"{name}={str(enabled).lower()}\n" for name, enabled in result.items())
        with args.output.open("a", encoding="utf-8", newline="\n") as output:
            output.write(rendered)
        print(rendered, end="")
    except (ScopeError, OSError, ValueError) as error:
        print(f"ci scope: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
