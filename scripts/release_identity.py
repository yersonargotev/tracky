#!/usr/bin/env python3
"""Validate and record the immutable identity of one Tracky release run."""

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path


def package_version(manifest):
    in_package = False
    for raw_line in manifest.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if line.startswith("[") and line.endswith("]"):
            in_package = line == "[package]"
            continue
        if in_package:
            match = re.fullmatch(r'version\s*=\s*"([^"]+)"(?:\s*#.*)?', line)
            if match:
                return match.group(1)
    raise ValueError("Cargo.toml [package] version is missing")


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def record_release_identity(root):
    root = Path(root)
    actual_sha = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"],
        text=True,
    ).strip()
    actual_version = package_version(root / "Cargo.toml")
    actual_lockfile = sha256(root / "Cargo.lock")
    if re.fullmatch(r"[0-9a-f]{40}", actual_sha) is None:
        raise ValueError("checked-out commit is not a full lowercase SHA")
    if re.fullmatch(r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?", actual_version) is None:
        raise ValueError("checked-out package version is not SemVer")
    return {
        "schema_version": 1,
        "source_sha": actual_sha,
        "package_version": actual_version,
        "lockfile_sha256": actual_lockfile,
    }


def validate_release_identity(root, accepted_sha, expected_version, expected_lockfile):
    if re.fullmatch(r"[0-9a-f]{40}", accepted_sha) is None:
        raise ValueError("accepted SHA must be exactly 40 lowercase hexadecimal characters")
    if re.fullmatch(r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?", expected_version) is None:
        raise ValueError("package version must be SemVer")
    if re.fullmatch(r"[0-9a-f]{64}", expected_lockfile) is None:
        raise ValueError("lockfile digest must be lowercase SHA-256")

    actual = record_release_identity(root)
    if actual["source_sha"] != accepted_sha:
        raise ValueError("checked-out commit differs from the accepted SHA")
    if actual["package_version"] != expected_version:
        raise ValueError("package version differs from the accepted version")
    if actual["lockfile_sha256"] != expected_lockfile:
        raise ValueError("lockfile digest differs from the accepted lockfile")
    return actual


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path, default=Path.cwd())
    parser.add_argument("--derive", action="store_true")
    parser.add_argument("--accepted-sha")
    parser.add_argument("--package-version")
    parser.add_argument("--lockfile-sha256")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    expected = (args.accepted_sha, args.package_version, args.lockfile_sha256)
    if args.derive:
        if any(value is not None for value in expected):
            parser.error("--derive cannot be combined with accepted identity fields")
        value = record_release_identity(args.source_root)
    else:
        if any(value is None for value in expected):
            parser.error("accepted SHA, package version, and lockfile digest are required")
        value = validate_release_identity(args.source_root, *expected)
    args.output.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
