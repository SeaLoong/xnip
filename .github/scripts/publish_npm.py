#!/usr/bin/env python3
"""Validate seven same-commit npm packs and publish platforms before their parent."""

from __future__ import annotations

import argparse
import base64
from contextlib import ExitStack
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from npm import prepare

SPEC = importlib.util.spec_from_file_location("xnip_release_identity", ROOT / ".github/scripts/release.py")
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)
REGISTRY = "https://registry.npmjs.org"
NAMES = tuple(value[1] for value in prepare.PLATFORMS.values()) + (prepare.PARENT_NAME,)
MAX_PACK = 128 * 1024 * 1024
MAX_METADATA = 16 * 1024 * 1024
VISIBILITY_SECONDS = 12 * 60
VISIBILITY_DELAYS = (1, 2, 4, 8, 15, 30) + (60,) * 11
ANSI = re.compile(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07\x1b]*(?:\x07|\x1b\\))")
DEFINITE_CODES = frozenset({
    "E400", "E401", "E402", "E403", "E404", "E409", "EOTP", "ENEEDAUTH",
    "EAUTH", "EACCES", "EPERM", "E_STAGE_REQUIRED", "EPUBLISHCONFLICT",
    "EINTEGRITY", "EPRIVATE", "EBADSEMVER", "EUNSCOPED", "EUSAGE", "ETARGET",
})
NPM_REASONS = frozenset({
    "name-too-similar", "name-restricted", "name-cooldown", "2fa", "stage-required",
    "access", "immutable", "internal", "unknown",
})
NPM_CODES = DEFINITE_CODES | {
    "E429", "E500", "E502", "E503", "E504", "EINTERNAL", "ECONNRESET",
    "ETIMEDOUT", "ENOTFOUND", "ECONNREFUSED", "EAI_AGAIN", "ENETUNREACH",
    "EHOSTUNREACH", "ESOCKETTIMEDOUT",
}


class PublishError(Exception):
    """An uncertain identity must stop publication rather than overwrite anything."""


class VersionNotVisible(PublishError):
    """No registry version appeared within the bounded verification window."""


def classify_npm(stdout: str, stderr: str) -> tuple[str, str]:
    """Inspect captured diagnostics, but return only fixed codes/categories."""
    text = ANSI.sub("", stderr + "\n" + stdout)
    match = re.search(r"(?im)^npm (?:ERR!|error) code ([A-Z][A-Z0-9_]{1,40})\s*$", text)
    code = match[1].upper() if match and match[1].upper() in NPM_CODES else "UNKNOWN"
    for raw in (stderr, stdout):
        try:
            value = json.loads(ANSI.sub("", raw))
        except (ValueError, TypeError):
            continue
        if not isinstance(value, dict):
            continue
        error = value.get("error", value)
        if not isinstance(error, dict):
            continue
        candidate = error.get("code")
        if isinstance(candidate, str) and candidate.upper() in NPM_CODES:
            code = candidate.upper()
        text += "\n" + "\n".join(error[key] for key in ("summary", "detail", "message")
                                 if isinstance(error.get(key), str))
    lower = text.lower()
    if re.search(r"(?:too similar|similar to (?:an? )?existing package)", lower):
        reason = "name-too-similar"
    elif re.search(r"(?:name.{0,100}(?:restricted|reserved|not allowed|forbidden|blacklisted|spam)|package is blacklisted)", lower):
        reason = "name-restricted"
    elif "unpublish" in lower and re.search(r"(?:24\s*(?:hours?|hrs?)|too soon)", lower):
        reason = "name-cooldown"
    elif code == "E_STAGE_REQUIRED" or "e_stage_required" in lower or "stage-only" in lower:
        reason = "stage-required"
    elif code == "EOTP" or re.search(r"(?:two[- ]factor|2fa|one[- ]time (?:pass|code)|one-time password)", lower):
        reason = "2fa"
    elif code in {"EPUBLISHCONFLICT", "E409"} or re.search(r"(?:cannot publish over|previously published version|cannot modify pre-existing version)", lower):
        reason = "immutable"
    elif code in {"EINTERNAL", "E500", "E502", "E503", "E504"} or "npm internal error" in lower:
        reason = "internal"
    elif code in {"E400", "E401", "E402", "E403", "E404", "ENEEDAUTH", "EAUTH", "EACCES", "EPERM"}:
        reason = "access"
    else:
        reason = "unknown"
    return code, reason


def safe_candidate(path: str | None) -> str:
    if path:
        basename = str(path).replace("\\", "/").rsplit("/", 1)[-1]
        for name in NAMES:
            if basename.startswith(name + "-") and basename.endswith(".tgz"):
                version = basename[len(name) + 1:-4]
                try:
                    release.semver(version)
                except release.ReleaseError:
                    continue
                return basename
    return "<unavailable>"


class NpmFailure(PublishError):
    """No raw captured text, configuration, credentials or paths leave this type."""

    def __init__(self, code: str, reason: str, operation: str, candidate: str | None, exit_code=None):
        code = code if code in NPM_CODES else "UNKNOWN"
        reason = reason if reason in NPM_REASONS else "unknown"
        self.code, self.reason = code, reason
        self.definite = code in DEFINITE_CODES
        operation = operation if operation in {"publish", "whoami"} else "command"
        status = f"; exit {exit_code}" if type(exit_code) is int else ""
        super().__init__(f"npm {operation} failed ({code}; reason={reason}{status}; "
                         f"parent={prepare.PARENT_NAME}; candidate={safe_candidate(candidate)})")


def read_json(raw: bytes, description: str) -> dict:
    try:
        result = json.loads(raw)
    except (ValueError, UnicodeError) as error:
        raise PublishError(f"invalid JSON in {description}") from error
    if not isinstance(result, dict):
        raise PublishError(f"{description} must be an object")
    return result


def object_field(value: dict, key: str, description: str) -> dict:
    result = value.get(key)
    if not isinstance(result, dict):
        raise PublishError(f"{description} must be an object")
    return result


def regular_file(path: Path) -> bytes:
    if path.is_symlink() or not path.is_file() or not 0 < path.stat().st_size <= MAX_PACK:
        raise PublishError(f"missing, linked, empty or oversized pack: {path.name}")
    return path.read_bytes()


def pack_contents(path: Path) -> dict[str, bytes]:
    contents = {}
    expanded = 0
    with tarfile.open(path, "r:gz") as archive:
        for member in archive:
            parts = member.name.split("/")
            if member.isdir():
                continue
            if (not member.isfile() or parts[0] != "package" or len(parts) < 2
                    or any(part in ("", ".", "..") for part in parts)
                    or "\\" in member.name or member.name in contents):
                raise PublishError(f"unsafe or duplicate npm pack entry: {path.name}")
            expanded += member.size
            if member.size < 0 or expanded > 2 * MAX_PACK:
                raise PublishError(f"expanded npm pack is too large: {path.name}")
            stream = archive.extractfile(member)
            if stream is None:
                raise PublishError(f"unreadable npm pack entry: {path.name}")
            contents[member.name] = stream.read()
    return contents


def validate_prepared(directory: Path, source_commit: str, prerelease: bool) -> tuple[str, dict]:
    if re.fullmatch(r"[0-9a-f]{40}", source_commit) is None:
        raise PublishError("source commit must be an exact lowercase 40-hex SHA")
    report = read_json((directory / "preparation.json").read_bytes(), "preparation report")
    version = report.get("version")
    try:
        parsed = release.semver(version) if isinstance(version, str) else None
    except release.ReleaseError as error:
        raise PublishError(str(error)) from error
    if parsed is None or bool(parsed["pre"]) != prerelease or parsed["build"]:
        raise PublishError("npm version must be exact SemVer without build metadata and agree with prerelease")
    if (report.get("schema_version") != 2 or report.get("publication") != prepare.PUBLICATION_PUBLIC
            or report.get("source_commit") != source_commit or report.get("local_only") is not False
            or report.get("repository") != prepare.REPOSITORY):
        raise PublishError("report is not an authorized public same-commit preparation")
    manifest = object_field(report, "manifest", "native manifest")
    proof = object_field(manifest, "release", "release attestation")
    object_field(manifest, "platforms", "platform manifest")
    object_field(proof, "archives", "release archive manifest")
    if (manifest.get("schema_version") != 2 or manifest.get("version") != version or manifest.get("publication") != prepare.PUBLICATION_PUBLIC
            or manifest.get("localOnly") is not False or manifest.get("repository") != prepare.REPOSITORY
            or proof.get("schema_version") != 1 or proof.get("version") != version
            or proof.get("source_commit") != source_commit
            or set(manifest.get("platforms", {})) != set(prepare.PLATFORMS)
            or set(proof.get("archives", {})) != set(prepare.PLATFORMS)):
        raise PublishError("public preparation is missing the complete release attestation")
    records = report.get("tarballs")
    if not isinstance(records, list) or len(records) != len(NAMES):
        raise PublishError("exactly seven npm tarballs are required")
    packs = {}
    for record in records:
        if not isinstance(record, dict):
            raise PublishError("tarball record must be an object")
        name = record.get("name")
        if name not in NAMES or name in packs:
            raise PublishError("unexpected or duplicate npm package")
        # Reports can travel between runners. Resolve only the verified basename
        # under this artifact's tarballs directory, never its original absolute path.
        filename = record.get("filename")
        expected_filename = f"{name}-{version}.tgz"
        if filename != "tarballs/" + expected_filename or record.get("version") != version:
            raise PublishError(f"unexpected tarball filename: {name}")
        path = directory / "tarballs" / expected_filename
        raw = regular_file(path)
        integrity = "sha512-" + base64.b64encode(hashlib.sha512(raw).digest()).decode("ascii")
        if (record.get("integrity") != integrity or record.get("sha256") != hashlib.sha256(raw).hexdigest()
                or record.get("size") != len(raw)):
            raise PublishError(f"npm tarball integrity mismatch: {name}")
        contents = pack_contents(path)
        metadata = read_json(contents.get("package/package.json", b""), "packed package.json")
        if (metadata.get("name") != name or metadata.get("version") != version
                or metadata.get("private") is not False or metadata.get("repository") != prepare.REPOSITORY
                or metadata.get("publishConfig") != {"access": "public"}):
            raise PublishError(f"public package identity mismatch: {name}")
        if name == prepare.PARENT_NAME:
            native = read_json(contents.get("package/native-manifest.json", b""), "packed native manifest")
            dependencies = {value[1]: version for value in prepare.PLATFORMS.values()}
            if (native != manifest or metadata.get("optionalDependencies") != dependencies
                    or metadata.get("bundledDependencies") or metadata.get("bundleDependencies")):
                raise PublishError("parent pack must use the six exact public optional dependencies")
        else:
            platform = next(key for key, value in prepare.PLATFORMS.items() if value[1] == name)
            target = prepare.PLATFORMS[platform][0]
            entry = object_field(manifest["platforms"], platform, "platform entry")
            archive = object_field(proof["archives"], platform, "archive entry")
            executable = "xnip.exe" if platform.startswith("win32") else "xnip"
            binary = contents.get("package/" + executable, b"")
            binary_metadata = read_json(contents.get("package/binary.json", b""), "packed binary identity")
            digest = hashlib.sha256(binary).hexdigest()
            expected_archive = {"package": name, "target": target,
                                "archive_sha256": entry.get("archive_sha256"), "sha256": digest}
            if (entry.get("package") != name or entry.get("target") != target or entry.get("binary") != executable
                    or entry.get("source_commit") != source_commit or entry.get("sha256") != digest
                    or entry.get("size") != len(binary) or archive != expected_archive
                    or re.fullmatch(r"[0-9a-f]{64}", str(entry.get("archive_sha256", ""))) is None
                    or metadata.get("os") != [platform.split("-")[0]]
                    or metadata.get("cpu") != [platform.split("-")[1]]
                    or binary_metadata != {"schema_version": 2, "version": version,
                        "publication": prepare.PUBLICATION_PUBLIC, "localOnly": False, "release": proof, **entry}):
                raise PublishError(f"platform binary attestation mismatch: {name}")
            try:
                prepare.binary_build_identity(binary, version, target)
                prepare.executable_identity(binary, platform)
            except ValueError as error:
                raise PublishError(f"{name}: {error}") from error
        packs[name] = {"path": path, "integrity": integrity, "metadata": metadata}
    if set(path.name for path in (directory / "tarballs").iterdir()) != {
            f"{name}-{version}.tgz" for name in NAMES}:
        raise PublishError("tarball directory must contain exactly the seven verified npm packs")
    return version, packs


class Registry:
    def metadata(self, name: str, *, timeout: float = 30) -> dict | None:
        url = REGISTRY + "/" + urllib.parse.quote(name, safe="")
        request = urllib.request.Request(url, headers={"Accept": "application/json", "User-Agent": "xnip-release"})
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                raw = response.read(MAX_METADATA + 1)
        except urllib.error.HTTPError as error:
            error.close()
            if error.code == 404:
                return None
            raise PublishError(f"registry lookup failed for {name}: HTTP {error.code}") from error
        except (OSError, urllib.error.URLError) as error:
            raise PublishError(f"registry lookup unavailable for {name}") from error
        if len(raw) > MAX_METADATA:
            raise PublishError(f"registry metadata is oversized: {name}")
        result = read_json(raw, "registry metadata")
        if result.get("name") != name or not isinstance(result.get("versions"), dict):
            raise PublishError(f"registry returned an invalid package identity: {name}")
        return result


class Npm:
    def __init__(self, auth: str):
        self.auth = auth
        self.command = prepare.npm_command()

    def run(self, arguments: list[str]) -> None:
        candidate = arguments[1] if arguments[0] == "publish" and len(arguments) > 1 else None
        try:
            with ExitStack() as cleanup:
                options = {}
                if self.auth == "oidc":
                    # npm may fall back to token authentication after OIDC fails.
                    # A clean cwd and empty configs make that fallback impossible.
                    directory = Path(cleanup.enter_context(
                        tempfile.TemporaryDirectory(prefix="xnip-oidc-publish-")))
                    user_config, global_config = directory / "user.npmrc", directory / "global.npmrc"
                    for path in (user_config, global_config):
                        path.write_text("", encoding="utf-8")
                    environment = {key: value for key, value in os.environ.items()
                        if not key.upper().startswith("NPM_CONFIG_")
                        and key.upper() not in {"NODE_AUTH_TOKEN", "NPM_TOKEN", "NPM_AUTH_TOKEN"}}
                    environment.update(NPM_CONFIG_USERCONFIG=str(user_config),
                                       NPM_CONFIG_GLOBALCONFIG=str(global_config))
                    options = {"env": environment, "cwd": directory}
                result = subprocess.run([*self.command, *arguments, "--registry", REGISTRY],
                                        capture_output=True, text=True, errors="replace", timeout=180,
                                        check=False, **options)
        except subprocess.TimeoutExpired:
            raise NpmFailure("ETIMEDOUT", "unknown", arguments[0], candidate) from None
        if result.returncode:
            code, reason = classify_npm(result.stdout, result.stderr)
            raise NpmFailure(code, reason, arguments[0], candidate, result.returncode)

    def preflight(self) -> None:
        if self.auth == "token":
            if not os.environ.get("NODE_AUTH_TOKEN", "").strip():
                raise PublishError("NODE_AUTH_TOKEN is missing; configure repository NPM_TOKEN or NODE_AUTH_TOKEN")
            self.run(["whoami"])
        elif self.auth == "oidc":
            if not all(os.environ.get(key) for key in ("ACTIONS_ID_TOKEN_REQUEST_URL", "ACTIONS_ID_TOKEN_REQUEST_TOKEN")):
                raise PublishError("OIDC requires a trusted publisher and id-token: write on caller and called workflow")
            if os.environ.get("NODE_AUTH_TOKEN", "").strip():
                raise PublishError("OIDC mode must not receive NODE_AUTH_TOKEN")
        else:
            raise PublishError("auth must be explicitly token or oidc")

    def publish(self, path: Path, channel: str) -> None:
        self.run(["publish", str(path.resolve()), "--access", "public", "--tag", channel, "--ignore-scripts"])


def version_order(value: str) -> tuple:
    parsed = release.semver(value)
    pre = parsed["pre"]
    identifiers = tuple((0, int(item)) if item.isdigit() else (1, item) for item in pre.split(".")) if pre else ()
    return int(parsed["major"]), int(parsed["minor"]), int(parsed["patch"]), not bool(pre), identifiers


def verify_existing(metadata: dict | None, name: str, version: str, integrity: str) -> bool:
    if metadata is None:
        return False
    versions = metadata.get("versions")
    if not isinstance(versions, dict):
        raise PublishError(f"registry versions are malformed: {name}")
    entry = versions.get(version)
    if entry is None:
        return False
    if (not isinstance(entry, dict) or entry.get("name") != name or entry.get("version") != version
            or not isinstance(entry.get("dist"), dict) or entry["dist"].get("integrity") != integrity):
        raise PublishError(f"immutable registry version has different integrity: {name}@{version}")
    return True


def verify_channel(metadata: dict | None, name: str, version: str, channel: str, *, final: bool = False) -> None:
    if metadata is None:
        if final:
            raise PublishError(f"published package is not visible: {name}")
        return
    tags = object_field(metadata, "dist-tags", "registry dist-tags")
    current = tags.get(channel)
    if current is not None and not isinstance(current, str):
        raise PublishError(f"invalid registry channel version: {name}")
    if current == version:
        return
    if final or version in metadata.get("versions", {}):
        raise PublishError(f"{name} {channel} points to {current!r}, expected {version}; explicit promotion is required")
    if current is not None:
        try:
            newer = version_order(current) > version_order(version)
        except release.ReleaseError as error:
            raise PublishError(f"invalid registry channel version: {name}") from error
        if newer:
            raise PublishError(f"refusing to move {name} {channel} backwards from {current} to {version}")


class VisibilityWindow:
    """One shared deadline/backoff for an entire batch, including uncertain writes."""

    def __init__(self, registry, version: str, channel: str, packs: dict, wait):
        self.registry, self.version, self.channel, self.packs = registry, version, channel, packs
        self.wait = wait
        self.delays = iter(VISIBILITY_DELAYS)
        self.started = None
        self.waited = 0

    def remaining(self) -> float:
        if self.started is None:
            self.started = time.monotonic()
        # Counting scheduled waits also bounds tests whose injected wait is a no-op.
        return min(VISIBILITY_SECONDS - self.waited,
                   VISIBILITY_SECONDS - (time.monotonic() - self.started))

    def verify(self, names, publication_errors: dict) -> None:
        pending = list(names)
        def expired():
            for name in pending:
                if name in publication_errors:
                    raise publication_errors[name]
            identities = ", ".join(f"{name}@{self.version}" for name in pending)
            raise VersionNotVisible(f"published registry versions are not visible after the shared "
                f"12-minute window: {identities}; publication may still be processing. "
                "Inspect registry/staged status before retrying; no candidate was resubmitted.")
        while pending:
            for name in pending[:]:
                remaining = self.remaining()
                if remaining <= 0:
                    expired()
                metadata = self.registry.metadata(name, timeout=min(30, remaining))
                if verify_existing(metadata, name, self.version, self.packs[name]["integrity"]):
                    verify_channel(metadata, name, self.version, self.channel, final=True)
                    pending.remove(name)
                else:
                    # A placeholder, missing package or old packument is not proof
                    # that the request failed or that human approval is required.
                    verify_channel(metadata, name, self.version, self.channel)
            if not pending:
                return
            delay = next(self.delays, None)
            remaining = self.remaining()
            if delay is None or remaining <= 0:
                expired()
            delay = min(delay, remaining)
            self.wait(delay)
            self.waited += delay


def publish(directory: Path, source_commit: str, prerelease: bool = False, *, auth="token",
            registry=None, npm=None, wait=time.sleep) -> dict:
    version, packs = validate_prepared(directory, source_commit, prerelease)
    channel = "next" if prerelease else "latest"
    registry = registry or Registry()
    npm = npm or Npm(auth)
    # Validate authentication and every existing version before any partial publish.
    npm.preflight()
    existing = {}
    for name in NAMES:
        metadata = registry.metadata(name)
        existing[name] = verify_existing(metadata, name, version, packs[name]["integrity"])
        verify_channel(metadata, name, version, channel)
    statuses = {}
    def submit_and_verify(names):
        window = VisibilityWindow(registry, version, channel, packs, wait)
        submitted, publication_errors = [], {}
        for name in names:
            submitted.append(name)
            if existing[name]:
                statuses[name] = "skipped"
                continue
            try:
                npm.publish(packs[name]["path"], channel)
            except PublishError as publication_error:
                if isinstance(publication_error, NpmFailure) and publication_error.definite:
                    # A definite rejection does not need the visibility window.
                    # One read still covers a successful earlier HTTP attempt.
                    try:
                        metadata = registry.metadata(name)
                    except PublishError:
                        raise publication_error from None
                    if not verify_existing(metadata, name, version, packs[name]["integrity"]):
                        raise publication_error
                    verify_channel(metadata, name, version, channel, final=True)
                    statuses[name] = "verified-after-error"
                    continue
                # Stop new submissions until an uncertain request is resolved.
                # This shares the batch budget and never retries the write.
                publication_errors[name] = publication_error
                window.verify(submitted, publication_errors)
                statuses[name] = "verified-after-error"
            else:
                statuses[name] = "published"
        window.verify(names, publication_errors)

    # Ordinary successful submissions do not wait one platform at a time.
    # The parent becomes discoverable only after all six exact payloads are live.
    submit_and_verify(NAMES[:-1])
    submit_and_verify((prepare.PARENT_NAME,))
    return {"version": version, "source_commit": source_commit, "channel": channel,
            "authentication": auth, "packages": statuses}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepared", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--prerelease", action="store_true")
    parser.add_argument("--auth", choices=("token", "oidc"), default="token")
    parser.add_argument("--check", action="store_true", help="validate packs only, without authentication or registry requests")
    args = parser.parse_args(argv)
    try:
        if args.check:
            version, packs = validate_prepared(args.prepared, args.source_commit, args.prerelease)
            result = {"version": version, "source_commit": args.source_commit, "packages": len(packs), "status": "validated"}
        else:
            result = publish(args.prepared, args.source_commit, args.prerelease, auth=args.auth)
    except (PublishError, OSError, ValueError, tarfile.TarError, subprocess.SubprocessError) as error:
        print(f"publish-npm: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
