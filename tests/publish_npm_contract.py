#!/usr/bin/env python3
"""Offline contracts for public pack identity, immutable npm retries and channels."""

from __future__ import annotations

import base64
import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import struct
import sys
import tarfile
import tempfile
import textwrap
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("xnip_publish_npm", ROOT / ".github/scripts/publish_npm.py")
publisher = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publisher)
SHA = "a" * 40
VERSION = "0.5.0"
PARENT = publisher.prepare.PARENT_NAME


def executable(platform, version, target):
    data = bytearray(128)
    if platform.startswith("linux"):
        data[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<H", data, 18, 62 if platform.endswith("x64") else 183)
        struct.pack_into("<Q", data, 32, 64)
        struct.pack_into("<HH", data, 54, 56, 1)
        struct.pack_into("<I", data, 64, 1)
    elif platform.startswith("darwin"):
        data[:4] = b"\xcf\xfa\xed\xfe"
        struct.pack_into("<I", data, 4, 0x1000007 if platform.endswith("x64") else 0x100000c)
    else:
        data[:2] = b"MZ"
        struct.pack_into("<I", data, 60, 64)
        data[64:68] = b"PE\0\0"
        struct.pack_into("<H", data, 68, 0x8664 if platform.endswith("x64") else 0xaa64)
    return bytes(data) + f"\0XNIP_BUILD_ID_V1:{version}|{target}\0".encode()


class Fixture:
    def __init__(self, directory, version=VERSION):
        self.directory = directory
        self.version = version
        (directory / "tarballs").mkdir()
        entries, archives, self.binaries = {}, {}, {}
        for key, (target, name) in publisher.prepare.PLATFORMS.items():
            binary = executable(key, version, target)
            self.binaries[name] = binary
            entries[key] = {"package": name, "target": target,
                "binary": "xnip.exe" if key.startswith("win32") else "xnip",
                "sha256": hashlib.sha256(binary).hexdigest(), "size": len(binary),
                "format": publisher.prepare.executable_identity(binary, key),
                "archive_sha256": "b" * 64, "source_commit": SHA}
            archives[key] = {field: entries[key][field] for field in ("package", "target", "sha256", "archive_sha256")}
        self.proof = {"schema_version": 1, "version": version, "source_commit": SHA, "archives": archives}
        self.manifest = {"schema_version": 2, "version": version, "publication": "public-release",
            "localOnly": False, "repository": publisher.prepare.REPOSITORY, "release": self.proof, "platforms": entries}
        self.report = {"schema_version": 2, "version": version, "publication": "public-release",
            "source_commit": SHA, "repository": publisher.prepare.REPOSITORY, "local_only": False,
            "manifest": self.manifest, "tarballs": []}
        self.contents = {}
        for name in publisher.NAMES:
            metadata = {"name": name, "version": version, "private": False,
                "repository": publisher.prepare.REPOSITORY, "publishConfig": {"access": "public"}}
            contents = {}
            if name == PARENT:
                metadata["optionalDependencies"] = {value[1]: version for value in publisher.prepare.PLATFORMS.values()}
                contents["package/native-manifest.json"] = json.dumps(self.manifest).encode()
            else:
                key = next(key for key, value in publisher.prepare.PLATFORMS.items() if value[1] == name)
                entry = entries[key]
                metadata.update({"os": [key.split("-")[0]], "cpu": [key.split("-")[1]]})
                contents["package/" + entry["binary"]] = self.binaries[name]
                contents["package/binary.json"] = json.dumps({"schema_version": 2, "version": version,
                    "publication": "public-release", "localOnly": False, "release": self.proof, **entry}).encode()
            contents["package/package.json"] = json.dumps(metadata).encode()
            self.contents[name] = contents
            self.repack(name)
        self.save()

    def repack(self, name):
        filename = f"{name}-{self.version}.tgz"
        destination = self.directory / "tarballs" / filename
        with tarfile.open(destination, "w:gz") as archive:
            for key, value in self.contents[name].items():
                member = tarfile.TarInfo(key)
                member.size = len(value)
                archive.addfile(member, io.BytesIO(value))
        raw = destination.read_bytes()
        record = {"name": name, "version": self.version, "filename": "tarballs/" + filename,
                  "size": len(raw), "sha256": hashlib.sha256(raw).hexdigest(),
                  "integrity": "sha512-" + base64.b64encode(hashlib.sha512(raw).digest()).decode()}
        self.report["tarballs"] = [entry for entry in self.report["tarballs"] if entry["name"] != name] + [record]
        self.save()

    def save(self):
        (self.directory / "preparation.json").write_text(json.dumps(self.report), encoding="utf-8")

    def metadata(self, name):
        integrity = next(item["integrity"] for item in self.report["tarballs"] if item["name"] == name)
        return {"name": name, "versions": {self.version: {"name": name, "version": self.version,
                "dist": {"integrity": integrity}}}, "dist-tags": {"next" if "-" in self.version else "latest": self.version}}


class Registry:
    def __init__(self, fixture):
        self.fixture = fixture
        self.packages = {}
        self.events = []
    def metadata(self, name, *, timeout=30):
        self.events.append(("lookup", name))
        return copy.deepcopy(self.packages.get(name))


class Npm:
    def __init__(self, registry):
        self.registry = registry
        self.events = []
        self.fail = None
        self.lost_response = None
        self.authentication_error = False
    def preflight(self):
        self.events.append(("auth",))
        if self.authentication_error:
            raise publisher.PublishError("missing token")
    def publish(self, path, channel):
        name = next(name for name in publisher.NAMES if path.name == f"{name}-{self.registry.fixture.version}.tgz")
        self.events.append(("publish", name, channel))
        if name == self.fail:
            raise publisher.PublishError("server rejected request")
        self.registry.packages[name] = self.registry.fixture.metadata(name)
        if name == self.lost_response:
            raise publisher.PublishError("connection lost after successful publish")


class PackContracts(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="xnip-public-pack-")
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.fixture = Fixture(self.directory)
    def validate(self, **kwargs):
        return publisher.validate_prepared(self.directory, kwargs.get("sha", SHA), kwargs.get("prerelease", False))
    def test_exact_seven_public_same_commit_packs_are_portable(self):
        version, packs = self.validate()
        self.assertEqual(version, VERSION)
        self.assertEqual(tuple(packs), publisher.NAMES)
        self.assertTrue(all(value["path"].parent == self.directory / "tarballs" for value in packs.values()))
    def test_malformed_manifest_and_tarball_shapes_fail_with_contract_errors(self):
        original = copy.deepcopy(self.fixture.report)
        for field, value in [("manifest", []), ("manifest", {"release": []}),
                             ("tarballs", [None] * 7)]:
            self.fixture.report = {**copy.deepcopy(original), field: value}
            self.fixture.save()
            with self.subTest(field=field), self.assertRaises(publisher.PublishError):
                self.validate()
        self.fixture.report = original
        self.fixture.report["manifest"]["platforms"]["linux-x64"] = []
        self.fixture.save()
        with self.assertRaises(publisher.PublishError):
            self.validate()
    def test_wrong_sha_private_local_or_incomplete_reports_fail(self):
        for field, value in [("source_commit", "b" * 40), ("publication", "disabled-private-packages"),
                             ("local_only", True), ("schema_version", 1), ("repository", {})]:
            with self.subTest(field=field):
                original = copy.deepcopy(self.fixture.report)
                self.fixture.report[field] = value
                self.fixture.save()
                with self.assertRaises(publisher.PublishError):
                    self.validate()
                self.fixture.report = original
        self.fixture.save()
        with self.assertRaises(publisher.PublishError):
            self.validate(sha="HEAD")
    def test_prerelease_must_match_and_build_metadata_is_rejected(self):
        with self.assertRaises(publisher.PublishError):
            self.validate(prerelease=True)
        for version in ("0.5.0+build.1", "0.5.0-dev.01", "0.5.0\n"):
            self.fixture.report["version"] = version
            self.fixture.save()
            with self.subTest(version=version), self.assertRaises(publisher.PublishError):
                self.validate()
    def test_missing_duplicate_and_unexpected_tarballs_fail(self):
        original = copy.deepcopy(self.fixture.report)
        for records in (original["tarballs"][:-1], original["tarballs"][:-1] + [original["tarballs"][0]],
                        original["tarballs"] + [original["tarballs"][0]]):
            self.fixture.report["tarballs"] = records
            self.fixture.save()
            with self.assertRaises(publisher.PublishError):
                self.validate()
    def test_size_hash_and_integrity_must_all_match(self):
        for field, value in [("size", 1), ("sha256", "0" * 64), ("integrity", "sha512-invalid")]:
            original = copy.deepcopy(self.fixture.report)
            self.fixture.report["tarballs"][0][field] = value
            self.fixture.save()
            with self.subTest(field=field), self.assertRaisesRegex(publisher.PublishError, "integrity"):
                self.validate()
            self.fixture.report = original
    def test_report_cannot_select_an_external_or_absolute_tarball(self):
        record = self.fixture.report["tarballs"][0]
        for filename in ("../outside.tgz", "/absolute.tgz", "tarballs/../evil.tgz"):
            record["filename"] = filename
            self.fixture.save()
            with self.subTest(filename=filename), self.assertRaisesRegex(publisher.PublishError, "filename"):
                self.validate()
    def test_extra_tarball_is_not_ignored(self):
        (self.directory / "tarballs" / "unexpected.tgz").write_bytes(b"untrusted")
        with self.assertRaisesRegex(publisher.PublishError, "exactly"):
            self.validate()
    def test_parent_identity_is_cli_while_six_payload_names_and_executables_stay_xnip(self):
        self.assertEqual(PARENT, "xnip-cli")
        self.assertEqual(publisher.NAMES[:-1], (
            "xnip-linux-x64-musl", "xnip-linux-arm64-musl", "xnip-darwin-x64",
            "xnip-darwin-arm64", "xnip-win32-x64", "xnip-win32-arm64",
        ))
        metadata = json.loads(self.fixture.contents[PARENT]["package/package.json"])
        metadata["name"] = "xnip"
        self.fixture.contents[PARENT]["package/package.json"] = json.dumps(metadata).encode()
        self.fixture.repack(PARENT)
        with self.assertRaisesRegex(publisher.PublishError, "identity"):
            self.validate()
    def test_packed_private_or_wrong_optional_versions_fail_even_with_new_integrity(self):
        name = PARENT
        original = copy.deepcopy(self.fixture.contents[name])
        for field, value in [("private", True), ("version", "0.4.0"), ("repository", {}),
                             ("optionalDependencies", {}), ("bundledDependencies", ["xnip-linux-x64-musl"])]:
            metadata = json.loads(original["package/package.json"])
            metadata[field] = value
            self.fixture.contents[name] = {**original, "package/package.json": json.dumps(metadata).encode()}
            self.fixture.repack(name)
            with self.subTest(field=field), self.assertRaises(publisher.PublishError):
                self.validate()
    def test_wrong_binary_even_rehashed_pack_fails_attestation(self):
        name = publisher.NAMES[0]
        key = "package/xnip"
        self.fixture.contents[name][key] += b"tampered"
        self.fixture.repack(name)
        with self.assertRaisesRegex(publisher.PublishError, "attestation"):
            self.validate()
    def test_tar_traversal_is_rejected_without_extracting(self):
        name = publisher.NAMES[0]
        self.fixture.contents[name]["package/../escape"] = b"escape"
        self.fixture.repack(name)
        with self.assertRaisesRegex(publisher.PublishError, "unsafe"):
            self.validate()
        self.assertFalse((self.directory / "escape").exists())
    def test_packed_target_cpu_metadata_is_checked(self):
        name = publisher.NAMES[0]
        metadata = json.loads(self.fixture.contents[name]["package/package.json"])
        metadata["cpu"] = ["arm64"]
        self.fixture.contents[name]["package/package.json"] = json.dumps(metadata).encode()
        self.fixture.repack(name)
        with self.assertRaisesRegex(publisher.PublishError, "attestation"):
            self.validate()


class PublicationContracts(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="xnip-npm-publication-")
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.fixture = Fixture(self.directory)
        self.registry = Registry(self.fixture)
        self.npm = Npm(self.registry)
        self.waits = []
    def publish(self, prerelease=False):
        return publisher.publish(self.directory, SHA, prerelease, registry=self.registry, npm=self.npm, wait=self.waits.append)
    def test_platforms_publish_before_parent_and_all_seven_are_verified(self):
        result = self.publish()
        self.assertEqual(result["channel"], "latest")
        self.assertEqual(result["authentication"], "token")
        self.assertEqual([event[1] for event in self.npm.events if event[0] == "publish"], list(publisher.NAMES))
        self.assertEqual(list(result["packages"].values()), ["published"] * 7)
    def test_successful_native_submissions_are_batched_before_delayed_visibility_and_parent(self):
        elapsed = [0]
        available, verified, trace = {}, set(), []
        original_metadata, original_publish = self.registry.metadata, self.npm.publish
        def metadata(name, *, timeout=30):
            value = original_metadata(name, timeout=timeout)
            if name not in available:
                return value
            trace.append(("poll", name))
            if elapsed[0] < available[name]:
                return {"name": name, "versions": {"0.0.0-stage": {}},
                        "dist-tags": {"latest": "0.0.0-stage"}}
            verified.add(name)
            return value
        def submit(path, channel):
            name = next(name for name in publisher.NAMES if path.name == f"{name}-{VERSION}.tgz")
            if name == PARENT:
                self.assertTrue(set(publisher.NAMES[:-1]).issubset(verified))
            trace.append(("submit", name))
            original_publish(path, channel)
            available[name] = elapsed[0] + 300
        def wait(seconds):
            self.waits.append(seconds)
            elapsed[0] += seconds
        with patch.object(self.registry, "metadata", side_effect=metadata), patch.object(self.npm, "publish", side_effect=submit):
            result = publisher.publish(self.directory, SHA, registry=self.registry, npm=self.npm, wait=wait)
        self.assertEqual(trace[:6], [("submit", name) for name in publisher.NAMES[:-1]])
        self.assertEqual([event[1] for event in trace if event[0] == "submit"], list(publisher.NAMES))
        self.assertEqual(set(result["packages"].values()), {"published"})
        self.assertEqual(elapsed[0], 600)  # independent native and parent windows
    def test_invisible_successful_native_batch_waits_once_and_never_resubmits_or_exposes_parent(self):
        original = self.registry.metadata
        def metadata(name, *, timeout=30):
            return None if name in self.registry.packages else original(name, timeout=timeout)
        with patch.object(self.registry, "metadata", side_effect=metadata):
            with self.assertRaisesRegex(publisher.VersionNotVisible, "publication may still be processing"):
                self.publish()
        self.assertEqual([event[1] for event in self.npm.events if event[0] == "publish"], list(publisher.NAMES[:-1]))
        self.assertEqual(sum(self.waits), 720)
        self.assertNotIn(PARENT, self.registry.packages)
    def test_immutable_mismatch_after_submission_blocks_parent_without_visibility_retry(self):
        original = self.registry.metadata
        def metadata(name, *, timeout=30):
            value = original(name, timeout=timeout)
            if value is not None:
                value["versions"][VERSION]["dist"]["integrity"] = "sha512-different"
            return value
        with patch.object(self.registry, "metadata", side_effect=metadata):
            with self.assertRaisesRegex(publisher.PublishError, "different integrity"):
                self.publish()
        self.assertEqual(self.waits, [])
        self.assertNotIn(PARENT, self.registry.packages)
    def test_prerelease_explicitly_uses_next_for_every_package(self):
        with tempfile.TemporaryDirectory(prefix="xnip-next-") as folder:
            fixture = Fixture(Path(folder), "0.5.1-dev.7.gaaaaaaaaaaaa")
            registry = Registry(fixture)
            npm = Npm(registry)
            result = publisher.publish(Path(folder), SHA, True, registry=registry, npm=npm, wait=self.waits.append)
            self.assertEqual(result["channel"], "next")
            self.assertTrue(all(event[2] == "next" for event in npm.events if event[0] == "publish"))
    def test_exact_existing_versions_skip_without_writes(self):
        for name in publisher.NAMES:
            self.registry.packages[name] = self.fixture.metadata(name)
        result = self.publish()
        self.assertEqual(self.npm.events, [("auth",)])
        self.assertEqual(set(result["packages"].values()), {"skipped"})
    def test_conflicting_parent_integrity_blocks_all_platform_writes(self):
        value = self.fixture.metadata(PARENT)
        value["versions"][VERSION]["dist"]["integrity"] = "sha512-different"
        self.registry.packages[PARENT] = value
        with self.assertRaisesRegex(publisher.PublishError, "different integrity"):
            self.publish()
        self.assertEqual(self.npm.events, [("auth",)])
    def test_missing_auth_blocks_any_registry_lookup_or_publication(self):
        self.npm.authentication_error = True
        with self.assertRaisesRegex(publisher.PublishError, "token"):
            self.publish()
        self.assertEqual(self.registry.events, [])
        self.assertEqual(self.npm.events, [("auth",)])
    def test_partial_failure_never_exposes_parent_and_retry_skips_uploaded_platforms(self):
        self.npm.fail = publisher.NAMES[2]
        with self.assertRaisesRegex(publisher.PublishError, "server rejected request"):
            self.publish()
        self.assertNotIn(PARENT, self.registry.packages)
        self.assertEqual(set(self.registry.packages), set(publisher.NAMES[:2]))
        self.npm.fail = None
        self.npm.events = []
        result = self.publish()
        self.assertEqual([event[1] for event in self.npm.events if event[0] == "publish"], list(publisher.NAMES[2:]))
        self.assertEqual([result["packages"][name] for name in publisher.NAMES[:2]], ["skipped", "skipped"])
    def test_failed_publish_preserves_safe_npm_error_after_absent_registry_verification(self):
        with patch.object(self.npm, "publish", side_effect=publisher.PublishError(
                "npm publish failed (E403; exit 1); verify publishing permissions")):
            with self.assertRaisesRegex(publisher.PublishError, "E403") as caught:
                self.publish()
        self.assertNotIn("not visible", str(caught.exception))
        self.assertEqual(self.waits, list(publisher.VISIBILITY_DELAYS))
        self.assertNotIn(PARENT, self.registry.packages)
    def test_definite_rejections_read_remote_once_and_fail_without_visibility_wait(self):
        for code, reason in (("E403", "name-too-similar"), ("E401", "access"),
                             ("EOTP", "2fa"), ("E_STAGE_REQUIRED", "stage-required")):
            self.registry.events.clear()
            failure = publisher.NpmFailure(code, reason, "publish",
                str(self.directory / f"{publisher.NAMES[0]}-{VERSION}.tgz"), 1)
            with self.subTest(code=code), patch.object(self.npm, "publish", side_effect=failure) as submit:
                with self.assertRaisesRegex(publisher.NpmFailure, code):
                    self.publish()
                self.assertEqual(submit.call_count, 1)
            self.assertEqual(len(self.registry.events), len(publisher.NAMES) + 1)
            self.assertEqual(self.registry.events[-1], ("lookup", publisher.NAMES[0]))
            self.assertEqual(self.waits, [])
            self.assertNotIn(PARENT, self.registry.packages)
    def test_definite_rejection_can_verify_an_exact_successful_earlier_attempt(self):
        original = self.npm.publish
        def submit(path, channel):
            original(path, channel)
            if path.name == f"{publisher.NAMES[0]}-{VERSION}.tgz":
                raise publisher.NpmFailure("E403", "access", "publish", str(path), 1)
        with patch.object(self.npm, "publish", side_effect=submit):
            result = self.publish()
        self.assertEqual(result["packages"][publisher.NAMES[0]], "verified-after-error")
        self.assertEqual(result["packages"][PARENT], "published")
        self.assertEqual(self.waits, [])
    def test_definite_error_never_hides_a_remote_immutable_integrity_conflict(self):
        original = self.npm.publish
        def submit(path, channel):
            original(path, channel)
            self.registry.packages[publisher.NAMES[0]]["versions"][VERSION]["dist"]["integrity"] = "sha512-different"
            raise publisher.NpmFailure("E403", "access", "publish", str(path), 1)
        with patch.object(self.npm, "publish", side_effect=submit):
            with self.assertRaisesRegex(publisher.PublishError, "different integrity"):
                self.publish()
        self.assertEqual(self.waits, [])
        self.assertNotIn(PARENT, self.registry.packages)
    def test_successful_publish_with_lost_response_and_delayed_visibility_is_verified(self):
        original_publish = self.npm.publish
        def submit(path, channel):
            original_publish(path, channel)
            if path.name == f"{publisher.NAMES[0]}-{VERSION}.tgz":
                raise publisher.NpmFailure("ECONNRESET", "unknown", "publish", str(path), 1)
        original = self.registry.metadata
        def metadata(name, *, timeout=30):
            if name == publisher.NAMES[0] and sum(self.waits) < 300:
                return None
            return original(name, timeout=timeout)
        with patch.object(self.registry, "metadata", side_effect=metadata), patch.object(self.npm, "publish", side_effect=submit):
            result = self.publish()
        self.assertEqual(result["packages"][publisher.NAMES[0]], "verified-after-error")
        self.assertEqual(result["packages"][PARENT], "published")
        self.assertEqual(sum(self.waits), 300)
    def test_existing_version_with_wrong_channel_requires_explicit_promotion(self):
        metadata = self.fixture.metadata(PARENT)
        metadata["dist-tags"]["latest"] = "0.4.0"
        self.registry.packages[PARENT] = metadata
        with self.assertRaisesRegex(publisher.PublishError, "explicit promotion"):
            self.publish()
        self.assertEqual(self.npm.events, [("auth",)])
    def test_older_job_cannot_move_latest_backwards(self):
        self.registry.packages[PARENT] = {"name": PARENT, "versions": {}, "dist-tags": {"latest": "0.6.0"}}
        with self.assertRaisesRegex(publisher.PublishError, "backwards"):
            self.publish()
        self.assertEqual(self.npm.events, [("auth",)])
    def test_malformed_registry_shapes_fail_before_any_platform_write(self):
        for field, value in [("dist-tags", []), ("dist-tags", {"latest": 123}), ("versions", [])]:
            metadata = self.fixture.metadata(PARENT)
            metadata[field] = value
            self.registry.packages[PARENT] = metadata
            with self.subTest(field=field), self.assertRaises(publisher.PublishError):
                self.publish()
        self.assertEqual(self.npm.events, [("auth",)] * 3)
    def test_registry_http_only_404_is_absent_and_transport_failure_is_fatal(self):
        registry = publisher.Registry()
        for code in (401, 403, 500):
            error = publisher.urllib.error.HTTPError("https://example.invalid", code, "fixture", {}, None)
            with patch.object(publisher.urllib.request, "urlopen", side_effect=error):
                with self.subTest(code=code), self.assertRaisesRegex(publisher.PublishError, str(code)):
                    registry.metadata(PARENT)
        missing = publisher.urllib.error.HTTPError("https://example.invalid", 404, "fixture", {}, None)
        with patch.object(publisher.urllib.request, "urlopen", side_effect=missing):
            self.assertIsNone(registry.metadata(PARENT))
        with patch.object(publisher.urllib.request, "urlopen", side_effect=publisher.urllib.error.URLError("offline")):
            with self.assertRaisesRegex(publisher.PublishError, "unavailable"):
                registry.metadata(PARENT)
    def test_registry_failure_is_not_treated_as_an_available_name(self):
        with patch.object(self.registry, "metadata", side_effect=publisher.PublishError("HTTP 403")):
            with self.assertRaisesRegex(publisher.PublishError, "403"):
                self.publish()
        self.assertEqual(self.npm.events, [("auth",)])
    def test_remote_visibility_is_bounded_by_one_shared_twelve_minute_window(self):
        packs = {name: {"integrity": "sha512-fixture"} for name in publisher.NAMES[:-1]}
        window = publisher.VisibilityWindow(self.registry, VERSION, "latest", packs, self.waits.append)
        with self.assertRaisesRegex(publisher.PublishError, "not visible"):
            window.verify(publisher.NAMES[:-1], {})
        self.assertEqual(self.waits, list(publisher.VISIBILITY_DELAYS))
        self.assertEqual(sum(self.waits), 720)
    def test_default_auth_does_not_fall_back_to_oidc(self):
        with patch.object(publisher.prepare, "npm_command", return_value=["npm"]), patch.dict(os.environ, {}, clear=True):
            with self.assertRaisesRegex(publisher.PublishError, "NODE_AUTH_TOKEN"):
                publisher.Npm("token").preflight()
            with self.assertRaisesRegex(publisher.PublishError, "OIDC requires"):
                publisher.Npm("oidc").preflight()
    def test_oidc_auth_rejects_mixed_token(self):
        with patch.object(publisher.prepare, "npm_command", return_value=["npm"]), patch.dict(os.environ, {
            "NODE_AUTH_TOKEN": "synthetic-test-only",
            "ACTIONS_ID_TOKEN_REQUEST_URL": "https://example.invalid",
            "ACTIONS_ID_TOKEN_REQUEST_TOKEN": "synthetic-oidc-only"}, clear=True):
            with self.assertRaisesRegex(publisher.PublishError, "must not receive"):
                publisher.Npm("oidc").preflight()
    def test_npm_errors_expose_only_the_safe_error_code_and_never_credentials(self):
        with patch.object(publisher.prepare, "npm_command", return_value=["node", "npm-cli.js"]):
            npm = publisher.Npm("token")
        failure = publisher.subprocess.CompletedProcess(["npm"], 1, "synthetic-private-stdout",
            "npm error code E403\n_authToken=synthetic-private-token\n")
        with patch.object(publisher.subprocess, "run", return_value=failure):
            with self.assertRaises(publisher.PublishError) as caught:
                npm.publish(Path("candidate.tgz"), "latest")
        self.assertIn("E403", str(caught.exception))
        self.assertNotIn("synthetic-private", str(caught.exception))
    def test_ansi_and_json_npm_causes_are_fixed_categories_with_no_captured_output(self):
        cases = [
            ("E403", "Package name too similar to existing package", "name-too-similar"),
            ("E403", "Package name is reserved", "name-restricted"),
            ("E403", "Cannot republish after unpublish for 24 hours", "name-cooldown"),
            ("EOTP", "One-time password required", "2fa"),
            ("E_STAGE_REQUIRED", "Stage-only token", "stage-required"),
            ("E401", "Unauthorized", "access"),
            ("EPUBLISHCONFLICT", "Cannot publish over existing version", "immutable"),
            ("E500", "npm internal error", "internal"),
            ("ECONNRESET", "Connection lost", "unknown"),
        ]
        for code, summary, reason in cases:
            for use_json in (False, True):
                output = (json.dumps({"error": {"code": code, "summary": summary,
                    "detail": "Authorization: Bearer synthetic-secret-value"}}) if use_json else
                    f"\\x1b[31mnpm error code {code}\\x1b[0m\nnpm error {summary}\n_authToken=synthetic-secret-value".replace("\\x1b", "\x1b"))
                stdout, stderr = (output, "") if use_json else ("", output)
                with self.subTest(code=code, json=use_json):
                    actual = publisher.classify_npm(stdout, stderr)
                    self.assertEqual(actual, (code, reason))
                    failure = publisher.NpmFailure(*actual, "publish",
                        f"/private-user/credentials/{PARENT}-{VERSION}.tgz", 1)
                    self.assertNotIn("synthetic-secret", str(failure))
                    self.assertNotIn("private-user", str(failure))
                    self.assertNotIn(summary, str(failure))
                    self.assertIn(f"parent={PARENT}", str(failure))
                    self.assertIn(f"candidate={PARENT}-{VERSION}.tgz", str(failure))
        unknown = publisher.classify_npm('{"error":{"code":"synthetic-secret-value","message":"private"}}', "")
        self.assertEqual(unknown, ("UNKNOWN", "unknown"))
        failure = publisher.NpmFailure("synthetic-secret-value", "synthetic-secret-value",
                                      "private-command", "/private-user/synthetic-secret-value.tgz", 1)
        self.assertNotIn("synthetic-secret-value", str(failure))
        self.assertNotIn("private-user", str(failure))
    def test_subprocess_timeout_is_an_unknown_outcome_without_raw_exception_output(self):
        with patch.object(publisher.prepare, "npm_command", return_value=["node", "npm-cli.js"]):
            npm = publisher.Npm("token")
        timeout = publisher.subprocess.TimeoutExpired(
            ["synthetic-secret-command"], 180, output="synthetic-secret-value")
        with patch.object(publisher.subprocess, "run", side_effect=timeout):
            with self.assertRaises(publisher.NpmFailure) as caught:
                npm.publish(Path(f"{PARENT}-{VERSION}.tgz"), "latest")
        self.assertEqual(caught.exception.code, "ETIMEDOUT")
        self.assertFalse(caught.exception.definite)
        self.assertNotIn("synthetic-secret", str(caught.exception))
    def test_oidc_publication_cannot_fall_back_to_environment_or_configured_tokens(self):
        isolated = []
        environment = {
            "ACTIONS_ID_TOKEN_REQUEST_URL": "https://example.invalid/oidc",
            "ACTIONS_ID_TOKEN_REQUEST_TOKEN": "synthetic-oidc-only",
            "GITHUB_ACTIONS": "true", "GITHUB_SHA": SHA,
            "NPM_TOKEN": "synthetic-legacy-only",
            "NPM_AUTH_TOKEN": "synthetic-legacy-only",
            "NPM_CONFIG_USERCONFIG": "synthetic-user-config",
            "npm_config_globalconfig": "synthetic-global-config",
            "npm_config_//registry.npmjs.org/:_authToken": "synthetic-legacy-only",
        }
        def run(arguments, **options):
            actual = options["env"]
            self.assertEqual(actual["ACTIONS_ID_TOKEN_REQUEST_URL"], environment["ACTIONS_ID_TOKEN_REQUEST_URL"])
            self.assertEqual(actual["ACTIONS_ID_TOKEN_REQUEST_TOKEN"], environment["ACTIONS_ID_TOKEN_REQUEST_TOKEN"])
            self.assertEqual(actual["GITHUB_SHA"], SHA)
            self.assertTrue({"NODE_AUTH_TOKEN", "NPM_TOKEN", "NPM_AUTH_TOKEN"}.isdisjoint(actual))
            self.assertEqual({key.upper() for key in actual if key.upper().startswith("NPM_CONFIG_")},
                             {"NPM_CONFIG_USERCONFIG", "NPM_CONFIG_GLOBALCONFIG"})
            directory = Path(options["cwd"])
            isolated.append(directory)
            self.assertNotEqual(directory.resolve(), Path.cwd().resolve())
            self.assertFalse((directory / ".npmrc").exists())
            for name in ("NPM_CONFIG_USERCONFIG", "NPM_CONFIG_GLOBALCONFIG"):
                config = Path(actual[name])
                self.assertEqual(config.parent, directory)
                self.assertEqual(config.read_bytes(), b"")
            self.assertEqual(arguments[-2:], ["--registry", publisher.REGISTRY])
            return publisher.subprocess.CompletedProcess(arguments, 0, "", "")
        with patch.object(publisher.prepare, "npm_command", return_value=["node", "npm-cli.js"]), \
                patch.dict(os.environ, environment, clear=True), \
                patch.object(publisher.subprocess, "run", side_effect=run):
            npm = publisher.Npm("oidc")
            npm.preflight()
            npm.publish(Path("candidate.tgz"), "next")
        self.assertEqual(len(isolated), 1)
        self.assertFalse(isolated[0].exists())

    def test_token_publication_keeps_its_explicit_setup_node_authentication(self):
        def run(arguments, **options):
            self.assertNotIn("env", options)
            self.assertNotIn("cwd", options)
            self.assertEqual(os.environ["NODE_AUTH_TOKEN"], "synthetic-legacy-only")
            self.assertEqual(os.environ["NPM_CONFIG_USERCONFIG"], "synthetic-user-config")
            return publisher.subprocess.CompletedProcess(arguments, 0, "", "")
        with patch.object(publisher.prepare, "npm_command", return_value=["node", "npm-cli.js"]), \
                patch.dict(os.environ, {"NODE_AUTH_TOKEN": "synthetic-legacy-only",
                                        "NPM_CONFIG_USERCONFIG": "synthetic-user-config"}, clear=True), \
                patch.object(publisher.subprocess, "run", side_effect=run):
            npm = publisher.Npm("token")
            npm.preflight()
            npm.publish(Path("candidate.tgz"), "latest")

    def test_publish_command_is_explicit_public_tagged_and_scriptless(self):
        with patch.object(publisher.prepare, "npm_command", return_value=["node", "npm-cli.js"]):
            npm = publisher.Npm("token")
        with patch.object(npm, "run") as run:
            npm.publish(Path("candidate.tgz"), "next")
        self.assertEqual(run.call_args.args[0], ["publish", str(Path("candidate.tgz").resolve()),
                         "--access", "public", "--tag", "next", "--ignore-scripts"])


class WorkflowConsumerContracts(unittest.TestCase):
    def test_oidc_workflow_never_injects_the_bootstrap_publishing_credential(self):
        workflow = (ROOT / ".github/workflows/build-release.yml").read_text(encoding="utf-8")
        publish = workflow.split("\n  publish:", 1)[1]
        self.assertIn("XNIP_NPM_AUTH: ${{ vars.XNIP_NPM_AUTH || "
                      "(inputs.prerelease && 'token' || 'oidc') }}", publish)
        self.assertIn("NODE_AUTH_TOKEN: ${{ env.XNIP_NPM_AUTH == 'token' && "
                      "(secrets.NPM_TOKEN || secrets.NODE_AUTH_TOKEN) || '' }}", publish)
        self.assertIn('if [ "$XNIP_NPM_AUTH" = oidc ]; then unset NODE_AUTH_TOKEN; fi', publish)
        self.assertIn("id-token: write", publish)

    def test_delivery_timeout_covers_native_and_parent_visibility_windows(self):
        workflow = (ROOT / ".github/workflows/build-release.yml").read_text(encoding="utf-8")
        publish = workflow.split("\n  publish:", 1)[1]
        self.assertIn("timeout-minutes: 45", publish)
        self.assertEqual(sum(publisher.VISIBILITY_DELAYS), publisher.VISIBILITY_SECONDS)
    def test_ci_local_builds_omit_symbols_without_relaxing_assertions_or_public_payload_cap(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        environment = workflow.split("\nenv:\n", 1)[1].split("\njobs:", 1)[0]
        self.assertIn('CARGO_PROFILE_DEV_DEBUG: "0"', environment)
        self.assertNotIn("CARGO_PROFILE_DEV_DEBUG_ASSERTIONS", workflow)
        self.assertEqual(publisher.prepare.MAX_EXECUTABLE, 128 * 1024 * 1024)
    def test_six_native_install_jobs_pin_node_architecture_and_assert_host_identity(self):
        workflow = (ROOT / ".github/workflows/build-release.yml").read_text(encoding="utf-8")
        job = workflow.split("  npm-install:\n", 1)[1].split("\n  publish:", 1)[0]
        matrix = dict(publisher.re.findall(r"- platform: (\S+)\n\s+os: (\S+)", job))
        self.assertEqual(matrix, {
            "linux-x64": "ubuntu-latest", "linux-arm64": "ubuntu-24.04-arm",
            "darwin-x64": "macos-15-intel", "darwin-arm64": "macos-latest",
            "win32-x64": "windows-latest", "win32-arm64": "windows-11-arm",
        })
        self.assertIn("architecture: ${{ endsWith(matrix.platform, '-arm64') && 'arm64' || 'x64' }}", job)
        self.assertIn("XNIP_EXPECT_PLATFORM: ${{ matrix.platform }}", job)
        self.assertIn("python tests/npm_release_smoke.py", job)
    def test_harness_inline_installer_rebases_the_portable_report_path(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        step = workflow.split("- name: Install the actual private host package and official DSH host dependencies", 1)[1]
        source = step.split("python - <<'PY'\n", 1)[1].split("\n          PY", 1)[0]
        source = textwrap.dedent(source)
        calls = []
        with tempfile.TemporaryDirectory(prefix="xnip-workflow-consumer-") as temporary:
            directory = Path(temporary)
            def prepared(dist, output, pack, binary):
                archive = output / "tarballs" / f"{PARENT}-{VERSION}.tgz"
                archive.parent.mkdir(parents=True)
                archive.write_bytes(b"offline workflow artifact fixture")
                return {"output_directory": str(output),
                        "tarballs": [{"name": PARENT, "filename": "tarballs/" + archive.name}]}
            def run(arguments, **kwargs):
                calls.append(arguments)
                if "--offline" in arguments:
                    archive = Path(arguments[-1])
                    self.assertTrue(archive.is_absolute())
                    self.assertTrue(archive.is_file(), "relative report filename must resolve under its preparation root")
                    self.assertEqual(archive.read_bytes(), b"offline workflow artifact fixture")
                return publisher.subprocess.CompletedProcess(arguments, 0)
            with patch.dict(os.environ, {"RUNNER_TEMP": str(directory), "GITHUB_ENV": str(directory / "github-env")}), \
                    patch.object(publisher.prepare, "prepare", side_effect=prepared), \
                    patch.object(publisher.prepare, "npm_command", return_value=["node", "npm-cli.js"]), \
                    patch.object(publisher.subprocess, "run", side_effect=run):
                exec(compile(source, ".github/workflows/ci.yml:harness-installer", "exec"), {})
            self.assertEqual(len(calls), 2)
            self.assertIn("--offline", calls[0])
            self.assertIn("@deepseek-ai/dsh-mcp-client@0.2.1-rc.2", calls[1])
            environment = (directory / "github-env").read_text(encoding="utf-8")
            self.assertIn("XNIP_DSH_TEST_INSTALL=", environment)
            pi_package = next(line.split("=", 1)[1] for line in environment.splitlines()
                              if line.startswith("XNIP_PI_TEST_PACKAGE="))
            self.assertEqual(Path(pi_package).name, PARENT)
            self.assertEqual(Path(pi_package).parent.name, "node_modules")
            self.assertEqual(Path(calls[0][-1]).name, f"{PARENT}-{VERSION}.tgz")


if __name__ == "__main__":
    unittest.main()
