#!/usr/bin/env python3
"""Offline CI scope contracts, including actual push and synthetic-merge histories."""

from __future__ import annotations

from contextlib import contextmanager
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
HELPER = ROOT / ".github" / "scripts" / "ci_scope.py"
SPEC = importlib.util.spec_from_file_location("xnip_ci_scope", HELPER)
assert SPEC is not None and SPEC.loader is not None
ci_scope = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ci_scope)
GIT_ENV = {**os.environ, "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull}


class Repository:
    def __init__(self, path: Path):
        self.path = path
        self.git("init", "--quiet", "--initial-branch=main")
        self.git("config", "user.name", "CI scope contract")
        self.git("config", "user.email", "ci-scope-contract@example.invalid")
        self.git("config", "core.autocrlf", "false")
        self.write("README.md")

    def git(self, *args: str) -> str:
        result = subprocess.run(
            ["git", "-C", str(self.path), *args], check=True,
            capture_output=True, env=GIT_ENV,
        )
        return result.stdout.decode("utf-8").strip()

    def write(self, path: str, content: str = "fixture\n") -> None:
        destination = self.path / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(content, encoding="utf-8")

    def commit(self, message: str) -> str:
        self.git("add", "--all")
        self.git("commit", "--quiet", "-m", message)
        return self.git("rev-parse", "HEAD")

    def cli(
        self, payload: object, sha: str, *, event_name: str = "push",
        ref: str = "refs/heads/main", extra_args: tuple[str, ...] = (),
        initial_output: str = "",
    ) -> tuple[subprocess.CompletedProcess, str]:
        # Keep workflow metadata outside the checkout, just like hosted runners.
        with tempfile.TemporaryDirectory(prefix="xnip-ci-event-") as temporary:
            event = Path(temporary) / "event.json"
            output = Path(temporary) / "output"
            event.write_text(json.dumps(payload), encoding="utf-8")
            output.write_text(initial_output, encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(HELPER), "--event", str(event),
                 "--output", str(output), "--root", str(self.path), *extra_args],
                capture_output=True, text=True, env={
                    **GIT_ENV, "GITHUB_EVENT_NAME": event_name,
                    "GITHUB_SHA": sha, "GITHUB_REF": ref,
                },
            )
            return result, output.read_text(encoding="utf-8")


@contextmanager
def repository():
    with tempfile.TemporaryDirectory(prefix="xnip-ci-scope-") as temporary:
        yield Repository(Path(temporary))


class GroupBoundaryContracts(unittest.TestCase):
    def assert_scope(self, path: str, rust=False, integrations=False, full=False):
        self.assertEqual(
            ci_scope.classify([path]),
            {"rust": rust, "integrations": integrations, "full": full},
        )

    def test_runtime_build_installers_and_unknown_inputs_require_rust(self):
        for path in [
            "src/main.rs", "src/README.md", "tests/edit_contract.rs",
            "tests/fixtures/data.json", "tests/install_sh.py", "tests/install_ps1.ps1",
            "Cargo.toml", "Cargo.lock", "Cargo.future", "build.rs", "Cross.toml",
            "deny.toml", "rust-toolchain", "rust-toolchain.toml", ".cargo/config.toml",
            "installer/package.sh", "install.sh", "install.ps1",
            "benches/large_file.rs", "benches/nested/new.rs", "new/unknown.bin",
            ".github/scripts/new_check.py", ".gitignore",
        ]:
            with self.subTest(path=path):
                self.assert_scope(path, rust=True)

    def test_cheap_docs_license_release_and_python_changes_skip_rust(self):
        for path in [
            "README.md", "README.zh.md", "CHANGELOG.md", "CONTRIBUTING.md",
            "GLOSSARY.md", "THIRD_PARTY_LICENSES.md", "LICENSE", "LICENSE-MIT",
            "COPYING", "NOTICE", "notes/guide.md", "docs/architecture.md",
            "docs/testing.md", "docs/benchmarks/results.json",
            "docs/benchmarks/charts/overview.svg", "benches/edit_cost.py",
            "benches/native/codex_adapter.py", "tests/release_contract.py",
            "tests/publish_release_contract.py", ".github/scripts/release.py",
            ".github/scripts/publish_release.py",
        ]:
            with self.subTest(path=path):
                self.assert_scope(path)

    def test_generated_integrations_and_source_docs_require_integration_checks(self):
        for path in [
            "docs/SKILL.md", "docs/error-codes.md", "docs/patch.md",
            "docs/recovery.md", "docs/edit.md", "integrations/codex/guide.md",
            "integrations/generic/package.sh", ".github/skills/xnip-editing/SKILL.md",
            ".github/skills/another/references/edit.md",
        ]:
            with self.subTest(path=path):
                self.assert_scope(path, integrations=True)

    def test_executable_documentation_and_generator_overlap_groups(self):
        for path in ["docs/apply-format.md", "docs/examples.md"]:
            with self.subTest(path=path):
                self.assert_scope(path, rust=True, integrations=True)
        for path in ["docs/cli.md", "docs/mcp.md"]:
            with self.subTest(path=path):
                self.assert_scope(path, rust=True)
        for path in ["xtask/Cargo.toml", "xtask/src/main.rs", "xtask/README.md"]:
            with self.subTest(path=path):
                self.assert_scope(path, rust=True, integrations=True)

    def test_workflow_and_selector_changes_require_every_group(self):
        for path in [
            ".github/workflows/ci.yml", ".github/workflows/future.yml",
            ".github/scripts/ci_scope.py", "tests/ci_scope_contract.py",
            ".github/scripts/verified_ci.cjs", "tests/verified_ci_contract.cjs",
        ]:
            with self.subTest(path=path):
                self.assert_scope(path, rust=True, integrations=True, full=True)

    def test_empty_and_mixed_scopes_combine_without_losing_checks(self):
        self.assertEqual(
            ci_scope.classify([]), {"rust": False, "integrations": False, "full": False},
        )
        self.assertEqual(
            ci_scope.classify(["src/main.rs", "integrations/codex/guide.md", "README.md"]),
            {"rust": True, "integrations": True, "full": False},
        )


class GitHistoryContracts(unittest.TestCase):
    def test_entire_multicommit_push_and_committed_tree_are_inspected(self):
        with repository() as repo:
            before = repo.commit("base")
            repo.write("src/space name.rs")
            repo.commit("runtime earlier in push")
            repo.write("docs/guide.md")
            head = repo.commit("docs at push tip")
            repo.write(".github/workflows/uncommitted.yml")
            payload = {"before": before, "commits": [{"modified": ["README.md"]}]}
            result, output = repo.cli(payload, head, initial_output="existing=value\n")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                output, "existing=value\nrust=true\nintegrations=false\nfull=false\n",
            )
            self.assertEqual(result.stdout, "rust=true\nintegrations=false\nfull=false\n")

    def test_documentation_only_and_empty_pushes_skip_expensive_groups(self):
        with repository() as repo:
            before = repo.commit("base")
            repo.write("docs/testing.md")
            head = repo.commit("docs")
            for base in [before, head]:
                with self.subTest(before=base):
                    result, output = repo.cli({"before": base}, head)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(output, "rust=false\nintegrations=false\nfull=false\n")

    def test_deleted_files_still_select_their_original_scope(self):
        with repository() as repo:
            repo.write("integrations/codex/guide.md")
            before = repo.commit("integration exists")
            (repo.path / "integrations/codex/guide.md").unlink()
            head = repo.commit("delete integration")
            result, output = repo.cli({"before": before}, head)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output, "rust=false\nintegrations=true\nfull=false\n")

    def test_renames_inspect_old_and_new_paths(self):
        with repository() as repo:
            repo.write("src/old.rs")
            before = repo.commit("runtime exists")
            (repo.path / "docs").mkdir()
            repo.git("mv", "src/old.rs", "docs/renamed.md")
            head = repo.commit("move runtime into docs")
            result, output = repo.cli({"before": before}, head)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output, "rust=true\nintegrations=false\nfull=false\n")

    def test_initial_push_compares_against_empty_tree(self):
        with repository() as repo:
            repo.write("integrations/codex/guide.md")
            head = repo.commit("initial integration")
            result, output = repo.cli({"before": "0" * len(head)}, head)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output, "rust=false\nintegrations=true\nfull=false\n")

    @unittest.skipIf(os.name == "nt", "Windows filenames cannot contain newlines")
    def test_nul_delimited_diff_preserves_newlines_in_paths(self):
        with repository() as repo:
            before = repo.commit("base")
            repo.write("src/line\nbreak.rs")
            head = repo.commit("unusual runtime filename")
            result, output = repo.cli({"before": before}, head)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output, "rust=true\nintegrations=false\nfull=false\n")

    def test_pr_uses_merge_base_and_the_checked_out_synthetic_merge(self):
        with repository() as repo:
            repo.commit("common ancestor")
            repo.git("checkout", "--quiet", "-b", "feature")
            repo.write("integrations/codex/guide.md")
            feature = repo.commit("PR integration")
            repo.git("checkout", "--quiet", "main")
            repo.write("src/main.rs")
            base = repo.commit("target branch advanced")
            repo.git("merge", "--quiet", "--no-ff", "feature", "-m", "synthetic merge")
            merge = repo.git("rev-parse", "HEAD")
            event = {"pull_request": {"base": {"sha": base}, "head": {"sha": feature}}}
            result, output = repo.cli(event, merge, event_name="pull_request", ref="refs/pull/1/merge")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output, "rust=true\nintegrations=true\nfull=false\n")

    def test_manual_unknown_and_tag_events_always_select_every_group(self):
        with repository() as repo:
            head = repo.commit("docs only")
            for event_name, ref in [
                ("workflow_dispatch", "refs/heads/main"),
                ("workflow_call", "refs/heads/main"),
                ("schedule", "refs/heads/main"),
                ("push", "refs/tags/v1.0.0"),
            ]:
                with self.subTest(event_name=event_name, ref=ref):
                    result, output = repo.cli({}, head, event_name=event_name, ref=ref)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(output, "rust=true\nintegrations=true\nfull=true\n")

    def test_default_branch_runtime_push_is_a_complete_release_baseline(self):
        with repository() as repo:
            before = repo.commit("base")
            repo.write("src/main.rs")
            head = repo.commit("runtime change")
            event = {"before": before, "repository": {"default_branch": "main"}}
            result, output = repo.cli(event, head)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output, "rust=true\nintegrations=true\nfull=true\n")
            for ref in ["refs/heads/feature", "refs/heads/main-other", "main"]:
                with self.subTest(ref=ref):
                    result, output = repo.cli(event, head, ref=ref)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(output, "rust=true\nintegrations=false\nfull=false\n")
            pr = {
                "repository": {"default_branch": "main"},
                "pull_request": {"base": {"sha": before}, "head": {"sha": head}},
            }
            result, output = repo.cli(pr, head, event_name="pull_request")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output, "rust=true\nintegrations=false\nfull=false\n")

    def test_default_branch_docs_and_integration_pushes_remain_scoped(self):
        with repository() as repo:
            before = repo.commit("base")
            repo.write("docs/testing.md")
            docs = repo.commit("docs only")
            event = {"before": before, "repository": {"default_branch": "main"}}
            result, output = repo.cli(event, docs)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output, "rust=false\nintegrations=false\nfull=false\n")
            repo.write("integrations/codex/guide.md")
            integration = repo.commit("integration only")
            result, output = repo.cli(event, integration)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output, "rust=false\nintegrations=true\nfull=false\n")

    def test_force_all_promotes_a_docs_push_to_complete_validation(self):
        with repository() as repo:
            head = repo.commit("docs only")
            result, output = repo.cli({"before": head}, head, extra_args=("--force-all",))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output, "rust=true\nintegrations=true\nfull=true\n")

    def test_missing_malformed_and_unavailable_shas_fail_without_output(self):
        with repository() as repo:
            head = repo.commit("base")
            for before in [None, "", "HEAD", "--help", "f" * 40]:
                with self.subTest(before=before):
                    result, output = repo.cli({"before": before}, head, initial_output="existing=value\n")
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("ci scope:", result.stderr)
                    self.assertEqual(output, "existing=value\n")
            for sha in ["", "HEAD", "0" * 40, "f" * 64]:
                with self.subTest(sha=sha):
                    result, output = repo.cli({"before": head}, sha)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(output, "")

    def test_invalid_pr_metadata_and_unrelated_histories_fail(self):
        with repository() as repo:
            head = repo.commit("main")
            for event in [
                {}, {"pull_request": {}},
                {"pull_request": {"base": {"sha": head}, "head": {}}},
                {"pull_request": {"base": {"sha": "f" * 40}, "head": {"sha": head}}},
            ]:
                with self.subTest(event=event):
                    result, output = repo.cli(event, head, event_name="pull_request")
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(output, "")
            repo.git("checkout", "--quiet", "--orphan", "unrelated")
            other = repo.commit("unrelated root")
            event = {"pull_request": {"base": {"sha": head}, "head": {"sha": other}}}
            result, output = repo.cli(event, other, event_name="pull_request")
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(output, "")

    def test_invalid_event_shape_and_missing_event_name_fail(self):
        with repository() as repo:
            head = repo.commit("base")
            for event, event_name in [([], "push"), ({"before": head}, "")]:
                with self.subTest(event=event, event_name=event_name):
                    result, output = repo.cli(event, head, event_name=event_name)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(output, "")


if __name__ == "__main__":
    unittest.main()
