import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "dashboard_evidence.py"
SPEC = importlib.util.spec_from_file_location("dashboard_evidence", SCRIPT)
tool = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(tool)


class DashboardEvidenceToolTest(unittest.TestCase):
    def setUp(self):
        self.baseline = tool.read_json(tool.BASELINE)
        self.current = {
            "schema_version": 1,
            "resolved_package_count": self.baseline["resolved_package_count"] + 60,
            "asset_bytes": 250 * 1024,
            "targets": [
                {
                    "target": item["target"],
                    "executable_bytes": min(item["executable_bytes"] + int(2.5 * 1024 * 1024), int(item["executable_bytes"] * 1.20)),
                    "archive_bytes": min(item["archive_bytes"] + 1024 * 1024, int(item["archive_bytes"] * 1.20)),
                    "archive_sha256": "a" * 64,
                    "executable_sha256": "b" * 64,
                }
                for item in self.baseline["targets"]
            ],
        }

    def test_accepts_exact_budget_boundaries(self):
        tool.compare_measurements(self.current, self.baseline)

    def test_static_budget_targets_the_assets_embedded_by_dashboard_rs(self):
        self.assertEqual(tool.ASSETS, tool.ROOT / "src" / "dashboard_assets")
        self.assertGreater(
            sum(path.stat().st_size for path in tool.ASSETS.iterdir() if path.suffix in {".css", ".js"}),
            0,
        )

    def test_manual_accessibility_checklist_retains_named_release_inputs(self):
        checklist = tool.MANUAL_ACCESSIBILITY.read_text(encoding="utf-8")
        tool.validate_manual_accessibility_checklist(checklist)
        self.assertIn("Status: not run", checklist)
        self.assertNotIn("Status: pass", checklist)

    def test_rejects_each_budget_above_boundary(self):
        cases = []
        for field, value in (("asset_bytes", 250 * 1024 + 1), ("resolved_package_count", self.baseline["resolved_package_count"] + 61)):
            changed = json.loads(json.dumps(self.current))
            changed[field] = value
            cases.append(changed)
        for field in ("executable_bytes", "archive_bytes"):
            changed = json.loads(json.dumps(self.current))
            frozen = self.baseline["targets"][0]
            changed["targets"][0][field] = int(frozen[field] * 1.20) + 1
            cases.append(changed)
        for changed in cases:
            with self.subTest(changed=changed):
                with self.assertRaises(ValueError):
                    tool.compare_measurements(changed, self.baseline)

    def test_release_validation_fails_closed_until_complete_and_approved(self):
        manifest = tool.read_json(tool.TEMPLATE)
        tool.validate_manifest(manifest)
        with self.assertRaises(ValueError):
            tool.validate_manifest(manifest, release=True)
        manifest["browsers"] = {"safari": "26.0", "firefox-esr": "153", "chromium": "150"}
        manifest["artifacts"] = [
            {"target": target, "name": "tracky.tar.xz", "sha256": "a" * 64, "bytes": 1}
            for target in sorted(tool.TARGETS)
        ]
        manifest["measurements"] = {"latency": {}, "resources": {}, "sizes": {}}
        manifest["results"] = [
            {"gate": gate, "status": "pass", "evidence": "retained evidence"}
            for gate in sorted(tool.REQUIRED_RELEASE_GATES)
        ]
        manifest["responsible_maintainer"] = "maintainer"
        manifest["approval"] = {"approved": True, "approved_by": "maintainer"}
        tool.validate_manifest(manifest, release=True)

    def test_json_schema_and_ci_validator_share_contract_vocabulary(self):
        tool.validate_schema_contract(tool.read_json(tool.SCHEMA))

    def test_manifest_rejects_duplicate_supported_targets(self):
        manifest = tool.read_json(tool.TEMPLATE)
        manifest["targets"] = [manifest["targets"][0]] * len(tool.TARGETS)
        with self.assertRaisesRegex(ValueError, "exactly once"):
            tool.validate_manifest(manifest)

    def test_renderer_uses_validated_manifest_inputs(self):
        manifest = tool.read_json(tool.TEMPLATE)
        rendered = tool.render_manifest(manifest)
        self.assertIn("# Dashboard verification", rendered)
        self.assertIn("**evidence-foundation**", rendered)
        self.assertIn("## Tools", rendered)
        self.assertIn("## Targets", rendered)
        self.assertIn("## Browsers", rendered)
        self.assertIn("## Artifacts", rendered)
        self.assertIn("## Measurements", rendered)


if __name__ == "__main__":
    unittest.main()
