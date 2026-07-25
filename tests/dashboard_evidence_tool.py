import importlib.util
import hashlib
import json
import lzma
import struct
import tarfile
import tempfile
import unittest
from io import BytesIO
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

    def test_release_contract_uses_automated_accessibility_only(self):
        self.assertIn("accessibility-automation", tool.REQUIRED_RELEASE_GATES)
        self.assertNotIn("manual-accessibility", tool.REQUIRED_RELEASE_GATES)
        self.assertFalse(
            (tool.ROOT / ".github" / "workflows" / "dashboard-release-accessibility.yml").exists()
        )

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
            source = self.make_package_source(root)
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

    def test_semantic_manifest_is_deterministic_across_archive_metadata(self):
        target = "aarch64-apple-darwin"
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            source = self.make_package_source(root)
            first = root / "first" / ("tracky-%s.tar.xz" % target)
            second = root / "second" / ("tracky-%s.tar.xz" % target)
            first.parent.mkdir()
            second.parent.mkdir()
            self.write_archive(first, source, target, mtime=1, reverse=False)
            self.write_archive(second, source, target, mtime=2, reverse=True)

            arguments = {
                "target": target,
                "source_sha": "a" * 40,
                "lockfile_sha256": "b" * 64,
                "cargo_dist_manifest_sha256": "c" * 64,
                "package_version": "0.2.3",
                "tools": {
                    "rust": "rustc 1.90.0",
                    "cargo": "cargo 1.90.0",
                    "cargo-dist": "cargo-dist 0.32.0",
                },
                "expected_root": source,
                "version_probe": lambda _: "tracky 0.2.3",
            }
            first_manifest = tool.semantic_archive_manifest(first, **arguments)
            second_manifest = tool.semantic_archive_manifest(second, **arguments)

            tool.validate_semantic_archive_manifest(first_manifest)
            self.assertEqual(
                tool.semantic_archive_identity(first_manifest),
                tool.semantic_archive_identity(second_manifest),
            )
            self.assertNotEqual(
                first_manifest["transport"]["archive_sha256"],
                second_manifest["transport"]["archive_sha256"],
            )
            self.assertNotEqual(
                first_manifest["transport"]["archive_bytes"],
                second_manifest["transport"]["archive_bytes"],
            )
            self.assertEqual(
                [item["path"] for item in first_manifest["files"]],
                sorted(tool.REQUIRED_ARCHIVE_FILES),
            )

            output = root / "semantic.json"
            dist_manifest = root / "dist-manifest.json"
            dist_manifest.write_text('{"artifacts":[]}\n', encoding="utf-8")
            original_probe = tool.packaged_version
            tool.packaged_version = lambda _: "tracky 0.2.3"
            try:
                tool.main([
                    "semantic-manifest",
                    "--archive", str(first),
                    "--target", target,
                    "--source-sha", "a" * 40,
                    "--lockfile-sha256", "b" * 64,
                    "--cargo-dist-manifest", str(dist_manifest),
                    "--package-version", "0.2.3",
                    "--rust-version", "rustc 1.90.0",
                    "--cargo-version", "cargo 1.90.0",
                    "--cargo-dist-version", "cargo-dist 0.32.0",
                    "--source-root", str(source),
                    "--output", str(output),
                ])
                tool.main([
                    "validate-semantic", str(output),
                    "--archive", str(first),
                    "--source-root", str(source),
                ])
            finally:
                tool.packaged_version = original_probe
            self.assertEqual(
                output.read_text(encoding="utf-8"),
                tool.canonical_json(tool.read_json(output)),
            )

    def test_semantic_manifest_rejects_adversarial_archives_and_provenance(self):
        target = "aarch64-apple-darwin"
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            source = self.make_package_source(root)

            def manifest(archive, **overrides):
                arguments = {
                    "target": target,
                    "source_sha": "a" * 40,
                    "lockfile_sha256": "b" * 64,
                    "cargo_dist_manifest_sha256": "c" * 64,
                    "package_version": "0.2.3",
                    "tools": {
                        "rust": "rustc 1.90.0",
                        "cargo": "cargo 1.90.0",
                        "cargo-dist": "cargo-dist 0.32.0",
                    },
                    "expected_root": source,
                    "version_probe": lambda _: "tracky 0.2.3",
                }
                arguments.update(overrides)
                return tool.semantic_archive_manifest(archive, **arguments)

            valid = root / "valid" / ("tracky-%s.tar.xz" % target)
            valid.parent.mkdir()
            self.write_archive(valid, source, target)
            with self.assertRaisesRegex(ValueError, "package version"):
                manifest(valid, version_probe=lambda _: "tracky 9.9.9")
            with self.assertRaisesRegex(ValueError, "source SHA"):
                manifest(valid, source_sha="short")
            with self.assertRaisesRegex(ValueError, "lockfile"):
                manifest(valid, lockfile_sha256="short")
            with self.assertRaisesRegex(ValueError, "Cargo Dist manifest"):
                manifest(valid, cargo_dist_manifest_sha256="short")

            cases = [
                ("unsafe.tar.xz", self.tar_entries(source, target) + [("../escape", b"x", 0o644, "file")], "unsafe path"),
                ("link.tar.xz", self.tar_entries(source, target) + [("tracky-%s/link" % target, b"", 0o777, "symlink")], "links"),
                ("special.tar.xz", self.tar_entries(source, target) + [("tracky-%s/device" % target, b"", 0o644, "fifo")], "non-file"),
                ("extra.tar.xz", self.tar_entries(source, target) + [("tracky-%s/extra" % target, b"x", 0o644, "file")], "allowlist"),
                ("missing.tar.xz", self.tar_entries(source, target)[:-1], "allowlist"),
            ]
            for name, entries, message in cases:
                with self.subTest(name=name):
                    archive = root / name
                    self.write_entries(archive, entries)
                    with self.assertRaisesRegex(ValueError, message):
                        manifest(archive)

            wrong_mode = root / "wrong-mode.tar.xz"
            entries = self.tar_entries(source, target)
            entries[0] = (entries[0][0], entries[0][1], 0o4755, entries[0][3])
            self.write_entries(wrong_mode, entries)
            with self.assertRaisesRegex(ValueError, "permissions"):
                manifest(wrong_mode)

            wrong_arch = root / "wrong-arch.tar.xz"
            entries = self.tar_entries(source, target)
            entries[0] = (entries[0][0], b"\x7fELF\x02\x01" + b"\0" * 14, 0o755, "file")
            self.write_entries(wrong_arch, entries)
            with self.assertRaisesRegex(ValueError, "Mach-O"):
                manifest(wrong_arch)

            accepted = manifest(valid)
            changed = root / "changed" / ("tracky-%s.tar.xz" % target)
            changed.parent.mkdir()
            entries = self.tar_entries(source, target)
            entries[0] = (
                entries[0][0],
                entries[0][1][:-1] + b"x",
                entries[0][2],
                entries[0][3],
            )
            self.write_entries(changed, entries)
            with self.assertRaisesRegex(ValueError, "archive (size|checksum)|archive files"):
                tool.verify_semantic_archive_manifest(
                    accepted,
                    changed,
                    expected_root=source,
                    version_probe=lambda _: "tracky 0.2.3",
                )

    @staticmethod
    def make_package_source(root):
        source = root / "source"
        source.mkdir()
        contents = {
            "tracky": b"\xcf\xfa\xed\xfe" + struct.pack("<I", 0x0100000C) + b"binary-header",
            "README.md": b"readme",
            "LICENSE": b"license",
            "THIRD-PARTY-NOTICES": b"notices",
        }
        for name, content in contents.items():
            path = source / name
            path.write_bytes(content)
            path.chmod(0o755 if name == "tracky" else 0o644)
        return source

    @staticmethod
    def tar_entries(source, target):
        names = ["tracky", "README.md", "LICENSE", "THIRD-PARTY-NOTICES"]
        return [
            (
                "tracky-%s/%s" % (target, name),
                (source / name).read_bytes(),
                0o755 if name == "tracky" else 0o644,
                "file",
            )
            for name in names
        ]

    def write_archive(self, archive, source, target, mtime=0, reverse=False):
        entries = self.tar_entries(source, target)
        if reverse:
            entries.reverse()
        self.write_entries(
            archive,
            entries,
            mtime=mtime,
            owner=1000 if reverse else 0,
            preset=9 if reverse else 0,
        )
        if reverse:
            archive.write_bytes(
                lzma.compress(
                    lzma.decompress(archive.read_bytes()) + (b"\0" * 512),
                    preset=9,
                )
            )

    @staticmethod
    def write_entries(archive, entries, mtime=0, owner=0, preset=6):
        with tarfile.open(archive, "w:xz", preset=preset) as bundle:
            root = tarfile.TarInfo(entries[0][0].split("/", 1)[0])
            root.type = tarfile.DIRTYPE
            root.mode = 0o755
            root.mtime = mtime
            root.uid = owner
            root.gid = owner
            bundle.addfile(root)
            for name, content, mode, kind in entries:
                member = tarfile.TarInfo(name)
                member.mode = mode
                member.mtime = mtime
                member.uid = owner
                member.gid = owner
                if kind == "symlink":
                    member.type = tarfile.SYMTYPE
                    member.linkname = "tracky"
                    bundle.addfile(member)
                elif kind == "fifo":
                    member.type = tarfile.FIFOTYPE
                    bundle.addfile(member)
                else:
                    member.size = len(content)
                    bundle.addfile(member, BytesIO(content))

    def test_release_workflow_blocks_publication_and_attaches_both_evidence_formats(self):
        workflow = (tool.ROOT / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8")
        self.assertIn("verify-dashboard-release:", workflow)
        self.assertIn("python3 scripts/dashboard_evidence.py validate --release", workflow)
        self.assertIn("dashboard-verification.json", workflow)
        self.assertIn("dashboard-verification.md", workflow)
        host = workflow.split("  host:", 1)[1].split("\n  publish-homebrew-formula:", 1)[0]
        self.assertIn("verify-dashboard-release", host)

    def test_homebrew_publish_uses_protected_environment(self):
        workflow = (tool.ROOT / ".github" / "workflows" / "release.yml").read_text(
            encoding="utf-8"
        )
        homebrew = workflow.split("  publish-homebrew-formula:", 1)[1].split(
            "\n  announce:", 1
        )[0]
        self.assertRegex(homebrew, r"(?m)^    environment: homebrew$")

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
