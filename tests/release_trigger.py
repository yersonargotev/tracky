import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "scripts" / "release_trigger.py"
SPEC = importlib.util.spec_from_file_location("release_trigger", SCRIPT)
trigger = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(trigger)


class ReleaseTriggerTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(
            ["git", "-C", str(self.root), "config", "user.email", "test@example.invalid"],
            check=True,
        )
        subprocess.run(
            ["git", "-C", str(self.root), "config", "user.name", "Tracky Test"],
            check=True,
        )
        self.before = self.commit_version("1.2.3")
        self.unchanged = self.commit_version("1.2.3", extra="description = \"changed\"\n")
        self.changed = self.commit_version("1.2.4")

    def tearDown(self):
        self.temp.cleanup()

    def commit_version(self, version, extra=""):
        (self.root / "Cargo.toml").write_text(
            '[package]\nname = "tracky"\nversion = "%s"\n%s' % (version, extra),
            encoding="utf-8",
        )
        subprocess.run(["git", "-C", str(self.root), "add", "Cargo.toml"], check=True)
        subprocess.run(["git", "-C", str(self.root), "commit", "-qm", version], check=True)
        return subprocess.check_output(
            ["git", "-C", str(self.root), "rev-parse", "HEAD"], text=True
        ).strip()

    def test_main_push_runs_only_for_a_parsed_package_version_change(self):
        self.assertEqual(
            trigger.release_trigger(self.root, "push", self.before),
            {"run_dag": True, "publish": True, "reason": "version-changed"},
        )
        subprocess.run(
            ["git", "-C", str(self.root), "checkout", "-q", self.unchanged],
            check=True,
        )
        self.assertEqual(
            trigger.release_trigger(self.root, "push", self.before),
            {"run_dag": False, "publish": False, "reason": "version-unchanged"},
        )

    def test_pull_request_validates_and_manual_dispatch_publishes(self):
        self.assertEqual(
            trigger.release_trigger(self.root, "pull_request"),
            {"run_dag": True, "publish": False, "reason": "pull-request"},
        )
        self.assertEqual(
            trigger.release_trigger(self.root, "workflow_dispatch"),
            {"run_dag": True, "publish": True, "reason": "manual-recovery"},
        )

    def test_rejects_unsupported_events_and_invalid_push_identity(self):
        with self.assertRaisesRegex(ValueError, "unsupported"):
            trigger.release_trigger(self.root, "schedule")
        with self.assertRaisesRegex(ValueError, "before SHA"):
            trigger.release_trigger(self.root, "push", "short")


if __name__ == "__main__":
    unittest.main()
