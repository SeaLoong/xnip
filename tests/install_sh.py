"""Offline installer contract tests. Run with Python 3 on Linux/macOS."""
import hashlib
import io
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

INSTALLER = Path(__file__).resolve().parents[1] / "install.sh"


class InstallerTests(unittest.TestCase):
    def run_install(self, version="latest", corrupt=False):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            bin_dir = root / "bin"
            bin_dir.mkdir()
            uname = bin_dir / "uname"
            uname.write_text('#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo x86_64;; esac\n', newline="\n")
            uname.chmod(0o755)
            archive = root / "fixture.tar.gz"
            binary = b"#!/bin/sh\necho 'xnip fixture'\n"
            with tarfile.open(archive, "w:gz") as tar:
                info = tarfile.TarInfo("xnip")
                info.size, info.mode = len(binary), 0o755
                tar.addfile(info, io.BytesIO(binary))
            digest = "0" * 64 if corrupt else hashlib.sha256(archive.read_bytes()).hexdigest()
            (root / "fixture.sha256").write_text(digest + "  fixture.tar.gz\n")
            curl = bin_dir / "curl"
            curl.write_text('''#!/bin/sh
out=; url=; latest=false
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) out="$2"; shift 2 ;;
    -w) latest=true; shift 2 ;;
    https:*) url="$1"; shift ;;
    *) shift ;;
  esac
done
if [ "$latest" = true ]; then
  printf 'https://github.com/SeaLoong/xnip/releases/tag/v0.1.0'
  exit 0
fi
case "$url" in
  */v0.1.0/xnip-v0.1.0-*.tar.gz.sha256) cp "$FIXTURE/fixture.sha256" "$out" ;;
  */v0.1.0/xnip-v0.1.0-*.tar.gz) cp "$FIXTURE/fixture.tar.gz" "$out" ;;
  *) echo "Unexpected URL: $url" >&2; exit 9 ;;
esac
''', newline="\n")
            curl.chmod(0o755)
            destination = root / "install space"
            destination.mkdir()
            (destination / "xnip").write_text("original")
            env = dict(os.environ, PATH=str(bin_dir) + os.pathsep + os.environ["PATH"],
                       FIXTURE=root.as_posix(), XNIP_INSTALL_DIR=destination.as_posix(), XNIP_VERSION=version)
            result = subprocess.run(["sh", str(INSTALLER)], env=env, capture_output=True, text=True)
            if corrupt:
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("SHA256 mismatch", result.stderr)
                self.assertEqual((destination / "xnip").read_text(), "original")
            else:
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual((destination / "xnip").read_bytes(), binary)

    def test_latest(self):
        self.run_install()

    def test_version_with_prefix(self):
        self.run_install("v0.1.0")

    def test_version_without_prefix(self):
        self.run_install("0.1.0")

    def test_checksum_failure_preserves_existing_installation(self):
        self.run_install(corrupt=True)


if __name__ == "__main__":
    unittest.main()
