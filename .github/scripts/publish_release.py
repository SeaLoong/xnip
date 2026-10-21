#!/usr/bin/env python3
"""Verify a complete release, upload only to a draft, then publish through gh."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
from urllib.parse import quote


TARGETS = (
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
)
INTEGRATIONS = (
    "xnip-agents-md.tar.gz",
    "xnip-aider-conventions.tar.gz",
    "xnip-claude-code-skill.tar.gz",
    "xnip-codex-instructions.tar.gz",
    "xnip-copilot-prompt.tar.gz",
    "xnip-cursor-rule.tar.gz",
    "xnip-generic-skill.tar.gz",
)
SEMVER = re.compile(
    r"v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-(?P<pre>[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
)
SHA = re.compile(r"(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})")
CHECKSUM = re.compile(r"([0-9a-fA-F]{64})[ \t]+\*?([^ \t\r\n]+)(?:\r?\n)?")
MAX_ASSET_BYTES = 128 * 1024 * 1024
MAX_CHECKSUM_BYTES = 4096
DRAFT_LOOKUP_DELAYS = (0, 1, 2, 4, 8, 15, 30)


class PublishError(Exception):
    """Refuse incomplete, inconsistent or ambiguous releases without guessing."""


@dataclass(frozen=True)
class Asset:
    path: Path
    size: int
    digest: str


def validate_identity(tag: str, sha: str, prerelease: bool) -> str:
    match = SEMVER.fullmatch(tag)
    if match is None:
        raise PublishError("tag must be v followed by a valid semantic version")
    pre = match["pre"]
    if pre and any(part.isdigit() and len(part) > 1 and part.startswith("0") for part in pre.split(".")):
        raise PublishError("numeric prerelease identifiers cannot have leading zeros")
    if bool(pre) != prerelease:
        raise PublishError("--prerelease must agree with the tag's semantic version")
    if SHA.fullmatch(sha) is None:
        raise PublishError("sha must be a complete 40- or 64-character hexadecimal commit")
    return sha.lower()


def archive_names(tag: str) -> tuple[str, ...]:
    return tuple(
        f"xnip-{tag}-{target}{'.zip' if 'windows' in target else '.tar.gz'}"
        for target in TARGETS
    )


def expected_names(tag: str) -> set[str]:
    archives = archive_names(tag)
    return set(archives) | {name + ".sha256" for name in archives} | set(INTEGRATIONS) | {
        "install.sh", "install.ps1",
    }


def file_digest(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate_dist(tag: str, dist: Path) -> dict[str, Asset]:
    if not dist.is_dir():
        raise PublishError(f"distribution directory does not exist: {dist}")
    entries = list(dist.iterdir())
    actual = {entry.name for entry in entries}
    expected = expected_names(tag)
    if actual != expected:
        raise PublishError(
            f"distribution must contain exactly 21 assets; "
            f"missing={sorted(expected - actual)}, unexpected={sorted(actual - expected)}"
        )
    assets: dict[str, Asset] = {}
    for entry in entries:
        if entry.is_symlink() or not entry.is_file():
            raise PublishError(f"asset must be a regular file, not a link or directory: {entry.name}")
        if entry.stat().st_size == 0:
            raise PublishError(f"asset is empty: {entry.name}")
        assets[entry.name] = Asset(entry.resolve(), entry.stat().st_size, file_digest(entry))
    for archive in archive_names(tag):
        checksum_path = assets[archive + ".sha256"].path
        try:
            checksum = CHECKSUM.fullmatch(checksum_path.read_text(encoding="ascii"))
        except UnicodeError as error:
            raise PublishError(f"checksum is not ASCII: {checksum_path.name}") from error
        if checksum is None or checksum[2] != archive:
            raise PublishError(f"checksum must name only its matching archive: {checksum_path.name}")
        if checksum[1].lower() != assets[archive].digest:
            raise PublishError(f"archive checksum mismatch: {archive}")
    return assets


def release_notes(tag: str, sha: str, prerelease: bool, changelog: Path = Path("CHANGELOG.md")) -> str:
    title = f"{'Development prerelease' if prerelease else 'Release'} {tag}"
    details = ""
    if not prerelease:
        try:
            document = changelog.read_text(encoding="utf-8")
        except OSError as error:
            raise PublishError("stable releases require a matching CHANGELOG.md entry") from error
        section = re.search(
            rf"^## \[{re.escape(tag[1:])}\][^\r\n]*\r?\n(?P<body>.*?)(?=^## |\Z)",
            document, re.MULTILINE | re.DOTALL,
        )
        if section is None or not section["body"].strip():
            raise PublishError("stable releases require a nonempty matching changelog entry")
        details = section["body"].strip() + "\n\n"
    return f"{title}\n\n{details}Build commit: `{sha}`\n"


class GitHub:
    """A small gh adapter; tests replace this or its subprocess runner."""

    def __init__(self, repository: str | None = None, runner=None):
        self.repository = repository or os.environ.get("GITHUB_REPOSITORY") or os.environ.get("GH_REPO")
        if not self.repository or re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", self.repository) is None:
            raise PublishError("GITHUB_REPOSITORY or GH_REPO must identify OWNER/REPO")
        self.prefix = f"repos/{self.repository}"
        self.runner = runner or subprocess.run

    def run(self, *args: str):
        try:
            return self.runner(["gh", *args], capture_output=True, check=False)
        except OSError as error:
            raise PublishError(f"cannot execute gh: {error}") from error

    @staticmethod
    def response(result, context: str, missing: bool = False):
        raw = result.stdout
        if isinstance(raw, str):
            raw = raw.encode("utf-8")
        status_match = re.match(rb"HTTP/\S+ ([0-9]{3})[^\r\n]*\r?\n", raw)
        status = int(status_match[1]) if status_match else None
        stderr = result.stderr.decode("utf-8", errors="replace") if isinstance(result.stderr, bytes) else result.stderr
        if status is None:
            error_status = re.search(r"\(HTTP ([0-9]{3})\)", stderr or "")
            if error_status:
                status = int(error_status[1])
        if result.returncode or (status is not None and not 200 <= status < 300):
            if missing and status == 404:
                return None
            raise PublishError(f"{context} failed: {(stderr or '').strip()[:500] or f'HTTP {status}'}")
        if status is None:
            raise PublishError(f"{context} returned no HTTP status; refusing to treat it as missing")
        separator = b"\r\n\r\n" if b"\r\n\r\n" in raw else b"\n\n"
        try:
            return json.loads(raw.split(separator, 1)[1])
        except (IndexError, UnicodeError, json.JSONDecodeError) as error:
            raise PublishError(f"{context} returned invalid JSON") from error

    def api(self, endpoint: str, *, method: str = "GET", payload=None, missing: bool = False):
        args = ["api", "--include", "--method", method, endpoint]
        if payload is None:
            return self.response(self.run(*args), f"gh api {method} {endpoint}", missing)
        # gh reads JSON from a closed temporary file: booleans and newlines are preserved.
        with tempfile.TemporaryDirectory(prefix="xnip-release-api-") as folder:
            source = Path(folder) / "request.json"
            source.write_text(json.dumps(payload, ensure_ascii=True) + "\n", encoding="utf-8", newline="\n")
            return self.response(
                self.run(*args, "--input", str(source)), f"gh api {method} {endpoint}", missing,
            )

    def pages(self, endpoint: str) -> list[dict]:
        collected = []
        for page in range(1, 10001):
            values = self.api(f"{endpoint}?per_page=100&page={page}")
            if not isinstance(values, list) or any(not isinstance(item, dict) for item in values):
                raise PublishError(f"unexpected paginated response: {endpoint}")
            collected.extend(values)
            if len(values) < 100:
                return collected
        raise PublishError(f"pagination limit exceeded: {endpoint}")

    def release(self, release_id: int) -> dict:
        value = self.api(f"{self.prefix}/releases/{release_id}")
        if not isinstance(value, dict):
            raise PublishError("GitHub returned an invalid release")
        return value

    def find_release(self, tag: str) -> dict | None:
        value = self.api(f"{self.prefix}/releases/tags/{quote(tag, safe='')}", missing=True)
        if value is not None:
            if not isinstance(value, dict):
                raise PublishError("GitHub returned an invalid release")
            return value
        # /releases/tags is documented for published releases. Draft retries need
        # the authenticated list, which includes drafts with contents:write.
        matches = [item for item in self.pages(f"{self.prefix}/releases") if item.get("tag_name") == tag]
        if len(matches) > 1:
            raise PublishError(f"multiple release records use {tag}")
        return self.release(positive_id(matches[0].get("id"), "release")) if matches else None

    def tag_commit(self, tag: str) -> str | None:
        value = self.api(f"{self.prefix}/git/ref/tags/{quote(tag, safe='')}", missing=True)
        if value is None:
            return None
        seen: set[str] = set()
        for _ in range(32):
            obj = value.get("object") if isinstance(value, dict) else None
            sha = obj.get("sha") if isinstance(obj, dict) else None
            if not isinstance(sha, str) or SHA.fullmatch(sha) is None:
                raise PublishError("tag resolution returned an invalid object")
            sha = sha.lower()
            if obj.get("type") == "commit":
                return sha
            if obj.get("type") != "tag" or sha in seen:
                raise PublishError("tag does not resolve unambiguously to a commit")
            seen.add(sha)
            value = self.api(f"{self.prefix}/git/tags/{sha}")
        raise PublishError("annotated tag nesting limit exceeded")

    def assets(self, release_id: int) -> list[dict]:
        return self.pages(f"{self.prefix}/releases/{release_id}/assets")

    def asset_bytes(self, asset_id: int) -> bytes:
        result = self.run(
            "api", "-H", "Accept: application/octet-stream",
            f"{self.prefix}/releases/assets/{asset_id}",
        )
        if result.returncode:
            stderr = result.stderr.decode("utf-8", errors="replace") if isinstance(result.stderr, bytes) else result.stderr
            raise PublishError(f"cannot verify release asset {asset_id}: {(stderr or '').strip()[:500]}")
        return result.stdout if isinstance(result.stdout, bytes) else result.stdout.encode("utf-8")

    def create_draft(self, tag: str, sha: str, prerelease: bool) -> None:
        with tempfile.TemporaryDirectory(prefix="xnip-release-notes-") as folder:
            notes = Path(folder) / "notes.md"
            notes.write_text(
                release_notes(tag, sha, prerelease),
                encoding="utf-8", newline="\n",
            )
            args = [
                "release", "create", tag, "--repo", self.repository, "--draft", "--target", sha,
                "--title", tag, "--latest=false", "--notes-file", str(notes),
            ]
            if prerelease:
                args.append("--prerelease")
            result = self.run(*args)
            if result.returncode:
                stderr = result.stderr.decode("utf-8", errors="replace") if isinstance(result.stderr, bytes) else result.stderr
                raise PublishError(f"cannot create release draft: {(stderr or '').strip()[:500]}")

    def upload(self, release_id: int, tag: str, assets: dict[str, Asset]) -> None:
        # Recheck immediately before --clobber. The workflow also serializes
        # publishers; an already published release is never intentionally mutated.
        current = self.release(release_id)
        if current.get("draft") is not True or current.get("tag_name") != tag:
            raise PublishError("refusing to upload or clobber assets of a published release or a retagged draft")
        result = self.run(
            "release", "upload", tag, "--repo", self.repository, "--clobber",
            *(str(assets[name].path) for name in sorted(assets)),
        )
        if result.returncode:
            stderr = result.stderr.decode("utf-8", errors="replace") if isinstance(result.stderr, bytes) else result.stderr
            raise PublishError(f"release asset upload failed: {(stderr or '').strip()[:500]}")

    def publish(self, release_id: int, sha: str, prerelease: bool) -> dict:
        value = self.api(
            f"{self.prefix}/releases/{release_id}", method="PATCH",
            payload={
                "draft": False, "prerelease": prerelease, "target_commitish": sha,
                "make_latest": "false" if prerelease else "true",
            },
        )
        if not isinstance(value, dict):
            raise PublishError("GitHub returned an invalid published release")
        return value


def positive_id(value, kind: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value < 1:
        raise PublishError(f"invalid GitHub {kind} id")
    return value


def verify_release(value: dict, tag: str, sha: str, prerelease: bool) -> int:
    if value.get("tag_name") != tag:
        raise PublishError("release tag does not match the requested tag")
    if value.get("prerelease") is not prerelease or not isinstance(value.get("draft"), bool):
        raise PublishError("release mode does not match the requested release mode")
    target = value.get("target_commitish")
    if not isinstance(target, str) or target.lower() != sha:
        raise PublishError("release target_commitish does not match the requested commit")
    return positive_id(value.get("id"), "release")


def verify_tag(gh, tag: str, sha: str, *, required: bool) -> None:
    target = gh.tag_commit(tag)
    if target is None:
        if required:
            raise PublishError("published release tag is missing")
    elif target != sha:
        raise PublishError(f"tag {tag} points to {target}, expected {sha}")


def remote_index(remote: list[dict], expected: set[str]) -> dict[str, dict]:
    if any(not isinstance(item, dict) for item in remote):
        raise PublishError("remote release contains an invalid asset record")
    names = [item.get("name") for item in remote]
    if len(names) != len(expected) or any(not isinstance(name, str) for name in names) or set(names) != expected:
        raise PublishError("remote release must contain exactly the expected 21 unique assets")
    for item in remote:
        size = item.get("size")
        if item.get("state") != "uploaded" or not isinstance(size, int) or isinstance(size, bool) or not 0 < size <= MAX_ASSET_BYTES:
            raise PublishError(f"remote asset is incomplete or exceeds its size limit: {item['name']}")
    return {item["name"]: item for item in remote}


def api_digest(item: dict) -> str | None:
    digest = item.get("digest")
    if isinstance(digest, str) and re.fullmatch(r"sha256:[0-9a-fA-F]{64}", digest):
        return digest.split(":", 1)[1].lower()
    return None


def read_remote_asset(gh, item: dict, limit: int = MAX_ASSET_BYTES) -> bytes:
    size = item["size"]
    if size > limit:
        raise PublishError(f"remote asset exceeds its verification limit: {item['name']}")
    data = gh.asset_bytes(positive_id(item.get("id"), "asset"))
    if len(data) != size or len(data) > limit:
        raise PublishError(f"remote asset download size mismatch: {item['name']}")
    declared = api_digest(item)
    if declared is not None and hashlib.sha256(data).hexdigest() != declared:
        raise PublishError(f"download does not match GitHub's asset digest: {item['name']}")
    return data


def remote_digest(gh, item: dict) -> str:
    return api_digest(item) or hashlib.sha256(read_remote_asset(gh, item)).hexdigest()


def verify_assets(gh, remote: list[dict], local: dict[str, Asset]) -> None:
    """An upload must match all 21 files from this exact build byte for byte."""
    for name, item in remote_index(remote, set(local)).items():
        expected = local[name]
        if item["size"] != expected.size or remote_digest(gh, item) != expected.digest:
            raise PublishError(f"remote asset checksum or size mismatch: {name}")


def verify_published_assets(gh, remote: list[dict], tag: str) -> None:
    """A rerun verifies immutable published artifacts, not fresh archive mtimes."""
    indexed = remote_index(remote, expected_names(tag))
    for archive in archive_names(tag):
        sidecar = indexed[archive + ".sha256"]
        try:
            text = read_remote_asset(gh, sidecar, MAX_CHECKSUM_BYTES).decode("ascii")
        except UnicodeError as error:
            raise PublishError(f"published checksum is not ASCII: {sidecar['name']}") from error
        checksum = CHECKSUM.fullmatch(text)
        if checksum is None or checksum[2] != archive:
            raise PublishError(f"published checksum must name its matching archive: {sidecar['name']}")
        if checksum[1].lower() != remote_digest(gh, indexed[archive]):
            raise PublishError(f"published archive checksum mismatch: {archive}")


def wait_for_created_draft(gh, tag: str, wait) -> dict:
    # A successful create can precede visibility in both the tag endpoint and
    # authenticated draft listing. Retry only explicit absence, never creation.
    for delay in DRAFT_LOOKUP_DELAYS:
        if delay:
            wait(delay)
        release = gh.find_release(tag)
        if release is not None:
            return release
    raise PublishError("created release draft did not become visible within the bounded lookup window; "
                       "the retained draft can be resumed on a later run")


def publish(tag: str, sha: str, dist: Path, prerelease: bool = False, gh=None, *, wait=time.sleep) -> dict[str, object]:
    sha = validate_identity(tag, sha, prerelease)
    local = validate_dist(tag, dist)
    gh = gh or GitHub()
    verify_tag(gh, tag, sha, required=False)
    release = gh.find_release(tag)
    if release is None:
        gh.create_draft(tag, sha, prerelease)
        release = wait_for_created_draft(gh, tag, wait)
        if release.get("draft") is not True:
            raise PublishError("new release was unexpectedly published before asset verification")
    release_id = verify_release(release, tag, sha, prerelease)
    if release["draft"] is False:
        verify_tag(gh, tag, sha, required=True)
        verify_published_assets(gh, gh.assets(release_id), tag)
        return {"status": "skipped", "tag": tag, "sha": sha, "assets": len(local), "prerelease": prerelease}
    # Unexpected names on a draft are not deleted: somebody may own those assets.
    existing = gh.assets(release_id)
    existing_names = [item.get("name") for item in existing]
    if any(not isinstance(name, str) or name not in local for name in existing_names) or len(set(existing_names)) != len(existing_names):
        raise PublishError("draft contains unexpected or duplicate assets")
    verify_tag(gh, tag, sha, required=False)
    gh.upload(release_id, tag, local)
    current = gh.release(release_id)
    verify_release(current, tag, sha, prerelease)
    if current["draft"] is not True:
        raise PublishError("draft was published by another publisher during upload")
    verify_assets(gh, gh.assets(release_id), local)
    verify_tag(gh, tag, sha, required=False)
    final = gh.publish(release_id, sha, prerelease)
    verify_release(final, tag, sha, prerelease)
    if final["draft"] is not False:
        raise PublishError("GitHub did not publish the verified draft")
    verify_tag(gh, tag, sha, required=True)
    return {"status": "published", "tag": tag, "sha": sha, "assets": len(local), "prerelease": prerelease}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--dist", type=Path, required=True)
    parser.add_argument("--prerelease", action="store_true")
    args = parser.parse_args(argv)
    try:
        result = publish(args.tag, args.sha, args.dist, args.prerelease)
    except (PublishError, OSError) as error:
        print(f"publish-release: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
