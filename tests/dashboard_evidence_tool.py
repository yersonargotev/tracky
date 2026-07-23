import importlib.util
import hashlib
import json
import struct
import tarfile
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
                if item["target"] in tool.TARGETS
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
        manifest["browsers"] = {
            "safari-minimum": "26.0",
            "safari-latest": "26.1",
            "firefox-esr-minimum": "153 ESR",
            "firefox-latest": "154",
            "chromium-minimum": "150",
            "chromium-latest": "151",
        }
        manifest["artifacts"] = [
            {
                "target": target,
                "name": "tracky-%s.tar.xz" % target,
                "sha256": "a" * 64,
                "bytes": 1,
            }
            for target in sorted(tool.TARGETS)
        ]
        manifest["measurements"] = {
            "latency": {
                target: {
                    "warmups": 5,
                    "runs": 30,
                    **{name: limit for name, limit in tool.LATENCY_LIMITS_MS.items()},
                }
                for target in tool.TARGETS
            },
            "resources": {
                target: {
                    **{name: limit for name, limit in tool.RESOURCE_LIMITS.items()},
                    "cycles": 100,
                    "descriptor_growth": 0,
                    "memory_growth_bytes": 8 * 1024 * 1024,
                    "memory_growth_percent": 6,
                }
                for target in tool.TARGETS
            },
            "sizes": {
                "schema_version": 1,
                "resolved_package_count": self.baseline["resolved_package_count"],
                "asset_bytes": 0,
                "targets": [
                    {
                        "target": target,
                        "archive_bytes": 1,
                        "archive_sha256": "a" * 64,
                        "executable_bytes": 1,
                        "executable_sha256": "b" * 64,
                    }
                    for target in sorted(tool.TARGETS)
                ],
            },
        }
        manifest["results"] = [
            {
                "gate": gate,
                "status": "pass",
                "evidence": "https://github.com/yersonargotev/tracky/actions/runs/%d#%s"
                % (index, gate),
            }
            for index, gate in enumerate(sorted(tool.REQUIRED_RELEASE_GATES), start=1)
        ]
        manifest["responsible_maintainer"] = "maintainer"
        manifest["approval"] = {"approved": True, "approved_by": "maintainer"}
        tool.validate_manifest(
            manifest,
            release=True,
            expected_commit=manifest["commit"],
            expected_lockfile_sha256=manifest["lockfile_sha256"],
        )

        incomplete = json.loads(json.dumps(manifest))
        incomplete["measurements"]["latency"][next(iter(tool.TARGETS))] = {}
        with self.assertRaisesRegex(ValueError, "latency metrics"):
            tool.validate_manifest(
                incomplete,
                release=True,
                expected_commit=manifest["commit"],
                expected_lockfile_sha256=manifest["lockfile_sha256"],
            )

        with self.assertRaisesRegex(ValueError, "accepted commit"):
            tool.validate_manifest(
                manifest,
                release=True,
                expected_commit="f" * 40,
                expected_lockfile_sha256=manifest["lockfile_sha256"],
            )
        with self.assertRaisesRegex(ValueError, "accepted lockfile"):
            tool.validate_manifest(
                manifest,
                release=True,
                expected_commit=manifest["commit"],
                expected_lockfile_sha256="f" * 64,
            )

        placeholder = json.loads(json.dumps(manifest))
        placeholder["results"][0]["evidence"] = "https://example.invalid/retained"
        with self.assertRaisesRegex(ValueError, "Tracky Actions evidence"):
            tool.validate_manifest(
                placeholder,
                release=True,
                expected_commit=manifest["commit"],
                expected_lockfile_sha256=manifest["lockfile_sha256"],
            )

    def test_packaged_archive_requires_exact_files_checksum_and_executable_mode(self):
        target = "aarch64-apple-darwin"
        archive_name = "tracky-%s.tar.xz" % target
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            source = root / "source"
            source.mkdir()
            for name, content in {
                "tracky": b"\xcf\xfa\xed\xfe" + struct.pack("<I", 0x0100000C) + b"binary-header",
                "README.md": b"readme",
                "LICENSE": b"license",
                "THIRD-PARTY-NOTICES": b"notices",
            }.items():
                path = source / name
                path.write_bytes(content)
                path.chmod(0o755 if name == "tracky" else 0o644)
            archive = root / archive_name
            with tarfile.open(archive, "w:xz") as bundle:
                bundle.add(source, arcname="tracky-%s" % target)
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            (root / (archive_name + ".sha256")).write_text(
                "%s  %s\n" % (digest, archive_name), encoding="utf-8"
            )

            measured = tool.inspect_release_archive(archive, target, expected_root=source)
            self.assertEqual(measured["archive_contents"], sorted(tool.REQUIRED_ARCHIVE_FILES))
            tool.verify_dist_checksum(archive)
            tool.verify_packaged_size_measurement(measured, measured)
            placeholder = dict(measured)
            placeholder["executable_bytes"] = 1
            with self.assertRaisesRegex(ValueError, "executable_bytes"):
                tool.verify_packaged_size_measurement(placeholder, measured)

            (root / (archive_name + ".sha256")).write_text("0" * 64 + "  " + archive_name + "\n")
            with self.assertRaisesRegex(ValueError, "checksum"):
                tool.verify_dist_checksum(archive)

            with tarfile.open(archive, "w:xz") as bundle:
                for path in sorted(source.iterdir()):
                    bundle.add(path, arcname="wrong-root/%s" % path.name)
            with self.assertRaisesRegex(ValueError, "allowlist"):
                tool.inspect_release_archive(archive, target, expected_root=source)

    def test_release_workflow_blocks_publication_and_attaches_both_evidence_formats(self):
        workflow = (tool.ROOT / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8")
        self.assertIn("verify-dashboard-release:", workflow)
        self.assertIn("python3 scripts/dashboard_evidence.py validate --release", workflow)
        self.assertIn("dashboard-verification.json", workflow)
        self.assertIn("dashboard-verification.md", workflow)
        host = workflow.split("  host:", 1)[1].split("\n  publish-homebrew-formula:", 1)[0]
        self.assertIn("verify-dashboard-release", host)

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
