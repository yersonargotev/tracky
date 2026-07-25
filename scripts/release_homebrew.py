#!/usr/bin/env python3
"""Prepare and verify the Homebrew formula for one published Tracky release."""

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import dashboard_evidence as dashboard  # noqa: E402
import release_publish as publication  # noqa: E402


PLACEHOLDER = publication.PLACEHOLDER
FORMULA_VERSION = re.compile(r"\d+\.\d+\.\d+")
TARGET_SYSTEM = {
    "aarch64-apple-darwin": "macos-arm64",
    "x86_64-unknown-linux-gnu": "linux-x86_64",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read_json(path):
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise ValueError("invalid JSON file: %s" % path) from error


def expected_asset_names():
    names = {"release-evidence.json", "release-evidence.md"}
    for target in dashboard.TARGETS:
        archive = "tracky-%s.tar.xz" % target
        names.update({archive, archive + ".sha256"})
    return names


def validate_inputs(
    release,
    assets_root,
    repository,
    tag,
    source_sha,
    package_version,
):
    """Fail closed and return validated archive information keyed by target."""
    publication.validate_tag(tag, package_version)
    require(publication.REPOSITORY.fullmatch(repository or ""), "repository must be owner/name")
    require(publication.SHA.fullmatch(source_sha or ""), "source SHA is invalid")
    require(release.get("draft") is False, "release must be published")
    require(release.get("prerelease") is False, "release must be stable")
    require(release.get("tag_name") == tag, "release tag differs")
    require(release.get("target_commitish") == source_sha, "release target differs")
    release_url = "https://github.com/%s/releases/tag/%s" % (repository, tag)
    require(release.get("html_url") == release_url, "release URL is noncanonical")
    require(PLACEHOLDER.search(json.dumps(release)) is None, "release contains a placeholder")

    assets_root = Path(assets_root)
    names = expected_asset_names()
    require(assets_root.is_dir(), "downloaded release asset directory is missing")
    downloaded = list(assets_root.iterdir())
    require(
        all(path.is_file() for path in downloaded)
        and {path.name for path in downloaded} == names,
        "downloaded release assets are missing or unexpected",
    )
    remote_assets = release.get("assets")
    require(isinstance(remote_assets, list), "release assets are invalid")
    by_name = {item.get("name"): item for item in remote_assets}
    require(len(by_name) == len(remote_assets), "release asset names are duplicated")
    require(set(by_name) == names, "release assets are missing or unexpected")
    local = {}
    for name in sorted(names):
        path = assets_root / name
        require(path.is_file(), "downloaded release asset is missing: %s" % name)
        size = path.stat().st_size
        digest = sha256(path)
        remote = by_name[name]
        require(remote.get("state") == "uploaded", "release asset is not uploaded: %s" % name)
        require(isinstance(remote.get("id"), int) and remote["id"] > 0, "release asset ID is invalid")
        require(remote.get("size") == size, "release asset size differs: %s" % name)
        require(remote.get("digest") == "sha256:" + digest, "release asset digest differs: %s" % name)
        require(
            remote.get("browser_download_url")
            == "https://github.com/%s/releases/download/%s/%s"
            % (repository, tag, name),
            "release asset URL is noncanonical: %s" % name,
        )
        local[name] = {"path": path, "bytes": size, "sha256": digest}

    evidence = read_json(local["release-evidence.json"]["path"])
    require(evidence.get("schema_version") == 2, "release evidence schema is invalid")
    require(evidence.get("source_sha") == source_sha, "release evidence source differs")
    require(evidence.get("package_version") == package_version, "release evidence version differs")
    require(evidence.get("mode") == "release" and evidence.get("published") is False,
            "release evidence publication state is invalid")
    lockfile = evidence.get("lockfile_sha256")
    require(publication.DIGEST.fullmatch(str(lockfile or "")), "release lockfile digest is invalid")
    gates = evidence.get("gates")
    require(
        isinstance(gates, list)
        and {item.get("gate") for item in gates} == dashboard.REQUIRED_RELEASE_GATES
        and all(item.get("status") == "pass" for item in gates),
        "release gates are incomplete or failed",
    )
    artifacts = evidence.get("artifacts")
    require(
        isinstance(artifacts, list)
        and {item.get("target") for item in artifacts} == dashboard.TARGETS
        and len(artifacts) == len(dashboard.TARGETS),
        "release native artifacts are incomplete",
    )
    result = {}
    for artifact in artifacts:
        target = artifact["target"]
        name = "tracky-%s.tar.xz" % target
        require(artifact.get("archive_name") == name, "evidence archive name differs")
        require(artifact.get("archive_bytes") == local[name]["bytes"], "evidence archive size differs")
        require(artifact.get("archive_sha256") == local[name]["sha256"], "evidence archive digest differs")
        semantic = artifact.get("semantic_manifest")
        dashboard.validate_semantic_archive_manifest(semantic)
        require(semantic["source_sha"] == source_sha, "semantic source differs")
        require(semantic["package_version"] == package_version, "semantic version differs")
        require(semantic["lockfile_sha256"] == lockfile, "semantic lockfile differs")
        require(semantic["target"] == target, "semantic target differs")
        require(
            semantic["transport"]
            == {
                "archive_name": name,
                "archive_bytes": local[name]["bytes"],
                "archive_sha256": local[name]["sha256"],
            },
            "semantic archive transport differs",
        )
        dashboard.verify_dist_checksum(local[name]["path"])
        result[target] = {
            **local[name],
            "name": name,
            "url": "https://github.com/%s/releases/download/%s/%s"
            % (repository, tag, name),
        }
    markdown = local["release-evidence.md"]["path"].read_text(encoding="utf-8")
    require(markdown.strip(), "release Markdown evidence is empty")
    return release_url, lockfile, result


def render_formula(repository, tag, package_version, archives):
    formula_version = tag.removeprefix("v")
    require(FORMULA_VERSION.fullmatch(formula_version), "formula version is invalid")
    mac = archives["aarch64-apple-darwin"]
    linux = archives["x86_64-unknown-linux-gnu"]
    return """class Tracky < Formula
  desc "Local-first, review-first personal finance CLI"
  homepage "https://github.com/%(repository)s"
  version "%(formula_version)s"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "%(mac_url)s"
      sha256 "%(mac_sha)s"
    end
  end

  on_linux do
    if Hardware::CPU.intel? && Hardware::CPU.is_64_bit?
      url "%(linux_url)s"
      sha256 "%(linux_sha)s"
    end
  end

  def install
    bin.install "tracky"
  end

  test do
    assert_match "tracky %(package_version)s", shell_output("#{bin}/tracky --version")
  end
end
""" % {
        "repository": repository,
        "formula_version": formula_version,
        "mac_url": mac["url"],
        "mac_sha": mac["sha256"],
        "linux_url": linux["url"],
        "linux_sha": linux["sha256"],
        "package_version": package_version,
    }


def preparation_evidence(repository, tag, source_sha, package_version, release_url, lockfile, archives, formula):
    value = {
        "schema_version": 1,
        "repository": repository,
        "tag": tag,
        "source_sha": source_sha,
        "package_version": package_version,
        "formula_version": tag.removeprefix("v"),
        "release_url": release_url,
        "lockfile_sha256": lockfile,
        "formula": {"path": "Formula/tracky.rb", "sha256": hashlib.sha256(formula.encode()).hexdigest()},
        "archives": [
            {
                "target": target,
                "system": TARGET_SYSTEM[target],
                "name": archives[target]["name"],
                "url": archives[target]["url"],
                "bytes": archives[target]["bytes"],
                "sha256": archives[target]["sha256"],
            }
            for target in sorted(archives)
        ],
    }
    require(PLACEHOLDER.search(json.dumps(value)) is None, "Homebrew evidence contains a placeholder")
    return value


def prepare(release_path, assets_root, repository, tag, source_sha, package_version):
    release = read_json(release_path)
    release_url, lockfile, archives = validate_inputs(
        release, assets_root, repository, tag, source_sha, package_version
    )
    formula = render_formula(repository, tag, package_version, archives)
    return formula, preparation_evidence(
        repository, tag, source_sha, package_version, release_url, lockfile, archives, formula
    )


def verify(formula_path, evidence_path, release_path, assets_root, repository, tag, source_sha, package_version):
    expected_formula, expected_evidence = prepare(
        release_path, assets_root, repository, tag, source_sha, package_version
    )
    require(Path(formula_path).read_text(encoding="utf-8") == expected_formula, "Homebrew formula differs")
    require(read_json(evidence_path) == expected_evidence, "Homebrew preparation evidence differs")
    return expected_evidence


def reconcile_tap(existing, desired, expected_previous=None):
    """Return create/update/noop while detecting a concurrent or unexpected drift."""
    if existing is None:
        require(expected_previous is None, "tap formula disappeared during reconciliation")
        return "create"
    if existing == desired:
        return "noop"
    if expected_previous is not None:
        require(existing == expected_previous, "tap formula changed during reconciliation")
    return "update"


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--release", type=Path)
    parser.add_argument("--assets-root", type=Path)
    parser.add_argument("--repository")
    parser.add_argument("--tag")
    parser.add_argument("--source-sha")
    parser.add_argument("--package-version")
    parser.add_argument("--formula", type=Path)
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--verify", action="store_true")
    parser.add_argument("--reconcile-existing", type=Path)
    parser.add_argument("--reconcile-desired", type=Path)
    args = parser.parse_args(argv)
    if args.reconcile_existing or args.reconcile_desired:
        require(
            args.reconcile_existing and args.reconcile_desired,
            "both reconciliation paths are required",
        )
        existing = (
            args.reconcile_existing.read_text(encoding="utf-8")
            if args.reconcile_existing.is_file()
            else None
        )
        desired = args.reconcile_desired.read_text(encoding="utf-8")
        print(reconcile_tap(existing, desired))
        return 0
    require(
        all(
            (
                args.release,
                args.assets_root,
                args.repository,
                args.tag,
                args.source_sha,
                args.package_version,
                args.formula,
                args.evidence,
            )
        ),
        "release preparation arguments are required",
    )
    common = (
        args.release, args.assets_root, args.repository, args.tag,
        args.source_sha, args.package_version,
    )
    if args.verify:
        verify(args.formula, args.evidence, *common)
    else:
        formula, value = prepare(*common)
        args.formula.parent.mkdir(parents=True, exist_ok=True)
        args.formula.write_text(formula, encoding="utf-8")
        args.evidence.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
