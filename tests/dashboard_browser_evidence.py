#!/usr/bin/env python3

import copy
import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "dashboard_browser_evidence.py"
SPEC = importlib.util.spec_from_file_location("dashboard_browser_evidence", SCRIPT)
collector = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(collector)
HARNESS_SCRIPT = ROOT / "scripts" / "dashboard_release_browser.py"
HARNESS_SPEC = importlib.util.spec_from_file_location("dashboard_release_browser", HARNESS_SCRIPT)
harness = importlib.util.module_from_spec(HARNESS_SPEC)
HARNESS_SPEC.loader.exec_module(harness)


class DashboardBrowserEvidenceTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.commit = "a" * 40
        self.lockfile = "b" * 64
        versions = {
            "safari-minimum": "26.0",
            "safari-latest": "26.3.1",
            "firefox-esr-minimum": "153.0.1",
            "firefox-latest": "154.0",
            "chromium-minimum": "150.0.1",
            "chromium-latest": "151.0.2",
        }
        for lane, (browser, _) in collector.LANES.items():
            self.write(lane, self.result(lane, browser, versions[lane]))

    def tearDown(self):
        self.temp.cleanup()

    def result(self, lane, browser, version):
        return {
            "schema_version": 1,
            "lane": lane,
            "commit": self.commit,
            "lockfile_sha256": self.lockfile,
            "browser": {"name": browser, "version": version},
            "driver": {"name": browser + "driver", "version": version},
            "command": "python3 scripts/dashboard_release_browser.py --lane " + lane,
            "gates": [
                {"gate": gate, "status": "pass"} for gate in sorted(collector.GATES)
            ],
        }

    def write(self, lane, value):
        (self.root / (lane + ".json")).write_text(json.dumps(value), encoding="utf-8")

    def test_assembles_six_exact_sha_lanes_deterministically(self):
        value = collector.assemble(self.root, self.commit, self.lockfile)
        self.assertEqual(set(value), {"commit", "lockfile_sha256", "browsers", "commands"})
        self.assertEqual(set(value["browsers"]), set(collector.LANES))
        self.assertEqual(value["commands"], sorted(value["commands"]))

    def test_rejects_missing_duplicate_and_unknown_lanes(self):
        (self.root / "safari-minimum.json").unlink()
        with self.assertRaisesRegex(ValueError, "exactly six"):
            collector.assemble(self.root, self.commit, self.lockfile)

    def test_rejects_commit_lockfile_version_and_gate_failures(self):
        path = self.root / "chromium-minimum.json"
        original = json.loads(path.read_text())
        cases = [
            ("commit", "c" * 40, "accepted commit"),
            ("lockfile_sha256", "d" * 64, "accepted lockfile"),
        ]
        for field, value, message in cases:
            with self.subTest(field=field):
                changed = copy.deepcopy(original)
                changed[field] = value
                self.write("chromium-minimum", changed)
                with self.assertRaisesRegex(ValueError, message):
                    collector.assemble(self.root, self.commit, self.lockfile)
        changed = copy.deepcopy(original)
        changed["browser"]["version"] = "149.9"
        self.write("chromium-minimum", changed)
        with self.assertRaisesRegex(ValueError, "below"):
            collector.assemble(self.root, self.commit, self.lockfile)
        changed = copy.deepcopy(original)
        changed["gates"][0]["status"] = "fail"
        self.write("chromium-minimum", changed)
        with self.assertRaisesRegex(ValueError, "failed browser gate"):
            collector.assemble(self.root, self.commit, self.lockfile)

    def test_release_workflow_declares_matrix_retention_and_exact_sha_binding(self):
        workflow = (ROOT / ".github" / "workflows" / "dashboard-release-browsers.yml").read_text(encoding="utf-8")
        for lane in collector.LANES:
            self.assertIn("lane: " + lane, workflow)
        self.assertIn("workflow_dispatch:", workflow)
        self.assertIn("accepted_sha", workflow)
        self.assertIn("Verify the checked-out commit", workflow)
        self.assertIn("retention-days: 90", workflow)
        self.assertIn("if-no-files-found: error", workflow)
        self.assertIn("dashboard-browser-evidence-", workflow)
        self.assertIn("scripts/dashboard_release_browser.py", workflow)
        self.assertIn("scripts/dashboard_browser_evidence.py", workflow)
        lane_job = workflow.split("  browser-lane:", 1)[1].split("\n  collect:", 1)[0]
        self.assertLess(lane_job.index("Initialize fail-closed raw lane output"), lane_job.index("Download the packaged candidate"))
        self.assertIn("shasum -a 256 --check", lane_job)
        self.assertIn(
            "browser-actions/setup-firefox@0bc507ddf224827e3b1af68e014d5e42ab93e795",
            lane_job,
        )
        self.assertIn(
            "browser-actions/setup-chrome@2e1d749697dd1612b833dba4a722266286fbefcd",
            lane_job,
        )

    def test_canonical_drawer_interaction_uses_an_async_open_state_probe(self):
        class Driver:
            def async_script(self, script):
                self.script = script
                return True

        driver = Driver()
        self.assertTrue(harness.open_canonical_drawer(driver))
        self.assertIn("[data-drawer]", driver.script)
        self.assertIn(".open", driver.script)
        self.assertIn("setInterval", driver.script)
        self.assertIn("done(false)", driver.script)

    def test_responsive_probe_ignores_hidden_buttons(self):
        class Driver:
            def script(self, script):
                self.script_source = script
                return {"width": 320, "scroll": 320, "undersized_visible_buttons": 0, "storage": 0, "history": 2}

        driver = Driver()
        self.assertEqual(harness.responsive_state(driver)["undersized_visible_buttons"], 0)
        self.assertIn("getClientRects().length", driver.script_source)

    def test_progressive_content_tracks_the_refreshed_snapshot(self):
        content = '700000 COP 2026-01-01 2026-07-31 COP <table data-region="alerts"'
        self.assertTrue(harness.has_progressive_content(content))
        self.assertFalse(harness.has_progressive_content(content.replace("700000 COP", "500000 COP")))

    def test_harness_writes_failed_raw_gate_and_returns_nonzero(self):
        binary = self.root / "tracky"
        driver = self.root / "driver"
        binary.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        driver.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        binary.chmod(0o755)
        driver.chmod(0o755)
        output = self.root / "failed.json"
        argv = [
            "dashboard_release_browser.py", "--binary", str(binary), "--browser", "chromium",
            "--driver", str(driver), "--lane", "chromium-minimum", "--minimum-version", "150",
            "--commit", self.commit, "--lockfile-sha256", self.lockfile, "--output", str(output),
        ]
        with contextlib.redirect_stderr(io.StringIO()), mock.patch.object(
            harness.sys, "argv", argv
        ), mock.patch.object(harness, "seed", side_effect=RuntimeError("browser-flow: synthetic failure")):
            self.assertEqual(harness.main(), 1)
        value = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(value["gates"][0]["status"], "fail")
        self.assertIn("synthetic failure", value["gates"][0]["error"])


if __name__ == "__main__":
    unittest.main()
