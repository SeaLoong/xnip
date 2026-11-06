"""Real npm-installed DSH lifecycle, with every working directory outside checkout.

Prepare the pinned official DSH hosts, build xnip, then run this file. Select one
host with XNIP_DSH_NATIVE_HOST; otherwise both supported versions execute.
"""
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
VERSIONS = ("0.2.1-rc.2", "0.2.1-rc.5")
NODE = shutil.which("node")
BINARY = Path(os.environ.get("XNIP_TEST_BINARY", ROOT/"target/debug"/("xnip.exe" if os.name == "nt" else "xnip")))
spec = importlib.util.spec_from_file_location("npm_prepare", ROOT/"npm/prepare.py")
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def tree_digest(root):
    """Hash file bytes and permissions; record links without following them."""
    result = hashlib.sha256()
    count = 0

    def visit(path, relative):
        nonlocal count
        metadata = path.lstat()
        mode = stat.S_IMODE(metadata.st_mode)
        if path.is_symlink():
            item = [relative, "link", mode, os.readlink(path)]
        elif path.is_dir():
            item = [relative, "directory", mode]
        else:
            item = [relative, "file", mode, digest(path)]
        result.update(json.dumps(item, ensure_ascii=False, separators=(",", ":")).encode())
        result.update(b"\n")
        count += 1
        if path.is_dir() and not path.is_symlink():
            for child in sorted(path.iterdir(), key=lambda entry: entry.name):
                visit(child, relative + "/" + child.name)

    visit(root, ".")
    return {"sha256": result.hexdigest(), "entries": count}


class InstalledNpmDshLifecycle(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if NODE is None or not BINARY.is_file():
            raise RuntimeError("Build xnip and activate Node/npm before the installed lifecycle E2E")
        selected = os.environ.get("XNIP_DSH_NATIVE_HOST")
        cls.hosts = [Path(selected).resolve()] if selected else [ROOT/"target/native-hosts"/f"dsh-{version}" for version in VERSIONS]
        for host in cls.hosts:
            manifest = json.loads((host/"node_modules/@deepseek-ai/dsh/package.json").read_text(encoding="utf-8"))
            if manifest["version"] not in VERSIONS:
                raise RuntimeError(f"Expected a pinned official DSH host, got {manifest['version']}")
        cls.temporary = tempfile.TemporaryDirectory(prefix="xnip-npm-dsh-lifecycle-")
        cls.base = Path(cls.temporary.name).resolve()
        assert not cls.base.is_relative_to(ROOT)
        cls.artifacts = prepare.prepare(None, cls.base/"artifacts", True, BINARY.resolve())
        artifact = next(item for item in cls.artifacts["tarballs"] if item["name"] == prepare.PARENT_NAME)
        cls.tarball = cls.base/"artifacts"/artifact["filename"]
        cls.prefix = cls.base/"installed"
        subprocess.run([*prepare.npm_command(), "install", "--prefix", str(cls.prefix),
            "--cache", str(cls.base/"npm-cache"), "--offline", "--ignore-scripts", "--no-audit", "--no-fund", str(cls.tarball)],
            cwd=cls.base, check=True, capture_output=True, timeout=90)
        cls.package = cls.prefix/"node_modules"/prepare.PARENT_NAME
        cls.launcher = cls.prefix/"node_modules/.bin"/("xnip.cmd" if os.name == "nt" else "xnip")
        assert cls.launcher.is_file()
        if os.name != "nt":
            assert cls.launcher.resolve() == cls.package/"bin/xnip.cjs"
        cls.receipts = []
        cls.evidence = Path(os.environ.get("XNIP_NPM_DSH_EVIDENCE", ROOT/"target/native-e2e/npm-dsh-lifecycle.json"))

    @classmethod
    def tearDownClass(cls):
        if hasattr(cls, "temporary"):
            cls.evidence.parent.mkdir(parents=True, exist_ok=True)
            cls.evidence.write_text(json.dumps({
                "status": "passed" if len(cls.receipts) == 3 * len(cls.hosts) else "failed",
                "npm_tarball_sha256": digest(cls.tarball),
                "binary_sha256": next(iter(cls.artifacts["manifest"]["platforms"].values()))["sha256"],
                "isolated_prefix": str(cls.prefix),
                "cases": cls.receipts,
            }, indent=2) + "\n", encoding="utf-8")
            cls.temporary.cleanup()

    def command(self, argv, cwd, env, *, successful=True):
        result = subprocess.run([str(argument) for argument in argv], cwd=cwd, env=env,
            capture_output=True, encoding="utf-8", timeout=180)
        if successful:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def check_verification(self, report, installed):
        self.assertEqual(report["phase"], "active" if installed else "restored")
        verification = report["verification"]
        self.assertTrue(verification["functional_edit"])
        self.assertTrue(verification["checks"])
        self.assertTrue(all(check["status"] == "passed" for check in verification["checks"]))
        checks = {check["name"]: check["detail"] for check in verification["checks"]}
        self.assertIn("official-profile-composition", checks)
        self.assertIn("real-native-edit", checks)
        if installed:
            detail = checks["profile-governance"]
            for assertion in ["dsh-rewind-plugin@0.19.2", "real approval rejection and audit pair",
                              "real /rewind command", "byte-exact restore", "observation refresh", "zero model calls"]:
                self.assertIn(assertion, detail)

    def scenario(self, host, shared=False, ordinary=False):
        version = json.loads((host/"node_modules/@deepseek-ai/dsh/package.json").read_text(encoding="utf-8"))["version"]
        scenario = "shared-dependencies" if shared else "ordinary-dependencies" if ordinary else "fresh"
        directory = Path(tempfile.mkdtemp(prefix=f"{version}-{scenario}-", dir=self.base))
        project = directory/"project"; project.mkdir()
        home = directory/"dsh-home"; home.mkdir()
        state = directory/"installation-state"
        dsh = (host/"node_modules/@deepseek-ai/dsh/lib/bin.js").resolve()
        env = {key: value for key, value in os.environ.items() if not key.startswith("XNIP_") and key != "NODE_PATH"}
        env.update(DSH_HOME=str(home), XNIP_AUTO_UPDATE="0", XNIP_NPM_CACHE_DIR=str(directory/"native-cache"),
                   XNIP_UPDATE_STATE_DIR=str(directory/"update-state"),
                   XNIP_DSH_NATIVE_PLUGIN=str(directory/"intentionally-missing-adapter"))
        env["PATH"] = os.pathsep.join(entry for entry in env.get("PATH", "").split(os.pathsep)
            if entry and not (Path(entry)/("xnip.exe" if os.name == "nt" else "xnip")).is_file())
        self.assertEqual(shutil.which("xnip", path=env["PATH"]), None)
        initialized = self.command([dsh, "--profile", "web", "--dump-config"], project, env)
        self.assertIn("id: fs-sandbox", initialized.stdout)
        profile = home/"profiles/web"
        manifest = profile/"package.json"
        metadata = json.loads(manifest.read_text(encoding="utf-8"))
        metadata["dsh"]["profile"]["bundles"].append("dsh-rewind-plugin")
        manifest.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
        with (profile/"cordis.patch.yml").open("a", encoding="utf-8") as stream:
            stream.write("\n# user-owned profile patch: 标题\n")
        (home/"cordis.patch.yml").write_text("# user-owned home patch\n[]\n", encoding="utf-8")
        neighbor = home/"profiles/neighbor"
        shutil.copytree(profile, neighbor, symlinks=True)
        shared_before = None
        if shared:
            # Both real profiles share the prepared official dependencies through
            # a home-level link. The provider owns its separate local bundle.
            (home/"node_modules").symlink_to(host/"node_modules", target_is_directory=True)
            for current in [profile, neighbor]:
                (current/"node_modules").symlink_to(Path("../../node_modules"), target_is_directory=True)
            shared_before = tree_digest(host/"node_modules")
        if ordinary:
            dependency = profile/"node_modules/retained-user-package"
            dependency.mkdir(parents=True)
            (dependency/"package.json").write_text('{"name":"retained-user-package","version":"1.0.0"}\n', encoding="utf-8")
            (dependency/"sentinel.bin").write_bytes(b"retained dependency bytes\0" * (128 * 1024))
            (profile/"pnpm-lock.yaml").write_text("lockfileVersion: '9.0'\nimporters:\n  .: {}\n", encoding="utf-8")
        profile_before = tree_digest(profile)
        neighbor_before = tree_digest(neighbor)
        home_patch_before = (home/"cordis.patch.yml").read_bytes()
        shared_link_before = os.readlink(home/"node_modules") if shared else None
        common = ["--profile", profile, "--dsh", dsh, "--node", NODE, "--state-dir", state]
        receipts = []

        def native(verb, *extra):
            argv = [self.launcher, "native", verb, *common, *extra]
            self.assertNotIn("--plugin", argv)
            result = self.command(argv, project, env)
            report = json.loads(result.stdout)
            receipts.append({"command": verb, "report": report})
            return report

        installed = native("install")
        self.check_verification(installed, True)
        installation_manifest = json.loads((state/"install-manifest.json").read_text(encoding="utf-8"))
        self.assertEqual(installation_manifest["baseline"]["roots"],
                         [str(profile/"package.json"), str(profile/".xnip-native-package")])
        self.assertEqual(str(profile/"node_modules") in installation_manifest["observed"]["roots"], shared)
        snapshot_bytes = sum(file.stat().st_size for file in (state/"blobs").rglob("*") if file.is_file())
        if ordinary:
            self.assertLess(snapshot_bytes, (profile/"node_modules/retained-user-package/sentinel.bin").stat().st_size // 4)
        bundle = profile/".xnip-native-package"
        installed_sources = {}
        for filename in ["index.mjs", "runtime.mjs", "rewind-validation.mjs", "lifecycle.mjs", "editor-client.mjs"]:
            source = self.package/"integrations"/("native" if filename == "editor-client.mjs" else "dsh-native")/filename
            self.assertEqual(digest(bundle/filename), digest(source), filename)
            installed_sources[filename] = {"installed_source": str(source), "sha256": digest(source)}
        configured = re.search(r"^\s+executable: (.+)$", (bundle/"cordis.patch.yml").read_text(encoding="utf-8"), re.MULTILINE)
        executable = Path(json.loads(configured[1]))
        self.assertTrue(executable.is_relative_to(directory/"native-cache"))
        payload = next(iter(self.artifacts["manifest"]["platforms"].values()))
        self.assertEqual(digest(executable), payload["sha256"])
        self.check_verification(native("verify"), True)
        self.check_verification(native("uninstall"), False)
        self.check_verification(native("verify", "--restored"), False)
        cleaned = json.loads(self.command([self.launcher, "native", "clean", "--state-dir", state, "--purge-user-data"], project, env).stdout)
        self.assertEqual(cleaned["installation_data"], "cleared")
        self.assertEqual(cleaned["retained_coordination_paths"], [str(state/"operation.lock")])
        self.assertEqual(sorted(entry.name for entry in state.iterdir()), ["operation.lock"])
        for _ in range(2):
            doctor = json.loads(self.command([self.launcher, "native", "doctor", "--state-dir", state], project, env).stdout)
            self.assertEqual(doctor["code"], "state_empty")
            self.assertIs(doctor["installed"], False)
            recovered = self.command([self.launcher, "native", "recover", *common], project, env, successful=False)
            self.assertIn("state_empty", recovered.stderr)
            self.assertIn(str(state), recovered.stderr)
            self.assertIn("install-manifest.json", recovered.stderr)
        repeated = json.loads(self.command([self.launcher, "native", "clean", "--state-dir", state, "--purge-user-data"], project, env).stdout)
        self.assertEqual(repeated["removed"], [])
        self.assertEqual(tree_digest(profile), profile_before)
        self.assertEqual(tree_digest(neighbor), neighbor_before)
        self.assertEqual((home/"cordis.patch.yml").read_bytes(), home_patch_before)
        if shared:
            self.assertEqual(os.readlink(home/"node_modules"), shared_link_before)
            self.assertEqual(tree_digest(host/"node_modules"), shared_before)
        self.receipts.append({"host": version, "scenario": scenario,
            "launcher": str(self.launcher), "cwd": str(project), "model_calls": 0,
            "installed_sources": installed_sources, "profile_restored": profile_before,
            "neighbor_preserved": neighbor_before, "shared_tree_preserved": shared_before,
            "snapshot_blob_bytes": snapshot_bytes, "clean": cleaned, "empty_state_reentries": 2, "phases": receipts})

    def test_fresh_profile_install_verify_restore_and_clean(self):
        for host in self.hosts:
            with self.subTest(host=host.name):
                self.scenario(host, False)

    def test_shared_dependency_links_and_neighbor_survive_full_lifecycle(self):
        for host in self.hosts:
            with self.subTest(host=host.name):
                self.scenario(host, True)

    def test_ordinary_dependencies_and_lockfile_survive_without_snapshot_copy(self):
        for host in self.hosts:
            with self.subTest(host=host.name):
                self.scenario(host, ordinary=True)


if __name__ == "__main__":
    unittest.main()
