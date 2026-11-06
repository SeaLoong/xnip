"""Advertised integrations retain executable entry points and recorded evidence."""

import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


class IntegrationCatalogContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.catalog = json.loads((ROOT / "integrations/catalog.json").read_text())
        cls.hosts = cls.catalog["hosts"]

    def test_every_previously_documented_or_measured_framework_has_a_current_entry(self):
        required = {"dsh", "pi", "gemini-sdk", "gemini-cli", "cline-sdk", "cline-gui",
                    "openhands", "opencode", "aider", "codex", "claude-code", "cursor",
                    "vscode", "copilot", "goose", "roo-code"}
        ids = [host["id"] for host in self.hosts]
        self.assertEqual(len(ids), len(set(ids)))
        self.assertTrue(required <= set(ids), required - set(ids))

    def test_each_advertised_entry_has_implementation_guide_tests_and_benchmark_evidence(self):
        self.assertEqual(self.catalog["schema_version"], 1)
        for host in self.hosts:
            with self.subTest(host=host["id"]):
                self.assertTrue(host["scope"])
                for field in ("entrypoints", "tests", "benchmarks"):
                    self.assertTrue(host[field], (host["id"], field))
                    for name in host[field]:
                        self.assertTrue((ROOT / name).exists(), name)
                self.assertTrue((ROOT / host["guide"].split("#")[0]).is_file(), host["guide"])
                for name in host["benchmarks"]:
                    report = json.loads((ROOT / name).read_text())
                    self.assertTrue(report, name)

    def test_gui_profiles_describe_the_client_configuration_boundary(self):
        gui = {"cursor", "vscode", "copilot", "roo-code", "cline-gui", "windsurf", "continue", "zed"}
        for host in self.hosts:
            if host["id"] in gui:
                with self.subTest(host=host["id"]):
                    self.assertEqual(host["mode"], "mcp-profile")
                    self.assertIn("configuration", host["scope"])


if __name__ == "__main__":
    unittest.main()
