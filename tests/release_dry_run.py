import hashlib
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "release_identity.py"
SUMMARY_SCRIPT = ROOT / "scripts" / "release_dry_run_summary.py"
QUALITY_SCRIPT = ROOT / "scripts" / "release_quality_evidence.py"
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
    def test_unified_run_builds_each_native_target_once_and_publishes_only_after_gates(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        for required_input in (
            "accepted_sha:",
            "package_version:",
            "lockfile_sha256:",
            "prerelease_tag:",
        ):
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
        self.assertIn('"--target=$TARGET" --output-format=json', workflow)
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
        self.assertIn("actions: read", workflow)
        self.assertIn("actions/runs/$RUN_ID/jobs?filter=latest", workflow)
        self.assertIn("actions/runs/$RUN_ID/artifacts?per_page=100", workflow)
        self.assertIn("--workflow-run workflow-run.json", workflow)
        self.assertIn("--workflow-jobs workflow-jobs.json", workflow)
        self.assertIn("--workflow-artifacts workflow-artifacts.json", workflow)
        self.assertIn("release-dry-run-evidence.json", workflow)
        self.assertIn("release-dry-run-evidence.md", workflow)
        self.assertIn("scripts/release_quality_evidence.py", workflow)
        self.assertIn("release-dry-run-quality-evidence-", workflow)
        publish_job = workflow.split("\n  publish-prerelease:", 1)[1]
        self.assertIn("needs: [identity, summarize]", publish_job)
        self.assertIn("inputs.prerelease_tag != ''", publish_job)
        self.assertIn("github.event.repository.default_branch", publish_job)
        self.assertIn('ref: ${{ github.sha }}', publish_job)
        self.assertIn(
            'git merge-base --is-ancestor "$ACCEPTED_SHA" "$WORKFLOW_SHA"',
            publish_job,
        )
        self.assertNotIn(
            "ref: ${{ needs.identity.outputs.source_sha }}",
            publish_job,
        )
        self.assertIn("contents: write", publish_job)
        self.assertIn("scripts/release_prerelease.py", publish_job)
        self.assertIn("release-dry-run-verified-", publish_job)
        self.assertIn("release-dry-run-evidence-", publish_job)
        self.assertIn("release-prerelease-publication-", publish_job)
        self.assertNotIn("dist build", publish_job)
        self.assertNotIn("--clobber", publish_job)

        self.assertIn("EmbarkStudios/cargo-deny-action@", workflow)
        quality_source = QUALITY_SCRIPT.read_text(encoding="utf-8")
        for command_part in (
            '"fmt", "--all", "--", "--check"',
            '"test", "--locked", "--all-targets"',
            '"clippy",',
            '"scripts/dashboard_evidence.py", "check"',
        ):
            self.assertIn(command_part, quality_source)

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
            "git push",
            "actions/create-release",
        ):
            self.assertNotIn(forbidden, workflow)

        legacy_release = (
            ROOT / ".github" / "workflows" / "release.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("!contains(github.ref_name, '-rc.')", legacy_release)


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
                    "commands": [
                        "python3 scripts/dashboard_release_browser.py --lane chromium-latest",
                        "python3 scripts/dashboard_release_browser.py --lane firefox-latest",
                        "python3 scripts/dashboard_release_browser.py --lane safari-latest",
                    ],
                }
            ),
            encoding="utf-8",
        )
        self.browser_results = self.root / "browser-results"
        self.browser_results.mkdir()
        for lane, (browser, _) in sorted(summary.browser_contract.LANES.items()):
            if lane not in summary.browser_contract.CURRENT_LANES:
                continue
            version = {
                "safari-latest": "26.3",
                "firefox-latest": "154",
                "chromium-latest": "151",
            }[lane]
            (self.browser_results / (lane + ".json")).write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "lane": lane,
                        "commit": self.commit,
                        "lockfile_sha256": self.identity["lockfile_sha256"],
                        "browser": {"name": browser, "version": version},
                        "driver": {"name": browser + "driver", "version": version},
                        "command": "python3 scripts/dashboard_release_browser.py --lane " + lane,
                        "gates": [
                            {"gate": gate, "status": "pass"}
                            for gate in summary.browser_contract.GATES
                        ],
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
            runtime.write_text(
                json.dumps(
                    {
                        "target": target,
                        "commands": ["tracky dashboard --db <sandbox>/ledger-fixture.sqlite"],
                        "latency": {
                            "readiness_p95_ms": 1,
                            "initial_snapshot_p95_ms": 1,
                            "refresh_p95_ms": 1,
                            "navigation_p95_ms": 1,
                            "drill_down_p95_ms": 1,
                            "filter_interaction_p95_ms": 1,
                            "warmups": 5,
                            "runs": 30,
                        },
                        "resources": {
                            "idle_rss_bytes": 1,
                            "peak_rss_bytes": 1,
                            "idle_cpu_percent": 0,
                            "threads": 1,
                            "descriptors": 1,
                            "cycles": 100,
                            "descriptor_growth": 0,
                            "memory_growth_bytes": 0,
                            "memory_growth_percent": 0,
                        },
                    }
                ),
                encoding="utf-8",
            )
            (candidate / "native-build-evidence.json").write_text(
                json.dumps(
                    summary.command_contract.assemble(
                        self.identity,
                        "native-build",
                        target,
                        [
                            "dist build --artifacts=local --target=" + target,
                            "dist manifest --artifacts=local --target=" + target,
                        ],
                    )
                ),
                encoding="utf-8",
            )
            (candidate.parent / "native-runtime-evidence.json").write_text(
                json.dumps(
                    summary.command_contract.assemble(
                        self.identity,
                        "native-runtime",
                        target,
                        [
                            "python3 scripts/dashboard_evidence.py validate-semantic",
                            "cargo test --locked --test dashboard_cli",
                            "python3 scripts/dashboard_candidate_runtime.py",
                        ],
                    )
                ),
                encoding="utf-8",
            )
        self.workflow_run, self.workflow_jobs, self.workflow_artifacts = (
            self.workflow_metadata()
        )
        self.quality_evidence = self.root / "quality-evidence.json"
        self.quality_evidence.write_text(
            json.dumps(
                summary.quality_contract.run_quality_gates(
                    self.identity,
                    execute=lambda command: None,
                    version_probe=lambda command: "version for " + command[0],
                )
            ),
            encoding="utf-8",
        )
        for name, value in (
            ("workflow-run.json", self.workflow_run),
            ("workflow-jobs.json", self.workflow_jobs),
            ("workflow-artifacts.json", self.workflow_artifacts),
        ):
            (self.root / name).write_text(json.dumps(value), encoding="utf-8")

    def workflow_metadata(self):
        run_id = 123456
        run_url = "https://github.com/yersonargotev/tracky/actions/runs/%s" % run_id
        started = datetime(2026, 7, 25, tzinfo=timezone.utc)
        names = [
            "Bind the release identity and target matrix",
            "Preserved release quality gates",
            *[
                "Build once (%s)" % target
                for target in sorted(summary.evidence.TARGETS)
            ],
            *[
                "Reuse and test (%s)" % target
                for target in sorted(summary.evidence.TARGETS)
            ],
            *[
                "Current browser (%s)" % lane
                for lane in sorted(summary.browser_contract.CURRENT_LANES)
            ],
            "Bind current browser evidence",
        ]
        jobs = []
        for index, name in enumerate(names, 1):
            job_started = started + timedelta(seconds=index)
            jobs.append(
                {
                    "id": 9000 + index,
                    "run_id": run_id,
                    "head_sha": self.commit,
                    "name": name,
                    "status": "completed",
                    "conclusion": "success",
                    "html_url": run_url + "/job/%s" % (9000 + index),
                    "started_at": job_started.isoformat().replace("+00:00", "Z"),
                    "completed_at": (job_started + timedelta(seconds=2))
                    .isoformat()
                    .replace("+00:00", "Z"),
                    "steps": [
                        {
                            "name": (
                                "Enforce dependency and license policy"
                                if name == "Preserved release quality gates"
                                else "Complete " + name
                            ),
                            "status": "completed",
                            "conclusion": "success",
                            "started_at": job_started.isoformat().replace(
                                "+00:00", "Z"
                            ),
                            "completed_at": (job_started + timedelta(seconds=1))
                            .isoformat()
                            .replace("+00:00", "Z"),
                        }
                    ],
                }
            )
        required_artifacts = [
            "release-dry-run-identity-%s" % self.commit,
            "release-dry-run-quality-evidence-%s" % self.commit,
            *[
                "release-dry-run-built-%s-%s" % (self.commit, target)
                for target in sorted(summary.evidence.TARGETS)
            ],
            *[
                "release-dry-run-verified-%s-%s" % (self.commit, target)
                for target in sorted(summary.evidence.TARGETS)
            ],
            *[
                "release-dry-run-current-browser-%s-%s" % (self.commit, lane)
                for lane in sorted(summary.browser_contract.CURRENT_LANES)
            ],
            "release-dry-run-current-browser-evidence-%s" % self.commit,
        ]
        artifacts = []
        for index, name in enumerate(required_artifacts, 1):
            artifacts.append(
                {
                    "id": 8000 + index,
                    "name": name,
                    "size_in_bytes": 100 + index,
                    "digest": "sha256:" + ("%064x" % index),
                    "expired": False,
                    "url": "https://api.github.com/repos/yersonargotev/tracky/actions/artifacts/%s"
                    % (8000 + index),
                    "workflow_run": {"id": run_id, "head_sha": self.commit},
                }
            )
        return (
            {
                "id": run_id,
                "run_attempt": 1,
                "head_sha": self.commit,
                "html_url": run_url,
                "status": "in_progress",
                "conclusion": None,
                "run_started_at": started.isoformat().replace("+00:00", "Z"),
            },
            {"total_count": len(jobs), "jobs": jobs},
            {"total_count": len(artifacts), "artifacts": artifacts},
        )

    def assemble(self):
        return summary.assemble(
            self.verified,
            self.identity,
            self.quality_evidence,
            self.browser_evidence,
            self.browser_results,
            self.workflow_run,
            self.workflow_jobs,
            self.workflow_artifacts,
            "yersonargotev/tracky",
        )

    def tearDown(self):
        self.temp.cleanup()

    def test_assembles_both_exact_native_bundles_deterministically(self):
        value = self.assemble()
        self.assertEqual(value["mode"], "dry-run")
        self.assertFalse(value["published"])
        self.assertNotIn("approval", value)
        self.assertNotIn("responsible_maintainer", value)
        serialized = json.dumps(value)
        self.assertNotIn("approved_by", serialized)
        self.assertNotIn("responsible_maintainer", serialized)
        self.assertEqual(value["workflow"]["run_id"], 123456)
        self.assertTrue(value["jobs"])
        self.assertTrue(value["gates"])
        self.assertTrue(value["commands"])
        self.assertTrue(value["tools"])
        self.assertTrue(value["measurements"])
        self.assertEqual(
            set(value["native_command_evidence"]),
            summary.evidence.TARGETS,
        )
        self.assertEqual(
            {item["gate"] for item in value["quality_evidence"]["gates"]},
            summary.quality_contract.REQUIRED_GATES,
        )
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
            "--quality-evidence", str(self.quality_evidence),
            "--browser-evidence", str(self.browser_evidence),
            "--browser-results", str(self.browser_results),
            "--workflow-run", str(self.root / "workflow-run.json"),
            "--workflow-jobs", str(self.root / "workflow-jobs.json"),
            "--workflow-artifacts", str(self.root / "workflow-artifacts.json"),
            "--repository", "yersonargotev/tracky",
            "--output", str(output),
            "--markdown-output", str(self.root / "summary.md"),
        ])
        self.assertEqual(
            output.read_text(encoding="utf-8"),
            json.dumps(value, indent=2, sort_keys=True) + "\n",
        )
        markdown = (self.root / "summary.md").read_text(encoding="utf-8")
        self.assertIn("# Tracky automated release evidence", markdown)
        self.assertIn(self.commit, markdown)
        self.assertIn("## Preserved quality gates", markdown)
        self.assertIn("## Browser gate outcomes", markdown)
        self.assertIn("## Runtime measurements", markdown)
        self.assertIn("## Gates", markdown)

    def test_rejects_missing_or_substituted_native_bundles(self):
        target = sorted(summary.evidence.TARGETS)[0]
        directory = self.verified / (
            "release-dry-run-verified-%s-%s" % (self.commit, target)
        )
        archive = directory / "candidate" / ("tracky-%s.tar.xz" % target)
        archive.write_bytes(b"substituted")
        with self.assertRaisesRegex(ValueError, "checksum|size"):
            self.assemble()

        # Removing a bundle is covered without deleting operator files: move it
        # under a non-matching name inside the isolated temporary directory.
        directory.rename(directory.with_name("ignored-bundle"))
        with self.assertRaisesRegex(ValueError, "exactly"):
            self.assemble()

    def test_rejects_browser_evidence_from_another_identity(self):
        value = json.loads(self.browser_evidence.read_text(encoding="utf-8"))
        value["lockfile_sha256"] = "f" * 64
        self.browser_evidence.write_text(json.dumps(value), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "browser evidence lockfile"):
            self.assemble()

    def test_rejects_missing_failed_duplicate_stale_or_placeholder_run_evidence(self):
        mutations = (
            (
                lambda: self.workflow_jobs["jobs"].pop(),
                "job matrix|missing",
            ),
            (
                lambda: self.workflow_jobs["jobs"][0].update(conclusion="failure"),
                "successful",
            ),
            (
                lambda: self.workflow_jobs["jobs"].append(
                    dict(self.workflow_jobs["jobs"][0])
                ),
                "duplicated",
            ),
            (
                lambda: self.workflow_artifacts["artifacts"][0]["workflow_run"].update(
                    head_sha="f" * 40
                ),
                "artifact.*SHA|identity",
            ),
            (
                lambda: self.workflow_artifacts["artifacts"][0].update(
                    digest="not-recorded"
                ),
                "digest|placeholder",
            ),
        )
        for mutate, message in mutations:
            with self.subTest(message=message):
                self.workflow_run, self.workflow_jobs, self.workflow_artifacts = (
                    self.workflow_metadata()
                )
                mutate()
                with self.assertRaisesRegex(ValueError, message):
                    self.assemble()

    def test_rejects_duplicate_browser_results_and_retained_artifact_mismatch(self):
        duplicate = self.browser_results / "duplicate.json"
        duplicate.write_text(
            (self.browser_results / "safari-latest.json").read_text(
                encoding="utf-8"
            ),
            encoding="utf-8",
        )
        with self.assertRaisesRegex(ValueError, "exactly|duplicated"):
            self.assemble()
        duplicate.rename(self.root / "ignored-browser.json")

        verified_name = "release-dry-run-verified-%s-%s" % (
            self.commit,
            sorted(summary.evidence.TARGETS)[0],
        )
        retained = next(
            item
            for item in self.workflow_artifacts["artifacts"]
            if item["name"] == verified_name
        )
        retained["name"] = retained["name"] + "-substituted"
        with self.assertRaisesRegex(ValueError, "artifact matrix|missing"):
            self.assemble()

    def test_rejects_placeholder_tool_or_browser_driver_versions(self):
        target = sorted(summary.evidence.TARGETS)[0]
        semantic = (
            self.verified
            / ("release-dry-run-verified-%s-%s" % (self.commit, target))
            / "candidate"
            / "semantic-manifest.json"
        )
        original = json.loads(semantic.read_text(encoding="utf-8"))
        value = dict(original)
        value["tools"] = dict(original["tools"])
        value["tools"]["rust"] = "TODO: fill later"
        semantic.write_text(json.dumps(value), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "tool version.*placeholder"):
            self.assemble()

        semantic.write_text(json.dumps(original), encoding="utf-8")
        browser = self.browser_results / "safari-latest.json"
        value = json.loads(browser.read_text(encoding="utf-8"))
        value["driver"]["version"] = "not-recorded"
        browser.write_text(json.dumps(value), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "driver version.*placeholder"):
            self.assemble()

    def test_rejects_missing_or_failed_retained_quality_gates(self):
        original = json.loads(self.quality_evidence.read_text(encoding="utf-8"))
        missing = json.loads(json.dumps(original))
        missing["gates"].pop()
        self.quality_evidence.write_text(json.dumps(missing), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "gate matrix"):
            self.assemble()

        failed = json.loads(json.dumps(original))
        failed["gates"][0]["status"] = "fail"
        self.quality_evidence.write_text(json.dumps(failed), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "unsuccessful gate"):
            self.assemble()

    def test_rejects_placeholder_or_stale_native_command_evidence(self):
        target = sorted(summary.evidence.TARGETS)[0]
        path = (
            self.verified
            / ("release-dry-run-verified-%s-%s" % (self.commit, target))
            / "native-runtime-evidence.json"
        )
        value = json.loads(path.read_text(encoding="utf-8"))
        value["commands"][0] = "TODO: record the command"
        path.write_text(json.dumps(value), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "placeholder"):
            self.assemble()


if __name__ == "__main__":
    unittest.main()
