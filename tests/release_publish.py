import hashlib
import importlib.util
import json
import copy
import tempfile
import unittest
from pathlib import Path


SPEC = importlib.util.spec_from_file_location(
    "release_publish",
    Path(__file__).parents[1] / "scripts" / "release_publish.py",
)
publication = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publication)


class Fixture:
    sha = "a" * 40
    version = "1.2.3"
    tag = "v1.2.3"
    lockfile = "b" * 64

    def __init__(self, root):
        self.root = root
        self.verified = root / "verified"
        artifacts = []
        for target in sorted(publication.evidence.TARGETS):
            candidate = self.verified / (
                "release-verified-%s-%s" % (self.sha, target)
            ) / "candidate"
            candidate.mkdir(parents=True)
            archive = candidate / ("tracky-%s.tar.xz" % target)
            archive.write_bytes(("native-" + target).encode())
            archive_hash = hashlib.sha256(archive.read_bytes()).hexdigest()
            checksum = archive.with_name(archive.name + ".sha256")
            checksum.write_text("%s  %s\n" % (archive_hash, archive.name))
            artifacts.append({
                "target": target,
                "archive_name": archive.name,
                "archive_bytes": archive.stat().st_size,
                "archive_sha256": archive_hash,
                "semantic_manifest": {
                    "schema_version": 1,
                    "source_sha": self.sha,
                    "lockfile_sha256": self.lockfile,
                    "cargo_dist_manifest_sha256": "c" * 64,
                    "package_version": self.version,
                    "target": target,
                    "tools": {
                        "cargo": "cargo 1",
                        "cargo-dist": "cargo-dist 1",
                        "rust": "rustc 1",
                    },
                    "files": [
                        {
                            "path": name,
                            "bytes": 1,
                            "sha256": "d" * 64,
                            "mode": "0755" if name == "tracky" else "0644",
                        }
                        for name in sorted(
                            publication.evidence.REQUIRED_ARCHIVE_FILES
                        )
                    ],
                    "transport": {
                        "archive_name": archive.name,
                        "archive_bytes": archive.stat().st_size,
                        "archive_sha256": archive_hash,
                    },
                },
            })
        self.evidence = root / "release-evidence.json"
        self.markdown = root / "release-evidence.md"
        self.value = {
            "schema_version": 2,
            "source_sha": self.sha,
            "package_version": self.version,
            "lockfile_sha256": self.lockfile,
            "mode": "release",
            "published": False,
            "gates": [
                {"gate": gate, "status": "pass"}
                for gate in sorted(publication.evidence.REQUIRED_RELEASE_GATES)
            ],
            "artifacts": artifacts,
        }
        self.write()
        self.markdown.write_text("# Verified evidence\n")

    def write(self):
        self.evidence.write_text(json.dumps(self.value))

    def prepare(self):
        return publication.prepare_assets(
            self.verified, self.evidence, self.markdown, self.tag
        )


class FakeClient:
    def __init__(self, source_sha, release=None, tag_target=None):
        self.repository = "o/r"
        self.source_sha = source_sha
        self._tag = tag_target
        self._release = release
        if self._release is not None:
            self._release.setdefault(
                "upload_url",
                "https://uploads.github.com/repos/o/r/releases/%s/assets{?name,label}"
                % self._release["id"],
            )
        self.bytes = {}
        self.next_id = 100
        self.uploaded = []
        self.deleted = []
        self.published = 0
        self.events = []

    def tag_target(self, _tag):
        return self._tag

    def create_tag(self, _tag, sha):
        self._tag = sha

    def release(self, _tag):
        return self._release

    def release_by_id(self, release_id):
        if release_id != self._release["id"]:
            raise AssertionError("wrong release")
        return self._release

    def create_release(self, tag, sha):
        self._release = {
            "id": 9,
            "html_url": "https://github.com/o/r/releases/tag/untagged-draft",
            "tag_name": tag,
            "prerelease": False,
            "draft": True,
            "target_commitish": sha,
            "upload_url": (
                "https://uploads.github.com/repos/o/r/releases/9/assets{?name,label}"
            ),
            "assets": [],
        }
        return self._release

    def download_asset(self, asset_id):
        return self.bytes[asset_id]

    def upload(self, release, asset):
        if release["id"] != self._release["id"]:
            raise AssertionError("wrong release")
        self.events.append("upload:" + asset.name)
        remote = {
            "id": self.next_id,
            "name": asset.name,
            "state": "uploaded",
            "size": asset.size,
            "digest": "sha256:" + asset.sha256,
        }
        self.next_id += 1
        self._release["assets"].append(remote)
        self.bytes[remote["id"]] = asset.path.read_bytes()
        self.uploaded.append(asset.name)

    def delete_asset(self, asset_id):
        matching = [
            item for item in self._release["assets"] if item["id"] == asset_id
        ]
        if len(matching) != 1:
            raise AssertionError("wrong asset")
        self.events.append("delete:%s" % asset_id)
        self._release["assets"].remove(matching[0])
        self.bytes.pop(asset_id, None)
        self.deleted.append(asset_id)

    def publish_release(self, release_id):
        self.events.append("publish")
        self.assert_complete_at_publish = len(self._release["assets"])
        if release_id != self._release["id"]:
            raise AssertionError("wrong release")
        self._release["draft"] = False
        self._release["html_url"] = (
            "https://github.com/o/r/releases/tag/" + self._release["tag_name"]
        )
        self.published += 1


class ReleasePublishTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.fixture = Fixture(Path(self.temp.name))

    def tearDown(self):
        self.temp.cleanup()

    def test_prepares_exact_deterministic_asset_set(self):
        sha, version, lockfile_sha256, assets = self.fixture.prepare()
        self.assertEqual(
            (sha, version, lockfile_sha256),
            (self.fixture.sha, self.fixture.version, self.fixture.lockfile),
        )
        self.assertEqual([item.name for item in assets], sorted([
            "release-evidence.json",
            "release-evidence.md",
            "tracky-aarch64-apple-darwin.tar.xz",
            "tracky-aarch64-apple-darwin.tar.xz.sha256",
            "tracky-x86_64-unknown-linux-gnu.tar.xz",
            "tracky-x86_64-unknown-linux-gnu.tar.xz.sha256",
        ]))
        self.assertTrue(all(item.size > 0 for item in assets))
        self.assertTrue(all(len(item.sha256) == 64 for item in assets))

    def test_requires_exact_stable_tag(self):
        for tag in ("1.2.3", "v1.2.3-rc.1", "v1.2.4", "v1.2.3+build"):
            with self.subTest(tag=tag), self.assertRaisesRegex(ValueError, "release tag"):
                publication.prepare_assets(
                    self.fixture.verified, self.fixture.evidence,
                    self.fixture.markdown, tag,
                )
        for version in ("1.2.3-rc.1", "1.2.3+build"):
            with self.subTest(version=version), self.assertRaisesRegex(
                ValueError, "stable SemVer"
            ):
                publication.validate_tag("v" + version, version)

    def test_rejects_schema_identity_gate_and_publish_tampering(self):
        mutations = [
            ("schema_version", 1, "schema"),
            ("source_sha", "b" * 39, "source SHA"),
            ("package_version", "next", "version"),
            ("mode", "dry-run", "unpublished release"),
            ("published", True, "unpublished release"),
        ]
        for key, changed, message in mutations:
            with self.subTest(key=key):
                original = self.fixture.value[key]
                self.fixture.value[key] = changed
                self.fixture.write()
                with self.assertRaisesRegex(ValueError, message):
                    self.fixture.prepare()
                self.fixture.value[key] = original
        self.fixture.value["gates"][0]["status"] = "fail"
        self.fixture.write()
        with self.assertRaisesRegex(ValueError, "every release gate"):
            self.fixture.prepare()

    def test_rejects_substituted_archive_and_checksum_bytes(self):
        archive = next(self.fixture.verified.rglob("*.tar.xz"))
        original = archive.read_bytes()
        archive.write_bytes(b"substitution")
        with self.assertRaisesRegex(ValueError, "byte count|digest"):
            self.fixture.prepare()
        archive.write_bytes(original)
        archive.with_name(archive.name + ".sha256").write_text(
            "0" * 64 + " *" + archive.name + "\n"
        )
        with self.assertRaisesRegex(ValueError, "checksum"):
            self.fixture.prepare()

    def test_rejects_semantic_identity_drift_and_missing_bundle(self):
        self.fixture.value["artifacts"][0]["semantic_manifest"]["source_sha"] = "b" * 40
        self.fixture.write()
        with self.assertRaisesRegex(ValueError, "semantic source"):
            self.fixture.prepare()
        self.fixture.value["artifacts"][0]["semantic_manifest"]["source_sha"] = self.fixture.sha
        self.fixture.write()
        next(self.fixture.verified.iterdir()).rename(self.fixture.verified / "ignored")
        with self.assertRaisesRegex(ValueError, "exactly one"):
            self.fixture.prepare()

    def test_canonical_semantic_manifest_rejects_omitted_contract_tampering(self):
        original = copy.deepcopy(self.fixture.value["artifacts"][0]["semantic_manifest"])
        mutations = (
            ("schema", lambda value: value.__setitem__("schema_version", 2)),
            ("Rust", lambda value: value.__setitem__("tools", {"cargo": "cargo 1"})),
            ("files", lambda value: value.__setitem__("files", [])),
            (
                "Cargo Dist",
                lambda value: value.__setitem__(
                    "cargo_dist_manifest_sha256", "invalid"
                ),
            ),
            (
                "lockfile",
                lambda value: value.__setitem__(
                    "lockfile_sha256", "e" * 64
                ),
            ),
        )
        for message, mutate in mutations:
            with self.subTest(message=message):
                semantic = copy.deepcopy(original)
                mutate(semantic)
                self.fixture.value["artifacts"][0]["semantic_manifest"] = semantic
                self.fixture.write()
                with self.assertRaisesRegex(ValueError, message):
                    self.fixture.prepare()
        self.fixture.value["artifacts"][0]["semantic_manifest"] = original
        self.fixture.write()

    def test_pure_reconciliation_fails_closed(self):
        self.assertEqual(publication.reconcile_tag(None, self.fixture.sha), "create")
        self.assertEqual(publication.reconcile_tag(self.fixture.sha, self.fixture.sha), "keep")
        with self.assertRaisesRegex(ValueError, "different commit"):
            publication.reconcile_tag("b" * 40, self.fixture.sha)
        with self.assertRaisesRegex(ValueError, "is a prerelease"):
            publication.reconcile_release(
                {"prerelease": True, "draft": True, "target_commitish": self.fixture.sha},
                self.fixture.sha,
            )
        with self.assertRaisesRegex(ValueError, "different commit"):
            publication.reconcile_release(
                {"prerelease": False, "draft": True, "target_commitish": "b" * 40},
                self.fixture.sha,
            )
        _, _, _, assets = self.fixture.prepare()
        with self.assertRaisesRegex(ValueError, "unexpected assets"):
            publication.reconcile_assets([{"name": "attestation.intoto"}], assets)

    def test_retry_keeps_existing_asset_and_fills_only_missing(self):
        _, _, _, assets = self.fixture.prepare()
        existing = assets[0]
        release = {
            "id": 9,
            "html_url": "https://github.com/o/r/releases/tag/untagged-partial",
            "tag_name": self.fixture.tag,
            "prerelease": False,
            "draft": True,
            "target_commitish": self.fixture.sha,
            "assets": [{
                "id": 42, "name": existing.name, "state": "uploaded",
                "size": existing.size, "digest": "sha256:" + existing.sha256,
            }],
        }
        client = FakeClient(self.fixture.sha, release, self.fixture.sha)
        client.bytes[42] = existing.path.read_bytes()
        result = publication.publish(client, self.fixture.tag, self.fixture.sha, assets)
        self.assertNotIn(existing.name, client.uploaded)
        self.assertEqual(result["assets"][0]["id"], 42)
        self.assertFalse(result["draft"])
        self.assertEqual(client.published, 1)
        self.assertEqual(client.assert_complete_at_publish, len(assets))
        self.assertEqual({item["name"] for item in result["assets"]},
                         {item.name for item in assets})

    def test_retry_replaces_empty_starter_without_touching_uploaded_asset(self):
        _, _, _, assets = self.fixture.prepare()
        successful, failed = assets[:2]
        release = {
            "id": 9,
            "html_url": "https://github.com/o/r/releases/tag/untagged-partial",
            "tag_name": self.fixture.tag,
            "prerelease": False,
            "draft": True,
            "target_commitish": self.fixture.sha,
            "assets": [
                {
                    "id": 42,
                    "name": successful.name,
                    "state": "uploaded",
                    "size": successful.size,
                    "digest": "sha256:" + successful.sha256,
                },
                {
                    "id": 43,
                    "name": failed.name,
                    "state": "starter",
                    "size": 0,
                    "digest": None,
                },
            ],
        }
        client = FakeClient(self.fixture.sha, release, self.fixture.sha)
        client.bytes[42] = successful.path.read_bytes()
        result = publication.publish(
            client, self.fixture.tag, self.fixture.sha, assets
        )
        self.assertEqual(client.deleted, [43])
        self.assertNotIn(successful.name, client.uploaded)
        self.assertIn(failed.name, client.uploaded)
        self.assertEqual(
            next(item for item in result["assets"] if item["name"] == successful.name)["id"],
            42,
        )
        self.assertLess(
            client.events.index("delete:43"),
            client.events.index("upload:" + failed.name),
        )
        self.assertEqual(client.events[-1], "publish")

    def test_refuses_nonempty_or_malformed_starter(self):
        _, _, _, assets = self.fixture.prepare()
        asset = assets[0]
        for changed in (
            {"size": 1, "digest": None},
            {"size": 0, "digest": "sha256:" + asset.sha256},
            {"id": 0, "size": 0, "digest": None},
        ):
            with self.subTest(changed=changed):
                remote = {
                    "id": 43,
                    "name": asset.name,
                    "state": "starter",
                    **changed,
                }
                release = {
                    "id": 9,
                    "html_url": "https://github.com/o/r/releases/tag/untagged-partial",
                    "tag_name": self.fixture.tag,
                    "prerelease": False,
                    "draft": True,
                    "target_commitish": self.fixture.sha,
                    "assets": [remote],
                }
                client = FakeClient(self.fixture.sha, release, self.fixture.sha)
                with self.assertRaisesRegex(ValueError, "starter asset"):
                    publication.publish(
                        client, self.fixture.tag, self.fixture.sha, assets
                    )
                self.assertEqual(client.deleted, [])
                self.assertEqual(client.uploaded, [])

    def test_retry_preflight_finishes_before_deleting_starters(self):
        _, _, _, assets = self.fixture.prepare()
        starter, mismatched = assets[:2]
        release = {
            "id": 9,
            "html_url": "https://github.com/o/r/releases/tag/untagged-partial",
            "tag_name": self.fixture.tag,
            "prerelease": False,
            "draft": True,
            "target_commitish": self.fixture.sha,
            "assets": [
                {
                    "id": 43,
                    "name": starter.name,
                    "state": "starter",
                    "size": 0,
                    "digest": None,
                },
                {
                    "id": 44,
                    "name": mismatched.name,
                    "state": "uploaded",
                    "size": mismatched.size,
                    "digest": "sha256:" + "0" * 64,
                },
            ],
        }
        client = FakeClient(self.fixture.sha, release, self.fixture.sha)
        client.bytes[44] = mismatched.path.read_bytes()
        with self.assertRaisesRegex(ValueError, "digest differs"):
            publication.publish(client, self.fixture.tag, self.fixture.sha, assets)
        self.assertEqual(client.deleted, [])
        self.assertEqual(client.uploaded, [])

    def test_published_release_never_deletes_a_starter(self):
        _, _, _, assets = self.fixture.prepare()
        starter = assets[0]
        release = {
            "id": 9,
            "html_url": "https://github.com/o/r/releases/tag/" + self.fixture.tag,
            "tag_name": self.fixture.tag,
            "prerelease": False,
            "draft": False,
            "target_commitish": self.fixture.sha,
            "assets": [{
                "id": 43,
                "name": starter.name,
                "state": "starter",
                "size": 0,
                "digest": None,
            }],
        }
        client = FakeClient(self.fixture.sha, release, self.fixture.sha)
        with self.assertRaisesRegex(ValueError, "published release is missing"):
            publication.publish(client, self.fixture.tag, self.fixture.sha, assets)
        self.assertEqual(client.deleted, [])
        self.assertEqual(client.uploaded, [])

    def test_refuses_existing_asset_with_different_bytes(self):
        _, _, _, assets = self.fixture.prepare()
        release = {
            "id": 9, "html_url": "https://github.com/o/r/releases/tag/" + self.fixture.tag,
            "tag_name": self.fixture.tag, "draft": True,
            "prerelease": False, "target_commitish": self.fixture.sha,
            "assets": [{
                "id": 42, "name": assets[0].name, "state": "uploaded",
                "size": assets[0].size, "digest": "sha256:" + assets[0].sha256,
            }],
        }
        client = FakeClient(self.fixture.sha, release, self.fixture.sha)
        client.bytes[42] = b"different"
        with self.assertRaisesRegex(ValueError, "bytes differ"):
            publication.publish(client, self.fixture.tag, self.fixture.sha, assets)
        self.assertEqual(client.uploaded, [])
        self.assertEqual(client.published, 0)

    def test_complete_published_release_is_exact_no_op(self):
        _, _, _, assets = self.fixture.prepare()
        release = {
            "id": 9,
            "html_url": "https://github.com/o/r/releases/tag/" + self.fixture.tag,
            "tag_name": self.fixture.tag,
            "draft": False,
            "prerelease": False,
            "target_commitish": self.fixture.sha,
            "assets": [],
        }
        client = FakeClient(self.fixture.sha, release, self.fixture.sha)
        for index, asset in enumerate(assets, start=1):
            release["assets"].append({
                "id": index,
                "name": asset.name,
                "state": "uploaded",
                "size": asset.size,
                "digest": "sha256:" + asset.sha256,
            })
            client.bytes[index] = asset.path.read_bytes()
        result = publication.publish(client, self.fixture.tag, self.fixture.sha, assets)
        self.assertIs(result, release)
        self.assertEqual(client.uploaded, [])
        self.assertEqual(client.published, 0)
        self.assertEqual(client.events, [])

    def test_refuses_remote_asset_metadata_mismatch_before_download_or_publish(self):
        _, _, _, assets = self.fixture.prepare()
        asset = assets[0]
        for field, changed, message in (
            ("state", "new", "not uploaded"),
            ("id", 0, "ID"),
            ("size", asset.size + 1, "size"),
            ("digest", "sha256:" + "b" * 64, "digest"),
        ):
            with self.subTest(field=field):
                remote = {
                    "id": 42,
                    "name": asset.name,
                    "state": "uploaded",
                    "size": asset.size,
                    "digest": "sha256:" + asset.sha256,
                }
                remote[field] = changed
                release = {
                    "id": 9,
                    "html_url": "https://github.com/o/r/releases/tag/" + self.fixture.tag,
                    "tag_name": self.fixture.tag,
                    "draft": True,
                    "prerelease": False,
                    "target_commitish": self.fixture.sha,
                    "assets": [remote],
                }
                client = FakeClient(self.fixture.sha, release, self.fixture.sha)
                client.bytes[42] = asset.path.read_bytes()
                with self.assertRaisesRegex(ValueError, message):
                    publication.publish(
                        client, self.fixture.tag, self.fixture.sha, assets
                    )
                self.assertEqual(client.uploaded, [])
                self.assertEqual(client.published, 0)

    def test_rejects_invalid_repository_and_release_identity(self):
        with self.assertRaisesRegex(ValueError, "owner/name"):
            publication.Gh("not-a-repository")
        with self.assertRaisesRegex(ValueError, "placeholder"):
            publication.Gh("example.com/repo")
        release = {
            "id": 9,
            "prerelease": False,
            "draft": True,
            "target_commitish": self.fixture.sha,
            "tag_name": "wrong",
            "html_url": "https://github.com/o/r/releases/tag/wrong",
            "upload_url": (
                "https://uploads.github.com/repos/o/r/releases/9/assets{?name,label}"
            ),
        }
        with self.assertRaisesRegex(ValueError, "tag differs"):
            publication.reconcile_release(
                release, self.fixture.sha, self.fixture.tag, "o/r"
            )


    def test_github_release_creation_is_stable_and_draft(self):
        calls = []
        release = {
            "id": 9,
            "html_url": "https://github.com/o/r/releases/tag/untagged-draft",
            "tag_name": self.fixture.tag,
            "prerelease": False,
            "draft": True,
            "target_commitish": self.fixture.sha,
            "assets": [],
        }

        def runner(argv, binary=False):
            calls.append(argv)
            if argv[2].endswith("/releases"):
                return 0, json.dumps(release), ""
            return 0, "{}", ""

        client = publication.Gh("o/r", runner=runner)
        self.assertEqual(client.create_release(self.fixture.tag, self.fixture.sha), release)
        self.assertEqual(len(calls), 1)
        create = calls[0]
        self.assertIn("draft=true", create)
        self.assertIn("prerelease=false", create)
        self.assertIn("body=Tracky stable release.", create)
        self.assertNotIn("prerelease=true", create)

    def test_upload_uses_validated_uploads_host_and_keeps_token_out_of_commands(self):
        _, _, _, assets = self.fixture.prepare()
        requests = []
        commands = []

        def uploader(request):
            requests.append(request)
            return json.dumps({"id": 77, "state": "uploaded"}).encode()

        def runner(argv, binary=False):
            commands.append(argv)
            return 0, b"" if binary else "", b"" if binary else ""

        client = publication.Gh(
            "o/r",
            runner=runner,
            uploader=uploader,
            environ={"GH_TOKEN": "secret-token"},
        )
        release = {
            "id": 9,
            "upload_url": (
                "https://uploads.github.com/repos/o/r/releases/9/assets{?name,label}"
            ),
        }
        client.upload(release, assets[0])
        self.assertEqual(len(requests), 1)
        request = requests[0]
        self.assertEqual(
            request.full_url,
            "https://uploads.github.com/repos/o/r/releases/9/assets?name="
            + assets[0].name,
        )
        self.assertEqual(request.method, "POST")
        self.assertEqual(request.data, assets[0].path.read_bytes())
        self.assertEqual(request.get_header("Authorization"), "Bearer secret-token")
        self.assertEqual(
            request.get_header("Content-type"), "application/octet-stream"
        )
        self.assertEqual(commands, [])
        self.assertNotIn("secret-token", repr(commands))
        bad = dict(release, upload_url="https://api.github.com/repos/o/r/releases/9/assets{?name,label}")
        with self.assertRaisesRegex(ValueError, "upload URL"):
            client.upload(bad, assets[0])

    def test_main_writes_concrete_publication_evidence(self):
        holder = {}

        def factory(_repository):
            holder["client"] = FakeClient(self.fixture.sha)
            return holder["client"]

        output = Path(self.temp.name) / "publication.json"
        publication.main([
            "--repository", "o/r",
            "--tag", self.fixture.tag,
            "--verified-root", str(self.fixture.verified),
            "--evidence-json", str(self.fixture.evidence),
            "--evidence-markdown", str(self.fixture.markdown),
            "--output", str(output),
        ], client_factory=factory)
        value = json.loads(output.read_text())
        self.assertEqual(value["schema_version"], 1)
        self.assertEqual(value["tag"], self.fixture.tag)
        self.assertEqual(value["source_sha"], self.fixture.sha)
        self.assertEqual(value["package_version"], self.fixture.version)
        self.assertEqual(value["lockfile_sha256"], self.fixture.lockfile)
        self.assertFalse(value["prerelease"])
        self.assertEqual(len(value["assets"]), 6)
        self.assertTrue(all(
            item["id"] > 0
            and item["bytes"] > 0
            and item["digest"].startswith("sha256:")
            for item in value["assets"]
        ))
        self.assertNotRegex(output.read_text(), publication.PLACEHOLDER)


if __name__ == "__main__":
    unittest.main()
