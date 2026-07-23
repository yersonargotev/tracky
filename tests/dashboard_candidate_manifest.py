import copy
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
SCRIPT = ROOT / "scripts" / "dashboard_candidate_manifest.py"
SPEC = importlib.util.spec_from_file_location("dashboard_candidate_manifest", SCRIPT)
collector = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(collector)
evidence = collector.evidence


class DashboardCandidateManifestTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.targets = self.root / "targets"
        self.targets.mkdir()
        baseline = evidence.read_json(evidence.BASELINE)
        by_target = {item["target"]: item for item in baseline["targets"]}
        self.commit = "a" * 40
        self.lockfile = baseline["lockfile_sha256"]
        for target in sorted(evidence.TARGETS):
            frozen = by_target[target]
            fragment = {
                "target": target,
                "commit": self.commit,
                "lockfile_sha256": self.lockfile,
                "tools": {"cargo-dist": "0.30.0", "rustc": "1.90.0"},
                "artifact": {"name": "tracky-%s.tar.xz" % target, "sha256": frozen["archive_sha256"], "bytes": frozen["archive_bytes"]},
                "latency": {"warmups": 5, "runs": 30, **{name: limit for name, limit in evidence.LATENCY_LIMITS_MS.items()}},
                "resources": {**{name: limit for name, limit in evidence.RESOURCE_LIMITS.items()}, "cycles": 100, "descriptor_growth": 0, "memory_growth_bytes": 0, "memory_growth_percent": 0},
                "size": {name: frozen[name] for name in ("target", "archive_bytes", "archive_sha256", "executable_bytes", "executable_sha256")},
                "commands": ["measure %s" % target],
            }
            self.write(self.targets / (target + ".json"), fragment)
        self.browsers = self.root / "browsers.json"
        self.write(self.browsers, {"browsers": {
            "safari-minimum": "26.0", "safari-latest": "26.1",
            "firefox-esr-minimum": "153 ESR", "firefox-latest": "154",
            "chromium-minimum": "150", "chromium-latest": "151",
        }, "commands": ["test browsers"]})
        self.results = self.root / "results.json"
        self.write(self.results, [{"gate": gate, "status": "pass", "evidence": "https://github.com/yersonargotev/tracky/actions/runs/%d" % index} for index, gate in enumerate(sorted(evidence.REQUIRED_RELEASE_GATES), 1)])
        self.inventory = self.root / "inventory.json"
        self.write(self.inventory, {"resolved_package_count": baseline["resolved_package_count"], "asset_bytes": 0})

    def tearDown(self):
        self.temp.cleanup()

    @staticmethod
    def write(path, value):
        path.write_text(json.dumps(value), encoding="utf-8")

    def assemble(self):
        return collector.assemble(self.targets, self.browsers, self.results, self.inventory, "release-owner", "reviewer")

    def test_assembles_release_valid_canonical_inputs_deterministically(self):
        manifest = self.assemble()
        evidence.validate_manifest(manifest, release=True, expected_commit=self.commit, expected_lockfile_sha256=self.lockfile)
        self.assertEqual(manifest["targets"], sorted(evidence.TARGETS))
        self.assertEqual(manifest["approval"], {"approved": True, "approved_by": "reviewer"})
        self.assertEqual(manifest["commands"][-1], "test browsers")
        self.assertEqual(manifest["measurements"]["sizes"]["asset_bytes"], 0)

    def test_cli_writes_canonical_json(self):
        output = self.root / "nested" / "manifest.json"
        subprocess.run([sys.executable, str(SCRIPT), "--targets-dir", str(self.targets), "--browsers", str(self.browsers), "--results", str(self.results), "--inventory", str(self.inventory), "--maintainer", "release-owner", "--approved-by", "reviewer", "--output", str(output)], check=True)
        self.assertEqual(output.read_text(encoding="utf-8"), evidence.canonical_json(json.loads(output.read_text(encoding="utf-8"))))

    def test_rejects_missing_duplicate_and_unknown_targets(self):
        first = next(self.targets.glob("*.json"))
        first.unlink()
        with self.assertRaisesRegex(ValueError, "exactly 2"):
            self.assemble()

    def test_rejects_inconsistent_commit_lockfile_and_tools(self):
        path = next(self.targets.glob("*.json"))
        original = json.loads(path.read_text())
        cases = [("commit", "b" * 40, "commits differ"), ("lockfile_sha256", "b" * 64, "lockfiles differ"), ("tools", {"rustc": "other"}, "tool versions differ")]
        for field, value, message in cases:
            with self.subTest(field=field):
                changed = copy.deepcopy(original)
                changed[field] = value
                self.write(path, changed)
                with self.assertRaisesRegex(ValueError, message):
                    self.assemble()
                self.write(path, original)

    def test_rejects_browser_gate_status_and_url_contract_drift(self):
        browser_input = json.loads(self.browsers.read_text())
        del browser_input["browsers"]["safari-minimum"]
        self.write(self.browsers, browser_input)
        with self.assertRaisesRegex(ValueError, "six required browsers"):
            self.assemble()

        self.setUp_browser_again()
        results = json.loads(self.results.read_text())
        results[0]["status"] = "fail"
        self.write(self.results, results)
        with self.assertRaisesRegex(ValueError, "must pass"):
            self.assemble()
        results[0]["status"] = "pass"
        results[0]["evidence"] = "https://example.com/run/1"
        self.write(self.results, results)
        with self.assertRaisesRegex(ValueError, "GitHub Actions"):
            self.assemble()

    def setUp_browser_again(self):
        self.write(self.browsers, {"browsers": {
            "safari-minimum": "26.0", "safari-latest": "26.1",
            "firefox-esr-minimum": "153 ESR", "firefox-latest": "154",
            "chromium-minimum": "150", "chromium-latest": "151",
        }, "commands": ["test browsers"]})

    def test_rejects_mismatched_size_target_and_inventory_shape(self):
        path = next(self.targets.glob("*.json"))
        fragment = json.loads(path.read_text())
        fragment["size"]["target"] = next(target for target in evidence.TARGETS if target != fragment["target"])
        self.write(path, fragment)
        with self.assertRaisesRegex(ValueError, "size target differs"):
            self.assemble()


if __name__ == "__main__":
    unittest.main()
