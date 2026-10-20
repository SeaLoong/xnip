"""Distribution input validation; no registry, foreign execution or publication."""
from __future__ import annotations
import importlib.util
import io
import json
import shutil
import subprocess
from unittest import mock
from pathlib import Path
import struct
import tarfile
import tempfile
import tomllib
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("npm_prepare", ROOT/"npm/prepare.py")
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)

def elf(*, interpreter=False, arch=62):
    data = bytearray(128)
    data[:6] = b"\x7fELF\x02\x01"
    struct.pack_into("<H", data, 18, arch)
    struct.pack_into("<Q", data, 32, 64)
    struct.pack_into("<HH", data, 54, 56, 1)
    struct.pack_into("<I", data, 64, 3 if interpreter else 1)
    return bytes(data)

def marker(version="0.5.0", target="x86_64-unknown-linux-musl"):
    return f"\0XNIP_BUILD_ID_V1:{version}|{target}\0".encode()

class NpmPreparationContract(unittest.TestCase):
    def setUp(self):
        (ROOT/"target").mkdir(exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix="npm-input-contract-", dir=ROOT/"target")
        self.base = Path(self.temp.name).resolve()
    def tearDown(self):
        assert self.base.is_relative_to((ROOT/"target").resolve())
        self.temp.cleanup()
    def test_build_identity_rejects_missing_old_wrong_target_and_conflicting_markers(self):
        prepare.binary_build_identity(elf()+marker(), "0.5.0", "x86_64-unknown-linux-musl")
        for invalid in [
            elf(), elf()+marker("0.4.0"), elf()+marker(target="aarch64-unknown-linux-musl"),
            elf()+marker()+marker("0.4.0"),
        ]:
            with self.subTest(payload=invalid[-90:]), self.assertRaisesRegex(ValueError, "build identity"):
                prepare.binary_build_identity(invalid, "0.5.0", "x86_64-unknown-linux-musl")
    def test_host_gnu_elf_is_explicit_and_not_accepted_as_static_distribution(self):
        data = elf(interpreter=True)
        with self.assertRaisesRegex(ValueError, "interpreter"):
            prepare.executable_identity(data, "linux-x64")
        self.assertEqual(prepare.executable_identity(data, "linux-x64", allow_dynamic=True),
                         "dynamic ELF (host-local only)")
        with self.assertRaisesRegex(ValueError, "architecture"):
            prepare.executable_identity(data, "linux-arm64", allow_dynamic=True)
    def test_full_dist_rejects_old_version_before_writing_packages(self):
        # This malformed old-version input is a format fixture, never executed.
        version = tomllib.loads((ROOT/"Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
        name = f"xnip-v{version}-x86_64-unknown-linux-musl.tar.gz"
        archive = self.base/name
        with tarfile.open(archive, "w:gz") as out:
            for entry, data in {"xnip":elf()+marker("0.0.0"), "README.md":b"readme",
                                "LICENSE":b"license", "THIRD_PARTY_LICENSES.md":b"notices"}.items():
                info = tarfile.TarInfo(entry); info.size = len(data)
                out.addfile(info, io.BytesIO(data))
        (self.base/(name+".sha256")).write_text(prepare.digest(archive.read_bytes())+"  "+name+"\n")
        output = self.base/"output"
        # The first full-distribution target must reject before later targets or output.
        with self.assertRaisesRegex(ValueError, "build identity"):
            prepare.prepare(self.base, output)
        self.assertFalse(output.exists())
    def test_zip_rejects_links_and_traversal_even_with_expected_entry_count(self):
        for mode in ["link", "traversal"]:
            path = self.base/(mode+".zip")
            with zipfile.ZipFile(path, "w") as out:
                for name in ["xnip.exe", "README.md", "LICENSE", "THIRD_PARTY_LICENSES.md"]:
                    info = zipfile.ZipInfo("../xnip.exe" if mode=="traversal" and name=="xnip.exe" else name)
                    if mode=="link" and name=="xnip.exe":
                        info.create_system=3;info.external_attr=0o120777 << 16
                    out.writestr(info, b"bytes")
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                prepare.read_archive(path, "xnip.exe")
    def test_tar_rejects_symlink_and_extra_inventory(self):
        for mode in ["link", "extra"]:
            path = self.base/(mode+".tar.gz")
            with tarfile.open(path, "w:gz") as out:
                for name in ["xnip", "README.md", "LICENSE", "THIRD_PARTY_LICENSES.md"] + (["extra"] if mode=="extra" else []):
                    info = tarfile.TarInfo(name)
                    if mode=="link" and name=="xnip":
                        info.type=tarfile.SYMTYPE;info.linkname="../outside";out.addfile(info)
                    else:
                        info.size=1;out.addfile(info,io.BytesIO(b"x"))
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                prepare.read_archive(path, "xnip")
    def test_binary_formats_cannot_be_relabelled_to_another_architecture(self):
        pe = bytearray(80);pe[:2]=b"MZ";struct.pack_into("<I",pe,60,64)
        pe[64:68]=b"PE\0\0";struct.pack_into("<H",pe,68,0x8664)
        self.assertEqual(prepare.executable_identity(pe,"win32-x64"),"PE")
        with self.assertRaisesRegex(ValueError,"architecture"):prepare.executable_identity(pe,"win32-arm64")
        mach = bytearray(32);mach[:4]=b"\xcf\xfa\xed\xfe";struct.pack_into("<I",mach,4,0x100000c)
        self.assertEqual(prepare.executable_identity(mach,"darwin-arm64"),"Mach-O")
        with self.assertRaisesRegex(ValueError,"architecture"):prepare.executable_identity(mach,"darwin-x64")


    def fixture_dist(self, change=None):
        version = tomllib.loads((ROOT/"Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
        commit = "a"*40
        for key, (target, _) in prepare.PLATFORMS.items():
            if key.startswith("linux"):
                payload = elf(arch=62 if key.endswith("x64") else 183)
            elif key.startswith("win32"):
                payload = bytearray(80); payload[:2] = b"MZ"
                struct.pack_into("<I", payload, 60, 64); payload[64:68] = b"PE\0\0"
                struct.pack_into("<H", payload, 68, 0x8664 if key.endswith("x64") else 0xaa64)
            else:
                payload = bytearray(32); payload[:4] = b"\xcf\xfa\xed\xfe"
                struct.pack_into("<I", payload, 4, 0x1000007 if key.endswith("x64") else 0x100000c)
            payload = bytes(payload)+marker(version, target)
            archive_name = f"xnip-v{version}-{target}"+(".zip" if key.startswith("win32") else ".tar.gz")
            entries = {"xnip.exe" if key.startswith("win32") else "xnip":payload,
                "README.md":b"fixture", "LICENSE":b"fixture", "THIRD_PARTY_LICENSES.md":b"fixture"}
            archive = self.base/archive_name
            if key.startswith("win32"):
                with zipfile.ZipFile(archive,"w") as out:
                    for name,data in entries.items(): out.writestr(name,data)
            else:
                with tarfile.open(archive,"w:gz") as out:
                    for name,data in entries.items():
                        info=tarfile.TarInfo(name); info.size=len(data); out.addfile(info,io.BytesIO(data))
            sha=prepare.digest(archive.read_bytes())
            (self.base/(archive_name+".sha256")).write_text(sha+"  "+archive_name+"\n")
            evidence={"schema_version":1,"version":version,"target":target,
                "source_commit":commit,"archive_sha256":sha}
            if change: change(key,evidence)
            (self.base/(archive_name+".build.json")).write_text(json.dumps(evidence),encoding="utf8")
        return version,commit

    def test_public_release_requires_six_same_commit_archives_and_cargo_version(self):
        version,commit=self.fixture_dist()
        report=prepare.prepare(self.base,self.base/"public",public=True,source_commit=commit)
        self.assertEqual(report["publication"],"public-release")
        self.assertEqual(report["source_commit"],commit)
        self.assertFalse(report["local_only"])
        self.assertEqual(set(report["manifest"]["platforms"]),set(prepare.PLATFORMS))
        directories=list((self.base/"public/packages").iterdir())
        self.assertEqual(len(directories),7)
        for directory in directories:
            pkg=json.loads((directory/"package.json").read_text(encoding="utf8"))
            self.assertIs(pkg["private"],False)
            self.assertEqual(pkg["version"],version)
            self.assertEqual(pkg["repository"],prepare.REPOSITORY)
            self.assertEqual(pkg["scripts"]["prepack"],"node lib/validate.cjs")
            self.assertTrue((directory/"lib/validate.cjs").is_file())
        parent=json.loads((self.base/"public/packages/xnip/package.json").read_text(encoding="utf8"))
        self.assertEqual(parent["optionalDependencies"],{name:version for _,name in prepare.PLATFORMS.values()})

    def test_full_dist_is_private_unless_public_is_explicit(self):
        self.fixture_dist()
        report=prepare.prepare(self.base,self.base/"private")
        self.assertEqual(report["publication"],"disabled-private-packages")
        self.assertIsNone(report["source_commit"])
        self.assertEqual(report["manifest"]["publication"],"private-local")
        for file in (self.base/"private/packages").glob("*/package.json"):
            self.assertIs(json.loads(file.read_text(encoding="utf8"))["private"],True)

    def test_public_rejects_host_local_before_any_native_execution(self):
        with mock.patch.object(prepare.subprocess,"check_output") as execute:
            with self.assertRaisesRegex(ValueError,"six release archives"):
                prepare.prepare(None,self.base/"invalid",host_binary=self.base/"native",public=True)
            execute.assert_not_called()
        self.assertFalse((self.base/"invalid").exists())

    def test_public_rejects_mixed_commit_or_wrong_ci_commit_before_output(self):
        _,commit=self.fixture_dist(lambda key,item:item.update(source_commit="b"*40) if key=="darwin-x64" else None)
        with self.assertRaisesRegex(ValueError,"same source commit"):
            prepare.prepare(self.base,self.base/"mixed",public=True)
        self.assertFalse((self.base/"mixed").exists())
        self.fixture_dist()
        with self.assertRaisesRegex(ValueError,"same source commit"):
            prepare.prepare(self.base,self.base/"wrong-ci",public=True,source_commit="b"*40)
        self.assertFalse((self.base/"wrong-ci").exists())

    def test_public_rejects_false_provenance_and_missing_platform_before_output(self):
        for field,value in [("version","0.0.0"),("target","wrong-target"),("archive_sha256","0"*64),
                            ("source_commit","main"),("schema_version",True)]:
            self.fixture_dist(lambda key,item:item.update({field:value}) if key=="linux-x64" else None)
            out=self.base/("bad-"+field)
            with self.subTest(field=field),self.assertRaisesRegex(ValueError,"provenance"):
                prepare.prepare(self.base,out,public=True)
            self.assertFalse(out.exists())
        self.fixture_dist()
        archive=next(self.base.glob("*aarch64-pc-windows-msvc.zip"))
        archive.unlink()
        with self.assertRaisesRegex(ValueError,"archive missing"):
            prepare.prepare(self.base,self.base/"missing",public=True)
        self.assertFalse((self.base/"missing").exists())

    @unittest.skipUnless(shutil.which("node"),"Node required for prepack validation")
    def test_public_prepack_validates_all_seven_without_executing_foreign_payloads(self):
        self.fixture_dist()
        prepare.prepare(self.base,self.base/"valid",public=True)
        for file in (self.base/"valid/packages").glob("*/lib/validate.cjs"):
            result=subprocess.run([shutil.which("node"),str(file)],capture_output=True,timeout=15)
            self.assertEqual(result.returncode,0,result.stderr)

    @unittest.skipUnless(shutil.which("node"),"Node required for prepack validation")
    def test_changing_only_private_cannot_publish_a_local_parent_or_native_package(self):
        self.fixture_dist()
        prepare.prepare(self.base,self.base/"private")
        for directory in [self.base/"private/packages/xnip", self.base/"private/packages/xnip-linux-x64-musl"]:
            file=directory/"package.json"
            pkg=json.loads(file.read_text(encoding="utf8"));pkg["private"]=False
            file.write_text(json.dumps(pkg),encoding="utf8")
            result=subprocess.run([shutil.which("node"),str(directory/"lib/validate.cjs")],capture_output=True,timeout=15)
            self.assertNotEqual(result.returncode,0)
            self.assertIn(b"private flag",result.stderr)

    @unittest.skipUnless(shutil.which("node"),"Node required for prepack validation")
    def test_prepack_rechecks_architecture_and_static_linkage_after_hash_changes(self):
        self.fixture_dist()
        prepare.prepare(self.base,self.base/"public",public=True)
        directory=self.base/"public/packages/xnip-linux-x64-musl"
        file=directory/"binary.json"
        original=json.loads(file.read_text(encoding="utf8"))
        payload=(directory/"xnip").read_bytes()
        for kind in ["dynamic","wrong-arch","wrong-marker"]:
            data=bytearray(payload)
            if kind=="dynamic":struct.pack_into("<I",data,64,3)
            elif kind=="wrong-arch":struct.pack_into("<H",data,18,183)
            else:data=bytearray(data.replace(marker(original["version"],original["target"]),marker("0.0.0",original["target"])))
            info=json.loads(json.dumps(original))
            info["sha256"]=prepare.digest(data);info["size"]=len(data)
            info["release"]["archives"]["linux-x64"]["sha256"]=info["sha256"]
            (directory/"xnip").write_bytes(data);file.write_text(json.dumps(info),encoding="utf8")
            result=subprocess.run([shutil.which("node"),str(directory/"lib/validate.cjs")],capture_output=True,timeout=15)
            self.assertNotEqual(result.returncode,0,kind)

    @unittest.skipUnless(shutil.which("node"),"Node required for local packing")
    def test_public_pack_records_portable_seven_tarballs_and_integrity(self):
        version,commit=self.fixture_dist()
        report=prepare.prepare(self.base,self.base/"packed",True,public=True,source_commit=commit)
        self.assertEqual(len(report["tarballs"]),7)
        self.assertEqual(report["tarballs"][-1]["name"],"xnip")
        for entry in report["tarballs"]:
            self.assertFalse(Path(entry["filename"]).is_absolute())
            self.assertEqual(entry["version"],version)
            file=self.base/"packed"/entry["filename"]
            self.assertEqual(file.stat().st_size,entry["size"])
            self.assertEqual(prepare.digest(file.read_bytes()),entry["sha256"])
            self.assertTrue(entry["integrity"].startswith("sha512-"))


    @unittest.skipUnless(shutil.which("node"),"Node required for host identity contract")
    def test_release_smoke_fails_wrong_native_platform_before_install(self):
        from npm_release_smoke import smoke
        _,commit=self.fixture_dist()
        prepare.prepare(self.base,self.base/"public",public=True)
        identity=json.loads(subprocess.check_output([shutil.which("node"),"-p",
            "JSON.stringify({platform:process.platform,arch:process.arch})"],text=True))
        actual=identity["platform"]+"-"+identity["arch"]
        other=next(key for key in prepare.PLATFORMS if key!=actual)
        with self.assertRaisesRegex(ValueError,"expected native platform"):
            smoke(self.base/"public",commit,expected_platform=other)

if __name__=="__main__":unittest.main()
