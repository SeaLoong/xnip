#!/usr/bin/env python3
"""Offline contracts for verified, draft-first and retry-safe GitHub publication."""

from __future__ import annotations

import copy
import gzip
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "xnip_publish_release", ROOT / ".github" / "scripts" / "publish_release.py",
)
assert SPEC is not None and SPEC.loader is not None
publisher = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = publisher
SPEC.loader.exec_module(publisher)
SHA = "a" * 40
OTHER_SHA = "b" * 40
STABLE = "v0.2.0"
PRE = "v0.2.1-dev.42.gaaaaaaaaaaaa"


def record(tag=STABLE, sha=SHA, *, draft=True, prerelease=False):
    return {
        "id": 17, "tag_name": tag, "target_commitish": sha,
        "draft": draft, "prerelease": prerelease,
    }


def remote_assets(local):
    return [
        {
            "id": index, "name": name, "state": "uploaded",
            "size": asset.size, "digest": "sha256:" + asset.digest,
        }
        for index, (name, asset) in enumerate(sorted(local.items()), 1)
    ]


class FakeGitHub:
    def __init__(self, local, *, release=None, commit=None):
        self.local = local
        self.current = copy.deepcopy(release)
        self.commit = commit
        self.remote = remote_assets(local) if release and not release["draft"] else []
        self.payloads = {item["id"]: local[item["name"]].path.read_bytes() for item in remote_assets(local)}
        self.events = []
        self.upload_error = None
        self.after_upload = None
        self.downloads = 0

    def tag_commit(self, tag):
        self.events.append(("tag", tag))
        return self.commit

    def find_release(self, tag):
        self.events.append(("find", tag))
        return copy.deepcopy(self.current)

    def release(self, release_id):
        self.events.append(("get", release_id))
        return copy.deepcopy(self.current)

    def assets(self, release_id):
        self.events.append(("assets", release_id))
        return copy.deepcopy(self.remote)

    def asset_bytes(self, asset_id):
        self.events.append(("download", asset_id))
        self.downloads += 1
        return self.payloads[asset_id]

    def create_draft(self, tag, sha, prerelease):
        self.events.append(("create-draft", tag, sha, prerelease))
        self.current = record(tag, sha, draft=True, prerelease=prerelease)

    def upload(self, release_id, tag, assets):
        self.events.append(("upload", release_id, tag))
        if self.upload_error:
            raise publisher.PublishError(self.upload_error)
        self.remote = copy.deepcopy(self.after_upload) if self.after_upload is not None else remote_assets(assets)
        self.payloads = {item["id"]: assets[item["name"]].path.read_bytes() for item in remote_assets(assets)}

    def publish(self, release_id, sha, prerelease):
        self.events.append(("publish", release_id, sha, prerelease))
        self.current["draft"] = False
        self.commit = sha
        return copy.deepcopy(self.current)


class Fixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="xnip-publish-contract-")
        self.addCleanup(self.temp.cleanup)
        self.dist = Path(self.temp.name) / "dist"
        self.dist.mkdir()

    def prepare(self, tag=STABLE):
        for name in publisher.expected_names(tag):
            (self.dist / name).write_bytes(("fixture:" + name + "\n").encode("ascii"))
        for name in publisher.archive_names(tag):
            digest = hashlib.sha256((self.dist / name).read_bytes()).hexdigest()
            (self.dist / (name + ".sha256")).write_text(f"{digest}  {name}\n", encoding="ascii")
        return publisher.validate_dist(tag, self.dist)

    def assert_no_mutation(self, fake):
        self.assertFalse(any(event[0] in {"create-draft", "upload", "publish"} for event in fake.events))


class Distribution(Fixture):
    def test_expected_set_matches_all_seven_real_integration_packagers(self):
        outputs = set()
        for script in (ROOT / "integrations").glob("*/package.sh"):
            match = re.search(r"\$\{OUT_DIR\}/([^\"\r\n]+)", script.read_text(encoding="utf-8"))
            self.assertIsNotNone(match, script)
            outputs.add(match[1])
        self.assertEqual(outputs, set(publisher.INTEGRATIONS))
        self.assertEqual(len(publisher.expected_names(STABLE)), 21)
        self.assertEqual(sum(name.endswith(".zip") for name in publisher.archive_names(STABLE)), 2)

    def test_distribution_checks_all_six_archive_checksums_and_accepts_binary_marker(self):
        local = self.prepare()
        self.assertEqual(len(local), 21)
        self.assertEqual(set(local), publisher.expected_names(STABLE))

    def test_missing_extra_empty_and_directory_assets_fail_locally(self):
        for kind in ("missing", "extra", "empty", "directory"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as folder:
                dist = Path(folder)
                for name, asset in self.prepare().items():
                    (dist / name).write_bytes(asset.path.read_bytes())
                if kind == "missing":
                    (dist / "install.sh").unlink()
                elif kind == "extra":
                    (dist / "unexpected").write_text("extra", encoding="utf-8")
                elif kind == "empty":
                    (dist / "install.sh").write_bytes(b"")
                else:
                    (dist / "install.sh").unlink()
                    (dist / "install.sh").mkdir()
                with self.assertRaises(publisher.PublishError):
                    publisher.validate_dist(STABLE, dist)

    def test_symbolic_link_is_not_a_release_asset(self):
        self.prepare()
        target = self.dist / "install.sh"
        target.unlink()
        try:
            target.symlink_to(self.dist / "install.ps1")
        except OSError as error:
            self.skipTest(f"symlink creation is unavailable: {error}")
        with self.assertRaisesRegex(publisher.PublishError, "regular file"):
            publisher.validate_dist(STABLE, self.dist)

    def test_bad_checksum_wrong_filename_and_multiple_lines_are_rejected(self):
        for kind in ("digest", "filename", "multiple", "non-ascii"):
            with self.subTest(kind=kind):
                self.prepare()
                name = publisher.archive_names(STABLE)[0]
                checksum = self.dist / (name + ".sha256")
                digest = publisher.file_digest(self.dist / name)
                text = {
                    "digest": f"{'0' * 64}  {name}\n",
                    "filename": f"{digest}  ../{name}\n",
                    "multiple": f"{digest}  {name}\n{digest}  {name}\n",
                    "non-ascii": "汉字",
                }[kind]
                checksum.write_text(text, encoding="utf-8")
                with self.assertRaises(publisher.PublishError):
                    publisher.validate_dist(STABLE, self.dist)

    def test_invalid_identity_is_rejected_before_any_remote_call(self):
        fake = FakeGitHub({})
        for tag, sha, pre in [
            ("v01.2.3", SHA, False), ("1.2.3", SHA, False),
            ("v1.2.3-01", SHA, True), ("v1.2.3;touch nope", SHA, False),
            (STABLE, "abc123", False), (STABLE, "-x" * 20, False),
            (PRE, SHA, False), (STABLE, SHA, True),
        ]:
            with self.subTest(tag=tag, sha=sha), self.assertRaises(publisher.PublishError):
                publisher.publish(tag, sha, self.dist, pre, fake)
        self.assertEqual(fake.events, [])

    def test_incomplete_local_distribution_never_queries_github(self):
        fake = FakeGitHub({})
        with self.assertRaises(publisher.PublishError):
            publisher.publish(STABLE, SHA, self.dist, gh=fake)
        self.assertEqual(fake.events, [])


class Publication(Fixture):
    def test_new_prerelease_stays_draft_until_every_uploaded_byte_is_verified(self):
        fake = FakeGitHub(self.prepare(PRE))
        result = publisher.publish(PRE, SHA.upper(), self.dist, True, fake)
        self.assertEqual(result["status"], "published")
        self.assertEqual(result["assets"], 21)
        self.assertTrue(result["prerelease"])
        actions = [event[0] for event in fake.events]
        self.assertLess(actions.index("create-draft"), actions.index("upload"))
        self.assertLess(actions.index("upload"), actions.index("publish"))
        self.assertIn("assets", actions[actions.index("upload") + 1:actions.index("publish")])
        self.assertFalse(fake.current["draft"])
        self.assertEqual(fake.commit, SHA)

    def test_stable_draft_retry_can_replace_partial_assets_then_publish(self):
        local = self.prepare()
        fake = FakeGitHub(local, release=record(), commit=SHA)
        fake.remote = remote_assets(local)[:4]
        result = publisher.publish(STABLE, SHA, self.dist, gh=fake)
        self.assertEqual(result["status"], "published")
        self.assertFalse(any(event[0] == "create-draft" for event in fake.events))

    def test_complete_published_release_is_read_only_and_idempotently_skipped(self):
        for tag, pre in ((STABLE, False), (PRE, True)):
            with self.subTest(tag=tag), tempfile.TemporaryDirectory() as folder:
                self.dist = Path(folder)
                local = self.prepare(tag)
                fake = FakeGitHub(local, release=record(tag, draft=False, prerelease=pre), commit=SHA)
                self.assertEqual(publisher.publish(tag, SHA, self.dist, pre, fake)["status"], "skipped")
                self.assert_no_mutation(fake)

    def test_published_asset_mismatch_or_mode_mismatch_never_uploads(self):
        for kind in ("missing", "digest", "size", "state", "duplicate", "mode", "target", "tag"):
            with self.subTest(kind=kind):
                local = self.prepare()
                fake = FakeGitHub(local, release=record(draft=False), commit=SHA)
                if kind == "missing":
                    fake.remote.pop()
                elif kind == "digest":
                    archive = next(item for item in fake.remote if item["name"] == publisher.archive_names(STABLE)[0])
                    archive["digest"] = "sha256:" + "0" * 64
                elif kind == "size":
                    sidecar = next(item for item in fake.remote if item["name"].endswith(".sha256"))
                    sidecar["size"] += 1
                elif kind == "state":
                    fake.remote[0]["state"] = "starter"
                elif kind == "duplicate":
                    fake.remote[0] = copy.deepcopy(fake.remote[1])
                elif kind == "mode":
                    fake.current["prerelease"] = True
                elif kind == "target":
                    fake.current["target_commitish"] = OTHER_SHA
                else:
                    fake.current["tag_name"] = PRE
                with self.assertRaises(publisher.PublishError):
                    publisher.publish(STABLE, SHA, self.dist, gh=fake)
                self.assert_no_mutation(fake)

    def test_old_assets_without_api_digest_are_downloaded_and_hashed(self):
        local = self.prepare()
        fake = FakeGitHub(local, release=record(draft=False), commit=SHA)
        for item in fake.remote:
            item["digest"] = None
        result = publisher.publish(STABLE, SHA, self.dist, gh=fake)
        self.assertEqual(result["status"], "skipped")
        self.assertEqual(fake.downloads, 12)
        self.assert_no_mutation(fake)

    def test_wrong_existing_tag_fails_before_creation_or_draft_upload(self):
        for release in (None, record(), record(draft=False)):
            with self.subTest(release=release):
                fake = FakeGitHub(self.prepare(), release=release, commit=OTHER_SHA)
                with self.assertRaisesRegex(publisher.PublishError, "points to"):
                    publisher.publish(STABLE, SHA, self.dist, gh=fake)
                self.assert_no_mutation(fake)

    def test_published_release_requires_a_resolvable_tag(self):
        fake = FakeGitHub(self.prepare(), release=record(draft=False))
        with self.assertRaisesRegex(publisher.PublishError, "tag is missing"):
            publisher.publish(STABLE, SHA, self.dist, gh=fake)
        self.assert_no_mutation(fake)

    def test_draft_from_another_commit_or_with_unexpected_assets_is_not_modified(self):
        for kind in ("target", "unexpected", "duplicate"):
            with self.subTest(kind=kind):
                fake = FakeGitHub(self.prepare(), release=record(), commit=SHA)
                if kind == "target":
                    fake.current["target_commitish"] = OTHER_SHA
                else:
                    fake.remote = remote_assets(fake.local)[:1]
                    if kind == "unexpected":
                        fake.remote[0]["name"] = "someone-elses-asset.zip"
                    else:
                        fake.remote.append(copy.deepcopy(fake.remote[0]))
                with self.assertRaises(publisher.PublishError):
                    publisher.publish(STABLE, SHA, self.dist, gh=fake)
                self.assert_no_mutation(fake)

    def test_failed_or_incomplete_upload_never_publishes(self):
        for kind in ("failure", "missing", "digest"):
            with self.subTest(kind=kind):
                local = self.prepare()
                fake = FakeGitHub(local, release=record(), commit=SHA)
                if kind == "failure":
                    fake.upload_error = "network unavailable"
                else:
                    fake.after_upload = remote_assets(local)
                    if kind == "missing":
                        fake.after_upload.pop()
                    else:
                        fake.after_upload[0]["digest"] = "sha256:" + "0" * 64
                with self.assertRaises(publisher.PublishError):
                    publisher.publish(STABLE, SHA, self.dist, gh=fake)
                self.assertFalse(any(event[0] == "publish" for event in fake.events))
                self.assertTrue(fake.current["draft"])

    def test_published_rerun_accepts_real_archives_rebuilt_with_different_timestamps(self):
        def rebuild(timestamp):
            for name in publisher.archive_names(STABLE):
                buffer = io.BytesIO()
                if name.endswith(".zip"):
                    with zipfile.ZipFile(buffer, "w") as archive:
                        info = zipfile.ZipInfo("xnip.exe", (2026, 10, 18, 12, 0, timestamp))
                        archive.writestr(info, b"same compiled fixture")
                else:
                    with gzip.GzipFile(fileobj=buffer, mode="wb", mtime=timestamp) as zipped:
                        with tarfile.open(fileobj=zipped, mode="w") as archive:
                            info = tarfile.TarInfo("xnip")
                            info.size = len(b"same compiled fixture")
                            info.mtime = timestamp
                            archive.addfile(info, io.BytesIO(b"same compiled fixture"))
                (self.dist / name).write_bytes(buffer.getvalue())
                digest = hashlib.sha256(buffer.getvalue()).hexdigest()
                (self.dist / (name + ".sha256")).write_text(f"{digest}  {name}\n", encoding="ascii")
            return publisher.validate_dist(STABLE, self.dist)

        self.prepare()
        old = rebuild(2)
        fake = FakeGitHub(old, release=record(draft=False), commit=SHA)
        fresh = rebuild(4)
        self.assertTrue(all(old[name].digest != fresh[name].digest for name in publisher.archive_names(STABLE)))
        self.assertEqual(publisher.publish(STABLE, SHA, self.dist, gh=fake)["status"], "skipped")
        self.assert_no_mutation(fake)

    def test_published_sidecar_must_match_the_published_archive_and_filename(self):
        for kind in ("digest", "filename", "multiple-lines"):
            with self.subTest(kind=kind):
                fake = FakeGitHub(self.prepare(), release=record(draft=False), commit=SHA)
                archive = publisher.archive_names(STABLE)[0]
                sidecar = next(item for item in fake.remote if item["name"] == archive + ".sha256")
                digest = publisher.file_digest(self.dist / archive)
                text = {
                    "digest": f"{'0' * 64}  {archive}\n",
                    "filename": f"{digest}  ../{archive}\n",
                    "multiple-lines": f"{digest}  {archive}\n{digest}  {archive}\n",
                }[kind].encode("ascii")
                fake.payloads[sidecar["id"]] = text
                sidecar["size"] = len(text)
                sidecar["digest"] = "sha256:" + hashlib.sha256(text).hexdigest()
                with self.assertRaises(publisher.PublishError):
                    publisher.publish(STABLE, SHA, self.dist, gh=fake)
                self.assert_no_mutation(fake)

    def test_published_verification_bounds_downloads_and_checks_actual_lengths(self):
        for kind in ("huge-sidecar", "huge-archive", "sidecar-length", "archive-length", "fallback-hash"):
            with self.subTest(kind=kind):
                fake = FakeGitHub(self.prepare(), release=record(draft=False), commit=SHA)
                name = publisher.archive_names(STABLE)[0]
                archive = next(item for item in fake.remote if item["name"] == name)
                sidecar = next(item for item in fake.remote if item["name"] == name + ".sha256")
                if kind == "huge-sidecar":
                    sidecar["size"] = publisher.MAX_CHECKSUM_BYTES + 1
                elif kind == "huge-archive":
                    archive["size"] = publisher.MAX_ASSET_BYTES + 1
                elif kind == "sidecar-length":
                    fake.payloads[sidecar["id"]] += b"extra"
                else:
                    archive["digest"] = None
                    original = fake.payloads[archive["id"]]
                    fake.payloads[archive["id"]] = original + b"extra" if kind == "archive-length" else b"X" + original[1:]
                with self.assertRaises(publisher.PublishError):
                    publisher.publish(STABLE, SHA, self.dist, gh=fake)
                self.assert_no_mutation(fake)
                if kind in {"huge-sidecar", "huge-archive"}:
                    self.assertEqual(fake.downloads, 0)

    def test_tag_changed_during_upload_prevents_publication(self):
        fake = FakeGitHub(self.prepare(), release=record(), commit=SHA)
        original = fake.upload

        def upload(*args):
            original(*args)
            fake.commit = OTHER_SHA

        fake.upload = upload
        with self.assertRaisesRegex(publisher.PublishError, "points to"):
            publisher.publish(STABLE, SHA, self.dist, gh=fake)
        self.assertFalse(any(event[0] == "publish" for event in fake.events))

    def test_other_publisher_changing_draft_state_prevents_publication(self):
        fake = FakeGitHub(self.prepare(), release=record(), commit=SHA)
        original = fake.upload

        def upload(*args):
            original(*args)
            fake.current["draft"] = False

        fake.upload = upload
        with self.assertRaisesRegex(publisher.PublishError, "another publisher"):
            publisher.publish(STABLE, SHA, self.dist, gh=fake)
        self.assertFalse(any(event[0] == "publish" for event in fake.events))


def response(value=None, *, status=200, code=0, stderr=b""):
    body = json.dumps(value).encode("utf-8")
    return subprocess.CompletedProcess(
        [], code, f"HTTP/2.0 {status} status\r\nContent-Type: application/json\r\n\r\n".encode("ascii") + body, stderr,
    )



class ReleaseNotes(unittest.TestCase):
    def test_stable_notes_include_only_the_matching_version_section(self):
        with tempfile.TemporaryDirectory() as folder:
            changelog = Path(folder) / "CHANGELOG.md"
            changelog.write_text(
                "# Changelog\n\n## [Unreleased]\n\n- Future.\n\n"
                "## [0.3.0] - 2026-10-18\n\n### Compatibility\n\n- Five tools.\n"
                "\n## [0.2.0] - 2026-10-16\n\n- Previous.\n",
                encoding="utf-8",
            )
            notes = publisher.release_notes("v0.3.0", SHA, False, changelog)
            self.assertIn("### Compatibility\n\n- Five tools.", notes)
            self.assertIn(SHA, notes)
            self.assertNotIn("Future.", notes)
            self.assertNotIn("Previous.", notes)

    def test_stable_notes_require_a_nonempty_matching_entry(self):
        with tempfile.TemporaryDirectory() as folder:
            changelog = Path(folder) / "CHANGELOG.md"
            for text in ["", "## [0.2.0]\n\n- Old.", "## [0.3.0]\n\n## [0.2.0]\n- Old."]:
                changelog.write_text(text, encoding="utf-8")
                with self.assertRaises(publisher.PublishError):
                    publisher.release_notes("v0.3.0", SHA, False, changelog)
            self.assertIn("Development prerelease", publisher.release_notes(PRE, SHA, True, changelog))

class Adapter(unittest.TestCase):
    def test_only_explicit_http_404_is_missing(self):
        for result, missing in [
            (response(status=404, code=1, stderr=b"gh: Not Found (HTTP 404)"), True),
            (response(status=401, code=1, stderr=b"gh: Bad credentials (HTTP 401)"), False),
            (response(status=403, code=1, stderr=b"rate limit (HTTP 403)"), False),
            (subprocess.CompletedProcess([], 1, b"", b"dial tcp: network unavailable"), False),
            (subprocess.CompletedProcess([], 0, b"null", b""), False),
        ]:
            with self.subTest(missing=missing):
                github = publisher.GitHub("owner/repo", runner=lambda *args, **kwargs: result)
                if missing:
                    self.assertIsNone(github.api("repos/owner/repo/releases/tags/v1.2.3", missing=True))
                else:
                    with self.assertRaises(publisher.PublishError):
                        github.api("repos/owner/repo/releases/tags/v1.2.3", missing=True)

    def test_annotated_tags_are_peeled_to_the_commit_not_the_tag_object(self):
        calls = []
        tag1, tag2 = "c" * 40, "d" * 40
        replies = iter([
            response({"object": {"type": "tag", "sha": tag1}}),
            response({"object": {"type": "tag", "sha": tag2}}),
            response({"object": {"type": "commit", "sha": SHA}}),
        ])

        def runner(args, **kwargs):
            calls.append(args)
            return next(replies)

        github = publisher.GitHub("owner/repo", runner=runner)
        self.assertEqual(github.tag_commit(STABLE), SHA)
        self.assertEqual(calls[-1][-1], f"repos/owner/repo/git/tags/{tag2}")

    def test_cyclic_annotated_tag_is_rejected(self):
        tag_sha = "c" * 40
        github = publisher.GitHub(
            "owner/repo",
            runner=lambda *args, **kwargs: response({"object": {"type": "tag", "sha": tag_sha}}),
        )
        with self.assertRaisesRegex(publisher.PublishError, "unambiguously"):
            github.tag_commit(STABLE)

    def test_draft_is_found_in_authenticated_list_after_published_tag_endpoint_404(self):
        replies = iter([response(status=404, code=1), response([record()]), response(record())])
        github = publisher.GitHub("owner/repo", runner=lambda *args, **kwargs: next(replies))
        self.assertTrue(github.find_release(STABLE)["draft"])

    def test_prerelease_creation_is_draft_never_latest_and_uses_real_notes_file(self):
        captured = []

        def runner(args, **kwargs):
            notes = Path(args[args.index("--notes-file") + 1]).read_text(encoding="utf-8")
            captured.append((args, notes))
            return subprocess.CompletedProcess(args, 0, b"https://example.invalid/release\n", b"")

        github = publisher.GitHub("owner/repo", runner=runner)
        github.create_draft(PRE, SHA, True)
        args, notes = captured[0]
        for flag in ("--draft", "--prerelease", "--latest=false"):
            self.assertIn(flag, args)
        self.assertEqual(args[args.index("--target") + 1], SHA)
        self.assertIn("\n\n", notes)
        self.assertNotIn("\\n", notes)
        self.assertIn(SHA, notes)
        self.assertNotIn("--notes", args)

    def test_final_publication_payload_has_real_booleans_and_latest_only_for_stable(self):
        for pre in (True, False):
            with self.subTest(prerelease=pre):
                captured = []

                def runner(args, **kwargs):
                    payload = json.loads(Path(args[args.index("--input") + 1]).read_text(encoding="utf-8"))
                    captured.append((args, payload))
                    return response(record(PRE if pre else STABLE, draft=False, prerelease=pre))

                github = publisher.GitHub("owner/repo", runner=runner)
                github.publish(17, SHA, pre)
                args, payload = captured[0]
                self.assertIn("PATCH", args)
                self.assertIs(payload["draft"], False)
                self.assertIs(payload["prerelease"], pre)
                self.assertEqual(payload["target_commitish"], SHA)
                self.assertEqual(payload["make_latest"], "false" if pre else "true")

    def test_published_release_is_never_clobbered_even_if_state_changed(self):
        calls = []

        def runner(args, **kwargs):
            calls.append(args)
            return response(record(draft=False))

        github = publisher.GitHub("owner/repo", runner=runner)
        with self.assertRaisesRegex(publisher.PublishError, "published release"):
            github.upload(17, STABLE, {})
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][1], "api")

    def test_draft_upload_uses_explicit_paths_and_clobber_only_after_fresh_draft_check(self):
        calls = []

        def runner(args, **kwargs):
            calls.append(args)
            return response(record()) if args[1] == "api" else subprocess.CompletedProcess(args, 0, b"", b"")

        with tempfile.TemporaryDirectory(prefix="xnip-upload-contract-") as folder:
            asset = publisher.Asset(Path(folder) / "install.sh", 4, "a" * 64)
            publisher.GitHub("owner/repo", runner=runner).upload(17, STABLE, {"install.sh": asset})
        self.assertEqual(calls[0][1], "api")
        self.assertEqual(calls[1][1:4], ["release", "upload", STABLE])
        self.assertIn("--clobber", calls[1])
        self.assertEqual(calls[1][-1], str(asset.path))

    def test_read_api_failure_does_not_get_reclassified_as_absent_release(self):
        calls = []

        def runner(args, **kwargs):
            calls.append(args)
            return subprocess.CompletedProcess(args, 1, b"", b"TLS handshake timeout")

        github = publisher.GitHub("owner/repo", runner=runner)
        with self.assertRaisesRegex(publisher.PublishError, "TLS handshake"):
            github.find_release(STABLE)
        self.assertEqual(len(calls), 1)

    def test_binary_verification_never_executes_or_decodes_asset_payloads(self):
        payload = b"#!/bin/sh\nexit 97\n\x00\xff"
        calls = []

        def runner(args, **kwargs):
            calls.append(args)
            return subprocess.CompletedProcess(args, 0, payload, b"")

        github = publisher.GitHub("owner/repo", runner=runner)
        self.assertEqual(github.asset_bytes(42), payload)
        self.assertEqual(calls[0][1], "api")
        self.assertIn("Accept: application/octet-stream", calls[0])


if __name__ == "__main__":
    unittest.main()
