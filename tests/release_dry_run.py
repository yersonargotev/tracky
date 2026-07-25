import hashlib
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "release_identity.py"
SUMMARY_SCRIPT = ROOT / "scripts" / "release_dry_run_summary.py"
WORKFLOW = ROOT / ".github" / "workflows" / "release-dry-run.yml"
BROWSER_ACTION = (
    ROOT / ".github" / "actions" / "dashboard-browser-lane" / "action.yml"
)
SPEC = importlib.util.spec_from_file_location("release_identity", SCRIPT)
identity = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(identity)
SUMMARY_SPEC = importlib.util.spec_from_file_location("release_dry_run_summary", SUMMARY_SCRIPT)
summary = importlib.util.module_from_spec(SUMMARY_SPEC)
SUMMARY_SPEC.loader.exec_module(summary)


class ReleaseIdentityTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / "Cargo.toml").write_text(
            '[package]\nname = "tracky"\nversion = "1.2.3"\n',
            encoding="utf-8",
        )
        (self.root / "Cargo.lock").write_text("locked\n", encoding="utf-8")
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(
            ["git", "-C", str(self.root), "config", "user.email", "test@example.invalid"],
            check=True,
        )
        subprocess.run(
            ["git", "-C", str(self.root), "config", "user.name", "Tracky Test"],
            check=True,
        )
        subprocess.run(["git", "-C", str(self.root), "add", "Cargo.toml", "Cargo.lock"], check=True)
        subprocess.run(["git", "-C", str(self.root), "commit", "-qm", "fixture"], check=True)
        self.sha = subprocess.check_output(
            ["git", "-C", str(self.root), "rev-parse", "HEAD"],
            text=True,
        ).strip()
        self.lockfile = hashlib.sha256((self.root / "Cargo.lock").read_bytes()).hexdigest()

    def tearDown(self):
        self.temp.cleanup()

    def test_validates_and_records_the_exact_release_identity(self):
        value = identity.validate_release_identity(
            self.root,
            self.sha,
            "1.2.3",
            self.lockfile,
        )
        self.assertEqual(
            value,
            {
                "schema_version": 1,
                "source_sha": self.sha,
                "package_version": "1.2.3",
                "lockfile_sha256": self.lockfile,
            },
        )

        output = self.root / "identity.json"
        subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--source-root",
                str(self.root),
                "--accepted-sha",
                self.sha,
                "--package-version",
                "1.2.3",
                "--lockfile-sha256",
                self.lockfile,
                "--output",
                str(output),
            ],
            check=True,
        )
        self.assertEqual(
            output.read_text(encoding="utf-8"),
            json.dumps(value, indent=2, sort_keys=True) + "\n",
        )
        subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--source-root",
                str(self.root),
                "--derive",
                "--output",
                str(output),
            ],
            check=True,
        )
        self.assertEqual(json.loads(output.read_text(encoding="utf-8")), value)

    def test_rejects_invalid_or_mismatched_identity_fields(self):
        cases = [
            (("short", "1.2.3", self.lockfile), "40 lowercase"),
            (("f" * 40, "1.2.3", self.lockfile), "checked-out commit"),
            ((self.sha, "9.9.9", self.lockfile), "package version"),
            ((self.sha, "1.2.3", "f" * 64), "lockfile"),
        ]
        for arguments, message in cases:
            with self.subTest(message=message):
                with self.assertRaisesRegex(ValueError, message):
                    identity.validate_release_identity(self.root, *arguments)


class ReleaseDryRunWorkflowTest(unittest.TestCase):
    def test_unified_dry_run_builds_each_native_target_once_and_never_publishes(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        for required_input in ("accepted_sha:", "package_version:", "lockfile_sha256:"):
            self.assertIn(required_input, workflow)
        self.assertIn("pull_request:", workflow)
        self.assertIn("cancel-in-progress: false", workflow)
        self.assertIn("group: release-dry-run", workflow)
        self.assertEqual(workflow.count("dist build --artifacts=local"), 1)
        self.assertIn("aarch64-apple-darwin", workflow)
        self.assertIn("x86_64-unknown-linux-gnu", workflow)
        self.assertIn("scripts/release_identity.py", workflow)
        self.assertIn("scripts/release_dry_run_summary.py", workflow)
        self.assertIn("scripts/dashboard_evidence.py semantic-manifest", workflow)
        self.assertIn("scripts/dashboard_evidence.py validate-semantic", workflow)
        self.assertIn("actions/download-artifact@", workflow)
        self.assertIn("scripts/dashboard_candidate_runtime.py", workflow)
        self.assertIn("cargo test --locked --test dashboard_cli", workflow)
        self.assertIn(
            "dist manifest --artifacts=local --no-local-paths",
            workflow,
        )
        self.assertIn("--target=${{ matrix.target }} --output-format=json", workflow)
        for lane in ("safari-latest", "firefox-latest", "chromium-latest"):
            self.assertIn("lane: " + lane, workflow)
        browser_job = workflow.split("  browser-current:", 1)[1].split(
            "\n  collect-current-browsers:", 1
        )[0]
        self.assertNotIn("dist build", browser_job)
        self.assertIn("scripts/dashboard_evidence.py validate-semantic", browser_job)
        self.assertIn("release-dry-run-built-", browser_job)
        self.assertIn("uses: ./.github/actions/dashboard-browser-lane", browser_job)
        self.assertNotIn("setup-firefox@", browser_job)
        self.assertNotIn("setup-chrome@", browser_job)
        self.assertIn(
            "scripts/dashboard_release_browser.py",
            BROWSER_ACTION.read_text(encoding="utf-8"),
        )
        self.assertIn("--profile current", workflow)
        self.assertIn("current-browser-evidence.json", workflow)

        for gate in (
            "cargo fmt --all -- --check",
            "cargo test --locked --all-targets",
            "cargo clippy --locked --all-targets --all-features -- -D warnings",
            "EmbarkStudios/cargo-deny-action@",
            "scripts/dashboard_evidence.py check",
        ):
            self.assertIn(gate, workflow)

        for retained in (
            ".tar.xz",
            ".tar.xz.sha256",
            "dist-manifest.json",
            "semantic-manifest.json",
        ):
            self.assertIn(retained, workflow)

        for forbidden in (
            "environment:",
            "HOMEBREW_TAP_TOKEN",
            "gh release",
            "git push",
            "actions/create-release",
        ):
            self.assertNotIn(forbidden, workflow)


class ReleaseDryRunSummaryTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.commit = "a" * 40
        self.identity = {
            "schema_version": 1,
            "source_sha": self.commit,
            "package_version": "1.2.3",
            "lockfile_sha256": "b" * 64,
        }
        self.identity_path = self.root / "release-identity.json"
        self.identity_path.write_text(
            json.dumps(self.identity, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        self.browser_evidence = self.root / "current-browser-evidence.json"
        self.browser_evidence.write_text(
            json.dumps(
                {
                    "commit": self.commit,
                    "lockfile_sha256": self.identity["lockfile_sha256"],
                    "browsers": {
                        "safari-latest": "26.3",
                        "firefox-latest": "154",
                        "chromium-latest": "151",
                    },
                    "commands": ["current Safari", "current Firefox", "current Chromium"],
                }
            ),
            encoding="utf-8",
        )
        self.verified = self.root / "verified"
        for target in sorted(summary.evidence.TARGETS):
            candidate = (
                self.verified
                / ("release-dry-run-verified-%s-%s" % (self.commit, target))
                / "candidate"
            )
            candidate.mkdir(parents=True)
            archive = candidate / ("tracky-%s.tar.xz" % target)
            archive.write_bytes(("archive-" + target).encode())
            archive_hash = hashlib.sha256(archive.read_bytes()).hexdigest()
            (candidate / (archive.name + ".sha256")).write_text(
                "%s  %s\n" % (archive_hash, archive.name),
                encoding="utf-8",
            )
            dist_manifest = candidate / "dist-manifest.json"
            checksum_name = archive.name + ".sha256"
            dist_manifest.write_text(
                json.dumps({
                    "dist_version": "0.32.0",
                    "releases": [{
                        "app_name": "tracky",
                        "app_version": self.identity["package_version"],
                        "artifacts": [archive.name, checksum_name],
                    }],
                    "artifacts": {
                        archive.name: {
                            "name": archive.name,
                            "kind": "executable-zip",
                            "target_triples": [target],
                            "checksum": checksum_name,
                            "assets": [{"name": "tracky", "path": "tracky"}],
                        },
                        checksum_name: {
                            "name": checksum_name,
                            "kind": "checksum",
                            "target_triples": [target],
                        },
                    },
                }),
                encoding="utf-8",
            )
            semantic = {
                "schema_version": 1,
                "source_sha": self.commit,
                "lockfile_sha256": self.identity["lockfile_sha256"],
                "cargo_dist_manifest_sha256": hashlib.sha256(
                    dist_manifest.read_bytes()
                ).hexdigest(),
                "target": target,
                "package_version": self.identity["package_version"],
                "tools": {
                    "cargo": "cargo 1",
                    "cargo-dist": "cargo-dist 0.32.0",
                    "rust": "rustc 1",
                },
                "files": [
                    {
                        "path": name,
                        "bytes": 1,
                        "sha256": "c" * 64,
                        "mode": "0755" if name == "tracky" else "0644",
                    }
                    for name in sorted(summary.evidence.REQUIRED_ARCHIVE_FILES)
                ],
                "transport": {
                    "archive_name": archive.name,
                    "archive_bytes": archive.stat().st_size,
                    "archive_sha256": archive_hash,
                },
            }
            (candidate / "semantic-manifest.json").write_text(
                json.dumps(semantic, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            (candidate / "release-identity.json").write_text(
                self.identity_path.read_text(encoding="utf-8"),
                encoding="utf-8",
            )
            runtime = candidate.parent / "runtime-evidence.json"
            runtime.write_text(json.dumps({"target": target}), encoding="utf-8")

    def tearDown(self):
        self.temp.cleanup()

    def test_assembles_both_exact_native_bundles_deterministically(self):
        value = summary.assemble(
            self.verified, self.identity, self.browser_evidence
        )
        self.assertEqual(value["mode"], "dry-run")
        self.assertFalse(value["published"])
        self.assertEqual(
            set(value["browsers"]),
            {"safari-latest", "firefox-latest", "chromium-latest"},
        )
        self.assertEqual(
            [artifact["target"] for artifact in value["artifacts"]],
            sorted(summary.evidence.TARGETS),
        )

        output = self.root / "summary.json"
        summary.main([
            "--verified-root", str(self.verified),
            "--identity", str(self.identity_path),
            "--browser-evidence", str(self.browser_evidence),
            "--output", str(output),
        ])
        self.assertEqual(
            output.read_text(encoding="utf-8"),
            json.dumps(value, indent=2, sort_keys=True) + "\n",
        )

    def test_rejects_missing_or_substituted_native_bundles(self):
        target = sorted(summary.evidence.TARGETS)[0]
        directory = self.verified / (
            "release-dry-run-verified-%s-%s" % (self.commit, target)
        )
        archive = directory / "candidate" / ("tracky-%s.tar.xz" % target)
        archive.write_bytes(b"substituted")
        with self.assertRaisesRegex(ValueError, "checksum|size"):
            summary.assemble(self.verified, self.identity, self.browser_evidence)

        # Removing a bundle is covered without deleting operator files: move it
        # under a non-matching name inside the isolated temporary directory.
        directory.rename(directory.with_name("ignored-bundle"))
        with self.assertRaisesRegex(ValueError, "exactly"):
            summary.assemble(self.verified, self.identity, self.browser_evidence)

    def test_rejects_browser_evidence_from_another_identity(self):
        value = json.loads(self.browser_evidence.read_text(encoding="utf-8"))
        value["lockfile_sha256"] = "f" * 64
        self.browser_evidence.write_text(json.dumps(value), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "browser evidence lockfile"):
            summary.assemble(self.verified, self.identity, self.browser_evidence)


if __name__ == "__main__":
    unittest.main()
