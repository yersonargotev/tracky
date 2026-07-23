#!/usr/bin/env python3

import copy
import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "dashboard_accessibility_evidence.py"
SPEC = importlib.util.spec_from_file_location("dashboard_accessibility_evidence", SCRIPT)
collector = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(collector)


class DashboardAccessibilityEvidenceTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.commit = "a" * 40
        self.lockfile = "b" * 64
        self.value = {
            "schema_version": 1,
            "commit": self.commit,
            "lockfile_sha256": self.lockfile,
            "candidate_run_id": 123456,
            "candidate_run_url": "https://github.com/yersonargotev/tracky/actions/runs/123456",
            "responsible_maintainer": "release-owner",
            "maintainer_sign_off": {"status": "pass", "signed_by": "release-owner", "signed_at": "2026-07-22T20:30:00Z"},
            "platforms": {
                name: self.platform(name)
                for name in sorted(collector.PLATFORMS)
            },
        }

    def tearDown(self):
        self.temp.cleanup()

    def platform(self, name):
        spec = collector.PLATFORMS[name]
        target = spec["target"]
        browser = spec["browser"]
        assistive = spec["assistive_technology"]
        checks = collector.COMMON_CHECKS | {spec["required_check"]}
        return {
            "environment": name,
            "target": target,
            "operating_system": "macOS 26.3" if browser == "Safari" else "Ubuntu 24.04",
            "browser": {"name": browser, "version": "26.3" if browser == "Safari" else "154.0"},
            "assistive_technology": {"name": assistive, "version": "26.3" if assistive == "VoiceOver" else "49.1"},
            "artifact": {"name": "tracky-%s.tar.xz" % target, "sha256": ("c" if browser == "Safari" else "d") * 64},
            "tester": "accessibility-tester",
            "date": "2026-07-22",
            "checks": [
                {"check": check, "status": "pass", "findings": "No issue found for %s." % check, "evidence": "Observed and recorded %s." % check}
                for check in sorted(checks)
            ],
            "sign_off": {"status": "pass", "signed_by": "accessibility-tester", "signed_at": "2026-07-22T20:00:00Z"},
        }

    def test_validates_and_finalizes_signed_exact_candidate_evidence(self):
        collector.check_contract()
        collector.validate_submission(self.value, self.commit, self.lockfile)
        canonical = collector.finalize(
            self.value,
            "https://github.com/yersonargotev/tracky/actions/runs/987654",
        )
        self.assertEqual(canonical["retained_run_url"], "https://github.com/yersonargotev/tracky/actions/runs/987654")
        self.assertEqual(canonical["status"], "pass")
        self.assertIn("Safari", collector.render(canonical))
        self.assertIn("VoiceOver", collector.render(canonical))
        self.assertIn("Firefox", collector.render(canonical))
        self.assertIn("Orca", collector.render(canonical))

    def test_rejects_stale_commit(self):
        with self.assertRaisesRegex(ValueError, "accepted commit"):
            collector.validate_submission(self.value, "e" * 40, self.lockfile)

    def test_rejects_schema_invalid_calendar_date(self):
        changed = copy.deepcopy(self.value)
        changed["platforms"]["safari-voiceover"]["date"] = "2026-99-99"
        with self.assertRaisesRegex(ValueError, "schema date format"):
            collector.validate_submission(changed, self.commit, self.lockfile)

    def test_rejects_boolean_schema_version(self):
        changed = copy.deepcopy(self.value)
        changed["schema_version"] = True
        with self.assertRaisesRegex(ValueError, "schema constant"):
            collector.validate_submission(changed, self.commit, self.lockfile)

    def test_rejects_incomplete_checks(self):
        changed = copy.deepcopy(self.value)
        changed["platforms"]["safari-voiceover"]["checks"].pop()
        with self.assertRaisesRegex(ValueError, "required checks|too few schema items"):
            collector.validate_submission(changed, self.commit, self.lockfile)

    def test_rejects_missing_required_platform(self):
        changed = copy.deepcopy(self.value)
        del changed["platforms"]["firefox-orca"]
        with self.assertRaisesRegex(ValueError, "both mandatory|missing schema-required fields"):
            collector.validate_submission(changed, self.commit, self.lockfile)

    def test_rejects_failed_checks(self):
        changed = copy.deepcopy(self.value)
        changed["platforms"]["firefox-orca"]["checks"][0]["status"] = "fail"
        with self.assertRaisesRegex(ValueError, "must pass"):
            collector.validate_submission(changed, self.commit, self.lockfile)

    def test_rejects_unsigned_and_placeholder_rows(self):
        cases = [
            ("sign_off", {"status": "pass", "signed_by": "", "signed_at": "2026-07-22T20:00:00Z"}, "signed"),
            ("tester", "TODO", "placeholder"),
        ]
        for field, value, message in cases:
            with self.subTest(field=field):
                changed = copy.deepcopy(self.value)
                changed["platforms"]["safari-voiceover"][field] = value
                with self.assertRaisesRegex(ValueError, message):
                    collector.validate_submission(changed, self.commit, self.lockfile)
        changed = copy.deepcopy(self.value)
        changed["platforms"]["firefox-orca"]["checks"][0]["findings"] = "not_run"
        with self.assertRaisesRegex(ValueError, "placeholder"):
            collector.validate_submission(changed, self.commit, self.lockfile)
        changed = copy.deepcopy(self.value)
        changed["maintainer_sign_off"]["signed_by"] = ""
        with self.assertRaisesRegex(ValueError, "maintainer must sign"):
            collector.validate_submission(changed, self.commit, self.lockfile)

    def test_verifies_both_downloaded_candidate_archives(self):
        candidates = self.root / "candidates"
        for name, platform in self.value["platforms"].items():
            target = platform["target"]
            directory = candidates / name
            directory.mkdir(parents=True)
            archive = directory / platform["artifact"]["name"]
            archive.write_bytes((name + " candidate").encode())
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            platform["artifact"]["sha256"] = digest
            (directory / (target + ".json")).write_text(json.dumps({
                "target": target,
                "commit": self.commit,
                "lockfile_sha256": self.lockfile,
                "artifact": {"name": archive.name, "sha256": digest, "bytes": archive.stat().st_size},
            }), encoding="utf-8")
        collector.verify_candidates(self.value, candidates)

    def test_workflow_has_protected_exact_sha_retention_boundary(self):
        workflow = (ROOT / ".github" / "workflows" / "dashboard-release-accessibility.yml").read_text(encoding="utf-8")
        for text in (
            "accepted_sha", "Verify the checked-out commit", "environment: dashboard-release",
            "run-id:", "github-token:", "dashboard-release-candidate-",
            ".github/workflows/dashboard-release-candidate.yml", '"conclusion": "success"',
            "scripts/dashboard_accessibility_evidence.py", "retention-days: 90",
            "dashboard-accessibility-evidence-", "if-no-files-found: error",
        ):
            self.assertIn(text, workflow)


if __name__ == "__main__":
    unittest.main()
