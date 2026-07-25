import copy
import hashlib
import importlib.util
import json
import tempfile
import unittest
from unittest import mock
from pathlib import Path


SPEC = importlib.util.spec_from_file_location(
    "release_homebrew",
    Path(__file__).parents[1] / "scripts" / "release_homebrew.py",
)
homebrew = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(homebrew)


class Fixture:
    repository = "owner/tracky"
    tag = "v0.2.4"
    source_sha = "a" * 40
    version = "0.2.4"
    lockfile = "b" * 64

    def __init__(self, root):
        self.root = root
        self.assets = root / "assets"
        self.assets.mkdir()
        artifacts = []
        for target in sorted(homebrew.dashboard.TARGETS):
            name = "tracky-%s.tar.xz" % target
            archive = self.assets / name
            archive.write_bytes(("archive-" + target).encode())
            archive_sha = hashlib.sha256(archive.read_bytes()).hexdigest()
            checksum = self.assets / (name + ".sha256")
            checksum.write_text("%s  %s\n" % (archive_sha, name))
            artifacts.append({
                "target": target,
                "archive_name": name,
                "archive_bytes": archive.stat().st_size,
                "archive_sha256": archive_sha,
                "semantic_manifest": {
                    "schema_version": 1,
                    "source_sha": self.source_sha,
                    "lockfile_sha256": self.lockfile,
                    "target": target,
                    "cargo_dist_manifest_sha256": "c" * 64,
                    "package_version": self.version,
                    "tools": {
                        "cargo": "cargo 1",
                        "cargo-dist": "cargo-dist 1",
                        "rust": "rustc 1",
                    },
                    "files": [
                        {
                            "path": path,
                            "bytes": 1,
                            "sha256": "d" * 64,
                            "mode": "0755" if path == "tracky" else "0644",
                        }
                        for path in sorted(homebrew.dashboard.REQUIRED_ARCHIVE_FILES)
                    ],
                    "transport": {
                        "archive_name": name,
                        "archive_bytes": archive.stat().st_size,
                        "archive_sha256": archive_sha,
                    },
                },
            })
        self.evidence_value = {
            "schema_version": 2,
            "source_sha": self.source_sha,
            "package_version": self.version,
            "lockfile_sha256": self.lockfile,
            "mode": "release",
            "published": False,
            "gates": [
                {"gate": gate, "status": "pass"}
                for gate in sorted(homebrew.dashboard.REQUIRED_RELEASE_GATES)
            ],
            "artifacts": artifacts,
        }
        (self.assets / "release-evidence.json").write_text(
            json.dumps(self.evidence_value)
        )
        (self.assets / "release-evidence.md").write_text(
            "# Exact release evidence\n\n"
            "`tracky --db <sandbox>/ledger.sqlite --help`\n"
        )
        self.release_value = {
            "id": 7,
            "draft": False,
            "prerelease": False,
            "tag_name": self.tag,
            "target_commitish": self.source_sha,
            "html_url": (
                "https://github.com/%s/releases/tag/%s"
                % (self.repository, self.tag)
            ),
            "assets": [],
        }
        self.refresh_remote()
        self.release_path = root / "release.json"
        self.write_release()

    def refresh_remote(self):
        self.release_value["assets"] = [
            {
                "id": index,
                "name": path.name,
                "state": "uploaded",
                "size": path.stat().st_size,
                "digest": "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest(),
                "browser_download_url": (
                    "https://github.com/%s/releases/download/%s/%s"
                    % (self.repository, self.tag, path.name)
                ),
            }
            for index, path in enumerate(sorted(self.assets.iterdir()), start=1)
        ]

    def write_release(self):
        self.release_path.write_text(json.dumps(self.release_value))

    def prepare(self):
        return homebrew.prepare(
            self.release_path,
            self.assets,
            self.repository,
            self.tag,
            self.source_sha,
            self.version,
        )


class ReleaseHomebrewTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.fixture = Fixture(Path(self.temp.name))

    def tearDown(self):
        self.temp.cleanup()

    def test_renders_deterministic_formula_and_preparation_evidence(self):
        formula, evidence = self.fixture.prepare()
        self.assertIn('desc "Local-first, review-first personal finance CLI"', formula)
        self.assertIn('license "MIT"', formula)
        self.assertIn('version "0.2.4"', formula)
        self.assertIn("on_macos do", formula)
        self.assertIn("on_linux do", formula)
        self.assertIn("tracky 0.2.4", formula)
        self.assertNotIn("tracky 0.2.4-rc.1", formula)
        for target in homebrew.dashboard.TARGETS:
            name = "tracky-%s.tar.xz" % target
            self.assertIn(
                "https://github.com/%s/releases/download/%s/%s"
                % (self.fixture.repository, self.fixture.tag, name),
                formula,
            )
        self.assertEqual(evidence["formula_version"], "0.2.4")
        self.assertEqual(evidence["package_version"], "0.2.4")
        self.assertEqual(
            evidence["formula"]["sha256"],
            hashlib.sha256(formula.encode()).hexdigest(),
        )
        self.assertEqual(
            [item["target"] for item in evidence["archives"]],
            sorted(homebrew.dashboard.TARGETS),
        )
        self.assertNotRegex(json.dumps(evidence), homebrew.PLACEHOLDER)

    def test_rejects_release_identity_and_state_drift(self):
        mutations = (
            ("draft", True, "published"),
            ("prerelease", True, "stable"),
            ("tag_name", "v0.2.5", "tag"),
            ("target_commitish", "e" * 40, "target"),
            ("html_url", "https://example.com/release", "URL|placeholder"),
        )
        for key, changed, message in mutations:
            with self.subTest(key=key):
                original = self.fixture.release_value[key]
                self.fixture.release_value[key] = changed
                self.fixture.write_release()
                with self.assertRaisesRegex(ValueError, message):
                    self.fixture.prepare()
                self.fixture.release_value[key] = original
        self.fixture.write_release()

    def test_rejects_missing_extra_or_duplicate_assets(self):
        original = copy.deepcopy(self.fixture.release_value["assets"])
        cases = (
            original[:-1],
            original + [{
                "id": 99, "name": "extra", "state": "uploaded",
                "size": 1, "digest": "sha256:" + "f" * 64,
            }],
            original + [copy.deepcopy(original[0])],
        )
        for assets in cases:
            with self.subTest(count=len(assets)):
                self.fixture.release_value["assets"] = assets
                self.fixture.write_release()
                with self.assertRaisesRegex(ValueError, "missing|unexpected|duplicated"):
                    self.fixture.prepare()
        self.fixture.release_value["assets"] = original
        self.fixture.write_release()
        (self.fixture.assets / "unexpected.txt").write_text("extra")
        with self.assertRaisesRegex(ValueError, "downloaded.*unexpected"):
            self.fixture.prepare()

    def test_rejects_remote_metadata_and_downloaded_byte_drift(self):
        remote = self.fixture.release_value["assets"][0]
        original = copy.deepcopy(remote)
        for key, changed, message in (
            ("size", remote["size"] + 1, "size"),
            ("digest", "sha256:" + "e" * 64, "digest"),
            ("state", "starter", "uploaded"),
            (
                "browser_download_url",
                "https://invalid.test/asset",
                "URL|placeholder",
            ),
        ):
            remote[key] = changed
            self.fixture.write_release()
            with self.assertRaisesRegex(ValueError, message):
                self.fixture.prepare()
            remote.clear()
            remote.update(original)
        self.fixture.write_release()
        path = self.fixture.assets / remote["name"]
        path.write_bytes(b"substituted")
        with self.assertRaisesRegex(ValueError, "size|digest"):
            self.fixture.prepare()

    def test_rejects_checksum_evidence_and_semantic_drift(self):
        checksum = next(self.fixture.assets.glob("*.sha256"))
        original_checksum = checksum.read_bytes()
        checksum.write_bytes(b"invalid checksum\n")
        self.fixture.refresh_remote()
        self.fixture.write_release()
        with self.assertRaisesRegex(ValueError, "checksum"):
            self.fixture.prepare()
        checksum.write_bytes(original_checksum)

        semantic = self.fixture.evidence_value["artifacts"][0]["semantic_manifest"]
        semantic["lockfile_sha256"] = "e" * 64
        evidence_path = self.fixture.assets / "release-evidence.json"
        evidence_path.write_text(json.dumps(self.fixture.evidence_value))
        self.fixture.refresh_remote()
        self.fixture.write_release()
        with self.assertRaisesRegex(ValueError, "semantic lockfile"):
            self.fixture.prepare()

    def test_rejects_placeholders(self):
        self.fixture.release_value["body"] = "TODO"
        self.fixture.write_release()
        with self.assertRaisesRegex(ValueError, "placeholder"):
            self.fixture.prepare()

    def test_verification_rejects_formula_or_evidence_drift(self):
        formula, value = self.fixture.prepare()
        formula_path = Path(self.temp.name) / "Formula" / "tracky.rb"
        formula_path.parent.mkdir()
        formula_path.write_text(formula)
        evidence_path = Path(self.temp.name) / "homebrew.json"
        evidence_path.write_text(json.dumps(value))
        verified = homebrew.verify(
            formula_path, evidence_path, self.fixture.release_path,
            self.fixture.assets, self.fixture.repository, self.fixture.tag,
            self.fixture.source_sha, self.fixture.version,
        )
        self.assertEqual(verified, value)
        formula_path.write_text(formula + "# drift\n")
        with self.assertRaisesRegex(ValueError, "formula differs"):
            homebrew.verify(
                formula_path, evidence_path, self.fixture.release_path,
                self.fixture.assets, self.fixture.repository, self.fixture.tag,
                self.fixture.source_sha, self.fixture.version,
            )

    def test_tap_reconciliation_create_update_noop_and_race(self):
        self.assertEqual(homebrew.reconcile_tap(None, "new"), "create")
        self.assertEqual(homebrew.reconcile_tap("old", "new"), "update")
        self.assertEqual(homebrew.reconcile_tap("new", "new"), "noop")
        self.assertEqual(
            homebrew.reconcile_tap("old", "new", expected_previous="old"),
            "update",
        )
        with self.assertRaisesRegex(ValueError, "changed"):
            homebrew.reconcile_tap("raced", "new", expected_previous="old")
        with self.assertRaisesRegex(ValueError, "disappeared"):
            homebrew.reconcile_tap(None, "new", expected_previous="old")

    def test_cli_prepare_and_verify(self):
        formula = Path(self.temp.name) / "out" / "Formula" / "tracky.rb"
        evidence = Path(self.temp.name) / "out" / "homebrew.json"
        common = [
            "--release", str(self.fixture.release_path),
            "--assets-root", str(self.fixture.assets),
            "--repository", self.fixture.repository,
            "--tag", self.fixture.tag,
            "--source-sha", self.fixture.source_sha,
            "--package-version", self.fixture.version,
            "--formula", str(formula),
            "--evidence", str(evidence),
        ]
        self.assertEqual(homebrew.main(common), 0)
        self.assertTrue(formula.is_file())
        self.assertEqual(homebrew.main(common + ["--verify"]), 0)

    def test_cli_reconciles_the_formula_used_by_the_workflow(self):
        existing = Path(self.temp.name) / "tap" / "Formula" / "tracky.rb"
        desired = Path(self.temp.name) / "prepared" / "Formula" / "tracky.rb"
        desired.parent.mkdir(parents=True)
        desired.write_text("desired\n")
        arguments = [
            "--reconcile-existing", str(existing),
            "--reconcile-desired", str(desired),
        ]
        with mock.patch("builtins.print") as output:
            self.assertEqual(homebrew.main(arguments), 0)
            output.assert_called_once_with("create")
        existing.parent.mkdir(parents=True)
        existing.write_text("desired\n")
        with mock.patch("builtins.print") as output:
            self.assertEqual(homebrew.main(arguments), 0)
            output.assert_called_once_with("noop")


if __name__ == "__main__":
    unittest.main()
