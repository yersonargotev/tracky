import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "release_command_evidence.py"
SPEC = importlib.util.spec_from_file_location("release_command_evidence", SCRIPT)
command_evidence = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(command_evidence)


class ReleaseCommandEvidenceTest(unittest.TestCase):
    def setUp(self):
        self.identity = {
            "schema_version": 1,
            "source_sha": "a" * 40,
            "package_version": "1.2.3",
            "lockfile_sha256": "b" * 64,
        }

    def test_records_successful_exact_identity_native_commands(self):
        for job_type in sorted(command_evidence.JOB_TYPES):
            with self.subTest(job_type=job_type):
                value = command_evidence.assemble(
                    self.identity,
                    job_type,
                    "aarch64-apple-darwin",
                    ["dist build --target=aarch64-apple-darwin", "cargo test"],
                )
                self.assertEqual(value["identity"], self.identity)
                self.assertEqual(value["job_type"], job_type)
                self.assertEqual(value["status"], "pass")
                self.assertEqual(len(value["commands"]), 2)

    def test_cli_reads_line_delimited_commands_deterministically(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            identity = root / "identity.json"
            commands = root / "commands.txt"
            output = root / "evidence.json"
            identity.write_text(json.dumps(self.identity), encoding="utf-8")
            commands.write_text("first command\nsecond command\n", encoding="utf-8")
            command_evidence.main(
                [
                    "--identity",
                    str(identity),
                    "--job-type",
                    "native-runtime",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "--commands-file",
                    str(commands),
                    "--output",
                    str(output),
                ]
            )
            self.assertEqual(
                json.loads(output.read_text(encoding="utf-8"))["commands"],
                ["first command", "second command"],
            )

    def test_rejects_missing_duplicate_stale_or_placeholder_commands(self):
        cases = (
            ([], "commands"),
            (["cargo test", "cargo test"], "duplicated"),
            (["TODO: record command"], "placeholder"),
        )
        for commands, message in cases:
            with self.subTest(message=message):
                with self.assertRaisesRegex(ValueError, message):
                    command_evidence.assemble(
                        self.identity,
                        "native-build",
                        "aarch64-apple-darwin",
                        commands,
                    )
        with self.assertRaisesRegex(ValueError, "identity"):
            command_evidence.assemble(
                dict(self.identity, source_sha="short"),
                "native-build",
                "aarch64-apple-darwin",
                ["cargo test"],
            )


if __name__ == "__main__":
    unittest.main()
