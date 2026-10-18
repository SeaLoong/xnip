"""Offline installer contracts; no real network requests or release downloads."""
import hashlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest

INSTALLER = Path(__file__).resolve().parents[1] / "install.sh"


@unittest.skipUnless(os.name == "posix" and shutil.which("sh"), "requires a POSIX shell")
class InstallerContract(unittest.TestCase):
    def exercise(self, version="latest", system="Linux", arch="x86_64",
                 repository="SeaLoong/xnip", corrupt=False, download_failure=False,
                 expected_target="x86_64-unknown-linux-musl", expected_error=None):
        with tempfile.TemporaryDirectory(prefix="xnip-installer-contract-") as temporary:
            root = Path(temporary)
            commands = root / "commands"
            commands.mkdir()
            binary = b"#!/bin/sh\nprintf 'xnip fixture\\n'\n"
            archive = root / "release.tar.gz"
            with tarfile.open(archive, "w:gz") as stream:
                info = tarfile.TarInfo("xnip")
                info.size, info.mode = len(binary), 0o755
                stream.addfile(info, io.BytesIO(binary))
            checksum = "0" * 64 if corrupt else hashlib.sha256(archive.read_bytes()).hexdigest()
            (root / "release.sha256").write_text(checksum + "  release.tar.gz\n")
            scripts = {
                "uname": '#!/bin/sh\ncase "$1" in -s) printf "%s" "$TEST_OS";; -m) printf "%s" "$TEST_ARCH";; esac\n',
                "curl": """#!/bin/sh
out=; latest=false
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o) out="$2"; shift 2 ;;
        -w) latest=true; shift 2 ;;
        https:*) url="$1"; shift ;;
        *) shift ;;
    esac
done
printf '%s\\n' "$url" >> "$FIXTURE/requests"
if [ "$latest" = true ]; then
    printf 'https://github.com/%s/releases/tag/v0.2.0' "$TEST_REPO"
    exit 0
fi
[ "$TEST_DOWNLOAD_FAILURE" = false ] || exit 22
case "$url" in
    */v0.2.0/xnip-v0.2.0-*.tar.gz.sha256) cp "$FIXTURE/release.sha256" "$out" ;;
    */v0.2.0/xnip-v0.2.0-*.tar.gz) cp "$FIXTURE/release.tar.gz" "$out" ;;
    *) echo "unexpected URL $url" >&2; exit 23 ;;
esac
""",
            }
            for name, script in scripts.items():
                path = commands / name
                path.write_text(script, newline="\n")
                path.chmod(0o755)
            destination = root / "installation with spaces"
            destination.mkdir()
            installed = destination / "xnip"
            installed.write_bytes(b"original installation")
            env = dict(os.environ, PATH=str(commands) + os.pathsep + os.environ["PATH"],
                       FIXTURE=str(root), TEST_OS=system, TEST_ARCH=arch, TEST_REPO=repository,
                       TEST_DOWNLOAD_FAILURE=str(download_failure).lower(),
                       XNIP_REPO=repository, XNIP_VERSION=version, XNIP_INSTALL_DIR=str(destination))
            result = subprocess.run(["sh", str(INSTALLER)], env=env, capture_output=True,
                                    text=True, timeout=20)
            failed = corrupt or download_failure or expected_error is not None
            self.assertEqual(result.returncode != 0, failed, result.stdout + result.stderr)
            if failed:
                self.assertEqual(installed.read_bytes(), b"original installation")
                if expected_error:
                    self.assertIn(expected_error, result.stderr)
            else:
                self.assertEqual(installed.read_bytes(), binary)
                self.assertTrue(os.access(installed, os.X_OK))
                requests = (root / "requests").read_text()
                self.assertIn(f"https://github.com/{repository}/releases/download/v0.2.0/"
                              f"xnip-v0.2.0-{expected_target}.tar.gz", requests)
                self.assertIn("xnip fixture", result.stdout)
            self.assertEqual(sorted(path.name for path in destination.iterdir()), ["xnip"])

    def test_release_selection_and_repository_override(self):
        for version in ["latest", "v0.2.0", "0.2.0"]:
            with self.subTest(version=version):
                self.exercise(version=version, repository="fixture/project")

    def test_supported_platforms_select_their_published_archive(self):
        for system, arch, target in [
            ("Linux", "x86_64", "x86_64-unknown-linux-musl"),
            ("Linux", "aarch64", "aarch64-unknown-linux-musl"),
            ("Linux", "arm64", "aarch64-unknown-linux-musl"),
            ("Darwin", "x86_64", "x86_64-apple-darwin"),
            ("Darwin", "arm64", "aarch64-apple-darwin"),
        ]:
            with self.subTest(system=system, arch=arch):
                self.exercise(system=system, arch=arch, expected_target=target)

    def test_checksum_and_download_errors_preserve_the_existing_installation(self):
        self.exercise(corrupt=True, expected_error="SHA256 mismatch")
        self.exercise(download_failure=True)

    def test_unsupported_platform_and_invalid_tags_fail_before_installation(self):
        self.exercise(system="Unsupported", expected_error="unsupported platform")
        self.exercise(version="not-a-tag", expected_error="cannot resolve release tag")
        self.exercise(version="v0.2.0/escape", expected_error="Invalid release tag")


if __name__ == "__main__":
    unittest.main()
