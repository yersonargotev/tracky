#!/usr/bin/env python3
"""Publish already-verified Tracky native artifacts as a stable release."""

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import quote
from urllib.request import Request, urlopen

sys.path.insert(0, str(Path(__file__).resolve().parent))
import dashboard_evidence as evidence  # noqa: E402


SHA = re.compile(r"[0-9a-f]{40}")
DIGEST = re.compile(r"[0-9a-f]{64}")
SEMVER = re.compile(r"\d+\.\d+\.\d+")
PLACEHOLDER = re.compile(r"(?i)(?:placeholder|todo|tbd|example\.com|<[^>]+>)")
REPOSITORY = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


@dataclass(frozen=True)
class Asset:
    name: str
    path: Path
    size: int
    sha256: str


def _asset(path):
    path = Path(path)
    require(path.is_file(), "release asset is missing: %s" % path)
    return Asset(path.name, path, path.stat().st_size, digest(path))


def validate_tag(tag, package_version):
    require(SEMVER.fullmatch(package_version or ""), "package version must be stable SemVer")
    require(
        re.fullmatch(r"v%s" % re.escape(package_version), tag or ""),
        "release tag must be v{exact package_version}",
    )


def prepare_assets(verified_root, evidence_json, evidence_markdown, tag):
    """Validate retained evidence and return the deterministic publication assets."""
    verified_root = Path(verified_root)
    evidence_json = Path(evidence_json)
    evidence_markdown = Path(evidence_markdown)
    require(evidence_json.name == "release-evidence.json", "evidence JSON name is invalid")
    require(evidence_markdown.name == "release-evidence.md", "evidence Markdown name is invalid")
    value = json.loads(evidence_json.read_text(encoding="utf-8"))
    require(isinstance(value, dict), "release evidence must be an object")
    require(value.get("schema_version") == 2, "release evidence schema is invalid")
    source_sha = value.get("source_sha")
    version = value.get("package_version")
    lockfile_sha256 = value.get("lockfile_sha256")
    require(SHA.fullmatch(str(source_sha or "")), "release evidence source SHA is invalid")
    require(SEMVER.fullmatch(str(version or "")), "release evidence package version is invalid")
    require(
        DIGEST.fullmatch(str(lockfile_sha256 or "")),
        "release evidence lockfile digest is invalid",
    )
    require(value.get("mode") == "release" and value.get("published") is False,
            "release evidence must describe an unpublished release")
    validate_tag(tag, version)
    gates = value.get("gates")
    require(
        isinstance(gates, list)
        and len(gates) == len(evidence.REQUIRED_RELEASE_GATES),
        "release evidence must contain all 11 gates",
    )
    require(
        {item.get("gate") for item in gates} == evidence.REQUIRED_RELEASE_GATES,
        "release gate matrix is invalid",
    )
    require(all(item.get("status") == "pass" for item in gates), "every release gate must pass")
    artifacts = value.get("artifacts")
    require(isinstance(artifacts, list) and len(artifacts) == 2, "release evidence must contain two native artifacts")
    require(
        {item.get("target") for item in artifacts} == evidence.TARGETS,
        "native target matrix is invalid",
    )

    assets = []
    for item in sorted(artifacts, key=lambda entry: entry["target"]):
        target = item["target"]
        name = item.get("archive_name")
        require(name == "tracky-%s.tar.xz" % target, "archive name is invalid for %s" % target)
        require(isinstance(item.get("archive_bytes"), int) and item["archive_bytes"] > 0,
                "archive byte count is invalid")
        require(DIGEST.fullmatch(str(item.get("archive_sha256", ""))), "archive digest is invalid")
        semantic = item.get("semantic_manifest")
        require(isinstance(semantic, dict), "semantic identity is missing")
        evidence.validate_semantic_archive_manifest(semantic)
        require(semantic.get("source_sha") == source_sha, "semantic source SHA differs")
        require(
            semantic.get("lockfile_sha256") == lockfile_sha256,
            "semantic lockfile digest differs",
        )
        require(semantic.get("package_version") == version, "semantic package version differs")
        require(semantic.get("target") == target, "semantic target differs")
        transport = semantic.get("transport")
        require(isinstance(transport, dict), "semantic transport is missing")
        require(transport.get("archive_name") == name, "semantic archive name differs")
        require(transport.get("archive_bytes") == item["archive_bytes"], "semantic archive bytes differ")
        require(transport.get("archive_sha256") == item["archive_sha256"], "semantic archive digest differs")
        matches = list(verified_root.glob("release-verified-%s-%s/candidate/%s" % (source_sha, target, name)))
        require(len(matches) == 1, "expected exactly one verified bundle for %s" % target)
        archive = matches[0]
        require(archive.stat().st_size == item["archive_bytes"], "archive byte count differs")
        require(digest(archive) == item["archive_sha256"], "archive digest differs")
        checksum = archive.with_name(archive.name + ".sha256")
        require(checksum.is_file(), "archive checksum file is missing")
        evidence.verify_dist_checksum(archive)
        require(DIGEST.fullmatch(digest(checksum)), "checksum file digest is invalid")
        assets.extend([_asset(archive), _asset(checksum)])
    assets.extend([_asset(evidence_json), _asset(evidence_markdown)])
    names = [item.name for item in assets]
    require(len(names) == len(set(names)), "release asset names must be unique")
    return (
        source_sha,
        version,
        lockfile_sha256,
        tuple(sorted(assets, key=lambda item: item.name)),
    )


def reconcile_tag(existing_target, source_sha):
    if existing_target is None:
        return "create"
    require(existing_target == source_sha, "existing tag points at a different commit")
    return "keep"


def reconcile_release(release, source_sha, tag=None, repository=None):
    if release is None:
        return "create"
    require(release.get("prerelease") is False, "existing release is a prerelease")
    require(release.get("target_commitish") == source_sha, "existing release targets a different commit")
    require(isinstance(release.get("draft"), bool), "existing release draft state is invalid")
    require(
        isinstance(release.get("id"), int) and release["id"] > 0,
        "existing release ID is invalid",
    )
    if tag is not None:
        require(release.get("tag_name") == tag, "existing release tag differs")
    if repository is not None:
        expected_upload_url = (
            "https://uploads.github.com/repos/%s/releases/%s/assets{?name,label}"
            % (repository, release["id"])
        )
        require(
            release.get("upload_url") == expected_upload_url,
            "existing release upload URL is invalid",
        )
        release_url = release.get("html_url")
        expected_url = "https://github.com/%s/releases/tag/%s" % (repository, tag)
        draft_prefix = "https://github.com/%s/releases/tag/untagged-" % repository
        require(
            release_url == expected_url
            or (release["draft"] and str(release_url).startswith(draft_prefix)),
            "existing release URL is invalid",
        )
        require(
            PLACEHOLDER.search(release_url) is None,
            "existing release URL contains a placeholder",
        )
    return "keep"


def reconcile_assets(remote_assets, local_assets):
    expected = {asset.name: asset for asset in local_assets}
    remote = {item.get("name"): item for item in remote_assets}
    require(len(remote) == len(remote_assets), "release contains duplicate asset names")
    unexpected = sorted(set(remote) - set(expected))
    require(not unexpected, "release contains unexpected assets: %s" % ", ".join(unexpected))
    return tuple(expected[name] for name in sorted(set(expected) - set(remote)))


def validate_remote_asset(remote, local):
    require(remote.get("name") == local.name, "remote asset name differs")
    require(
        isinstance(remote.get("id"), int) and remote["id"] > 0,
        "remote asset ID is invalid",
    )
    require(remote.get("state") == "uploaded", "remote asset is not uploaded")
    require(remote.get("size") == local.size, "remote asset size differs: %s" % local.name)
    require(
        remote.get("digest") == "sha256:" + local.sha256,
        "remote asset digest differs: %s" % local.name,
    )
    return remote


def recoverable_starter(remote):
    if remote.get("state") != "starter":
        return False
    require(
        isinstance(remote.get("id"), int) and remote["id"] > 0,
        "starter asset ID is invalid",
    )
    require(
        remote.get("size") == 0 and remote.get("digest") is None,
        "starter asset is not an empty failed upload",
    )
    return True


class Gh:
    def __init__(self, repository, runner=None, uploader=None, environ=None):
        require(REPOSITORY.fullmatch(repository or ""), "repository must be owner/name")
        require(PLACEHOLDER.search(repository) is None, "repository contains a placeholder")
        self.repository = repository
        self.runner = runner or self._run
        self.uploader = uploader or self._upload
        self.environ = environ if environ is not None else os.environ

    @staticmethod
    def _upload(request):
        with urlopen(request) as response:
            return response.read()

    @staticmethod
    def _run(argv, binary=False):
        result = subprocess.run(argv, capture_output=True, text=not binary, check=False)
        return result.returncode, result.stdout, result.stderr

    def command(self, argv, binary=False, allow_missing=False):
        code, stdout, stderr = self.runner(argv, binary=binary)
        error_text = stderr.decode() if binary and isinstance(stderr, bytes) else str(stderr)
        if code and allow_missing and ("HTTP 404" in error_text or "Not Found" in error_text):
            return None
        if code:
            raise ValueError("gh command failed: %s" % error_text.strip())
        return stdout

    def api_json(self, path, *extra, allow_missing=False):
        output = self.command(["gh", "api", path, *extra], allow_missing=allow_missing)
        return None if output is None else json.loads(output)

    def tag_target(self, tag):
        value = self.api_json("repos/{}/git/ref/tags/{}".format(self.repository, tag), allow_missing=True)
        if value is None:
            return None
        obj = value.get("object", {})
        require(obj.get("type") == "commit", "existing tag is not lightweight")
        return obj.get("sha")

    def create_tag(self, tag, source_sha):
        self.api_json("repos/{}/git/refs".format(self.repository), "-f", "ref=refs/tags/" + tag, "-f", "sha=" + source_sha)

    def release(self, tag):
        pages = self.api_json(
            "repos/{}/releases?per_page=100".format(self.repository),
            "--paginate",
            "--slurp",
        )
        require(
            isinstance(pages, list) and all(isinstance(page, list) for page in pages),
            "release list response is invalid",
        )
        releases = [release for page in pages for release in page]
        matches = [item for item in releases if item.get("tag_name") == tag]
        require(len(matches) <= 1, "release tag is duplicated")
        return matches[0] if matches else None

    def release_by_id(self, release_id):
        require(
            isinstance(release_id, int) and release_id > 0,
            "release ID is invalid",
        )
        return self.api_json(
            "repos/{}/releases/{}".format(self.repository, release_id)
        )

    def create_release(self, tag, source_sha):
        release = self.api_json(
            "repos/{}/releases".format(self.repository),
            "-f", "tag_name=" + tag,
            "-f", "target_commitish=" + source_sha,
            "-f", "name=" + tag,
            "-f", "body=Tracky stable release.",
            "-F", "draft=true",
            "-F", "prerelease=false",
        )
        require(isinstance(release, dict), "release creation response is invalid")
        return release

    def download_asset(self, asset_id):
        return self.command(["gh", "api", "repos/{}/releases/assets/{}".format(self.repository, asset_id),
                             "-H", "Accept: application/octet-stream"], binary=True)

    def upload(self, release, asset):
        expected = (
            "https://uploads.github.com/repos/{}/releases/{}/assets{{?name,label}}"
            .format(self.repository, release["id"])
        )
        require(release.get("upload_url") == expected, "release upload URL is invalid")
        token = self.environ.get("GH_TOKEN")
        require(isinstance(token, str) and token, "GH_TOKEN is required for asset upload")
        request = Request(
            expected.replace("{?name,label}", "?name=" + quote(asset.name, safe="")),
            data=asset.path.read_bytes(),
            method="POST",
            headers={
                "Authorization": "Bearer " + token,
                "Accept": "application/vnd.github+json",
                "Content-Type": "application/octet-stream",
                "X-GitHub-Api-Version": "2022-11-28",
            },
        )
        try:
            result = json.loads(self.uploader(request))
        except (TypeError, ValueError, json.JSONDecodeError) as error:
            raise ValueError("release asset upload response is invalid") from error
        require(isinstance(result, dict), "release asset upload response is invalid")
        return result

    def delete_asset(self, asset_id):
        self.command([
            "gh",
            "api",
            "repos/{}/releases/assets/{}".format(self.repository, asset_id),
            "--method",
            "DELETE",
        ])

    def publish_release(self, release_id):
        self.api_json(
            "repos/{}/releases/{}".format(self.repository, release_id),
            "--method", "PATCH",
            "-F", "draft=false",
        )


def publish(client, tag, source_sha, assets):
    if reconcile_tag(client.tag_target(tag), source_sha) == "create":
        client.create_tag(tag, source_sha)
    release = client.release(tag)
    if reconcile_release(release, source_sha, tag, client.repository) == "create":
        release = client.create_release(tag, source_sha)
    require(isinstance(release, dict), "created release cannot be read back")
    reconcile_release(release, source_sha, tag, client.repository)
    local = {asset.name: asset for asset in assets}
    reconcile_assets(release.get("assets", []), assets)
    retained_remote = []
    starters = []
    for remote in release.get("assets", []):
        if recoverable_starter(remote):
            starters.append(remote)
            continue
        validate_remote_asset(remote, local[remote["name"]])
        require(client.download_asset(remote["id"]) == local[remote["name"]].path.read_bytes(),
                "existing release asset bytes differ: %s" % remote["name"])
        retained_remote.append(remote)
    missing = reconcile_assets(retained_remote, assets)
    require(
        release["draft"] or (not missing and not starters),
        "published release is missing assets",
    )
    for remote in starters:
        client.delete_asset(remote["id"])
    for asset in missing:
        client.upload(release, asset)
    complete = client.release_by_id(release["id"])
    reconcile_release(complete, source_sha, tag, client.repository)
    require(not reconcile_assets(complete.get("assets", []), assets), "release asset upload is incomplete")
    by_name = {item["name"]: item for item in complete["assets"]}
    for asset in assets:
        validate_remote_asset(by_name[asset.name], asset)
        require(client.download_asset(by_name[asset.name]["id"]) == asset.path.read_bytes(),
                "downloaded release asset bytes differ: %s" % asset.name)
    if complete["draft"]:
        client.publish_release(complete["id"])
        complete = client.release_by_id(complete["id"])
        reconcile_release(complete, source_sha, tag, client.repository)
        require(complete["draft"] is False, "release remained a draft after publication")
    return complete


def publication_evidence(
    release, tag, source_sha, package_version, lockfile_sha256, assets
):
    by_name = {item["name"]: item for item in release["assets"]}
    value = {
        "schema_version": 1, "tag": tag, "source_sha": source_sha,
        "package_version": package_version, "lockfile_sha256": lockfile_sha256,
        "release_id": release.get("id"),
        "release_url": release.get("html_url"), "prerelease": False,
        "assets": [
            {
                "name": asset.name,
                "id": validate_remote_asset(by_name[asset.name], asset)["id"],
                "bytes": by_name[asset.name]["size"],
                "digest": by_name[asset.name]["digest"],
            }
            for asset in assets
        ],
    }
    require(isinstance(value["release_id"], int) and value["release_id"] > 0, "release ID is invalid")
    require(isinstance(value["release_url"], str) and value["release_url"].startswith("https://github.com/"), "release URL is invalid")
    require(PLACEHOLDER.search(json.dumps(value)) is None, "publication evidence contains a placeholder")
    return value


def main(argv=None, client_factory=Gh):
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--verified-root", type=Path, required=True)
    parser.add_argument("--evidence-json", type=Path, required=True)
    parser.add_argument("--evidence-markdown", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    require(REPOSITORY.fullmatch(args.repository or ""), "repository must be owner/name")
    require(PLACEHOLDER.search(args.repository) is None, "repository contains a placeholder")
    source_sha, version, lockfile_sha256, assets = prepare_assets(
        args.verified_root, args.evidence_json, args.evidence_markdown, args.tag
    )
    release = publish(client_factory(args.repository), args.tag, source_sha, assets)
    args.output.write_text(
        json.dumps(
            publication_evidence(
                release, args.tag, source_sha, version, lockfile_sha256, assets
            ),
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
