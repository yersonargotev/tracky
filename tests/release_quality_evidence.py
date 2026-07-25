import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "release_quality_evidence.py"
SPEC = importlib.util.spec_from_file_location("release_quality_evidence", SCRIPT)
quality = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(quality)


class ReleaseQualityEvidenceTest(unittest.TestCase):
    def setUp(self):
        self.identity = {
            "schema_version": 1,
            "source_sha": "a" * 40,
            "package_version": "1.2.3",
            "lockfile_sha256": "b" * 64,
        }

    def test_executes_one_canonical_command_catalog_and_records_success(self):
        executed = []

        def execute(command):
            executed.append(command)

        value = quality.run_quality_gates(
            self.identity,
            execute=execute,
            version_probe=lambda command: "version for " + command[0],
        )
        self.assertEqual(executed, [item["argv"] for item in quality.COMMANDS])
        self.assertEqual(value["identity"], self.identity)
        self.assertEqual(
            {item["gate"] for item in value["gates"]},
            quality.REQUIRED_GATES,
        )
        self.assertTrue(all(item["status"] == "pass" for item in value["gates"]))
        self.assertEqual(
            value["commands"],
            [quality.shell_join(item["argv"]) for item in quality.COMMANDS],
        )
        self.assertTrue(value["tools"])

    def test_cli_writes_only_after_every_command_succeeds(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            identity = root / "identity.json"
            output = root / "quality.json"
            identity.write_text(json.dumps(self.identity), encoding="utf-8")

            calls = []

            def fail_second(command):
                calls.append(command)
                if len(calls) == 2:
                    raise subprocess.CalledProcessError(1, command)

            with self.assertRaises(subprocess.CalledProcessError):
                quality.main(
                    ["--identity", str(identity), "--output", str(output)],
                    execute=fail_second,
                    version_probe=lambda command: "version",
                )
            self.assertFalse(output.exists())

    def test_rejects_stale_or_placeholder_identity_and_tool_versions(self):
        stale = dict(self.identity, source_sha="short")
        with self.assertRaisesRegex(ValueError, "identity"):
            quality.run_quality_gates(
                stale,
                execute=lambda command: None,
                version_probe=lambda command: "version",
            )
        with self.assertRaisesRegex(ValueError, "tool version"):
            quality.run_quality_gates(
                self.identity,
                execute=lambda command: None,
                version_probe=lambda command: "not-recorded",
            )


if __name__ == "__main__":
    unittest.main()
