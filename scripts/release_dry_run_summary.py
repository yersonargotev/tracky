#!/usr/bin/env python3
"""Assemble exact-artifact evidence from a completed release dry run."""

import argparse
import json
import sys
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parent))
import dashboard_evidence as evidence  # noqa: E402


def assemble(verified_root, identity):
    evidence.require(
        set(identity) == {
            "schema_version", "source_sha", "package_version", "lockfile_sha256",
        }
        and identity["schema_version"] == 1,
        "release identity fields are invalid",
    )
    prefix = "release-dry-run-verified-%s-" % identity["source_sha"]
    directories = sorted(path for path in Path(verified_root).iterdir() if path.name.startswith(prefix))
    evidence.require(
        len(directories) == len(evidence.TARGETS),
        "dry run must contain exactly one bundle for every supported target",
    )

    artifacts = []
    found_targets = set()
    for directory in directories:
        target = directory.name[len(prefix):]
        evidence.require(target in evidence.TARGETS, "dry run bundle has unknown target")
        evidence.require(target not in found_targets, "dry run bundle target is duplicated")
        found_targets.add(target)
        candidate = directory / "candidate"
        recorded_identity = evidence.read_json(candidate / "release-identity.json")
        semantic = evidence.read_json(candidate / "semantic-manifest.json")
        dist_manifest = candidate / "dist-manifest.json"
        dist_manifest_value = evidence.read_json(dist_manifest)
        archive = candidate / ("tracky-%s.tar.xz" % target)
        runtime = evidence.read_json(directory / "runtime-evidence.json")

        evidence.require(recorded_identity == identity, "bundle release identity differs")
        evidence.validate_semantic_archive_manifest(semantic)
        evidence.require(semantic["source_sha"] == identity["source_sha"], "semantic source SHA differs")
        evidence.require(semantic["lockfile_sha256"] == identity["lockfile_sha256"], "semantic lockfile differs")
        evidence.require(semantic["package_version"] == identity["package_version"], "semantic package version differs")
        evidence.require(semantic["target"] == target, "semantic target differs")
        evidence.validate_cargo_dist_manifest(
            dist_manifest_value,
            target,
            identity["package_version"],
        )
        evidence.require(
            semantic["cargo_dist_manifest_sha256"] == evidence.hash_file(dist_manifest),
            "Cargo Dist manifest checksum differs",
        )
        evidence.require(archive.is_file(), "verified archive is missing")
        evidence.verify_dist_checksum(archive)
        evidence.require(
            semantic["transport"]["archive_bytes"] == archive.stat().st_size,
            "verified archive size differs",
        )
        archive_sha256 = evidence.hash_file(archive)
        evidence.require(
            semantic["transport"]["archive_sha256"] == archive_sha256,
            "verified archive checksum differs",
        )
        evidence.require(runtime.get("target") == target, "runtime evidence target differs")
        artifacts.append({
            "target": target,
            "archive_name": archive.name,
            "archive_bytes": archive.stat().st_size,
            "archive_sha256": archive_sha256,
            "cargo_dist_manifest_sha256": semantic["cargo_dist_manifest_sha256"],
        })

    evidence.require(found_targets == evidence.TARGETS, "dry run targets are incomplete")
    return {
        "schema_version": 1,
        "source_sha": identity["source_sha"],
        "package_version": identity["package_version"],
        "lockfile_sha256": identity["lockfile_sha256"],
        "mode": "dry-run",
        "published": False,
        "artifacts": sorted(artifacts, key=lambda item: item["target"]),
    }


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--verified-root", type=Path, required=True)
    parser.add_argument("--identity", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    value = assemble(args.verified_root, evidence.read_json(args.identity))
    args.output.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
