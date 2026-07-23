#!/usr/bin/env python3
"""Assemble per-run dashboard release evidence into one canonical manifest."""

import argparse
import json
from pathlib import Path

import dashboard_evidence as evidence


TARGET_FIELDS = {
    "target", "commit", "lockfile_sha256", "tools", "artifact", "latency",
    "resources", "size", "commands",
}


def _require_commands(commands, source):
    evidence.require(
        isinstance(commands, list)
        and commands
        and all(isinstance(command, str) and command for command in commands),
        "%s commands are required" % source,
    )


def assemble(targets_dir, browsers_path, results_path, inventory_path, maintainer, approved_by):
    fragments = []
    for path in sorted(Path(targets_dir).glob("*.json")):
        fragment = evidence.read_json(path)
        evidence.require(set(fragment) == TARGET_FIELDS, "invalid target fragment fields in %s" % path.name)
        fragments.append(fragment)
    evidence.require(
        len(fragments) == len(evidence.TARGETS),
        "targets directory must contain exactly %d JSON fragments" % len(evidence.TARGETS),
    )

    targets = [fragment["target"] for fragment in fragments]
    evidence.require(set(targets) == evidence.TARGETS and len(set(targets)) == len(targets), "target fragments must cover every target exactly once")
    fragments.sort(key=lambda item: item["target"])
    targets = [fragment["target"] for fragment in fragments]

    commit = fragments[0]["commit"]
    lockfile = fragments[0]["lockfile_sha256"]
    tools = fragments[0]["tools"]
    evidence.require(all(item["commit"] == commit for item in fragments), "target fragment commits differ")
    evidence.require(all(item["lockfile_sha256"] == lockfile for item in fragments), "target fragment lockfiles differ")
    evidence.require(all(item["tools"] == tools for item in fragments), "target fragment tool versions differ")

    artifacts = []
    sizes = []
    commands = []
    latency = {}
    resources = {}
    for fragment in fragments:
        target = fragment["target"]
        artifact = fragment["artifact"]
        evidence.require(set(artifact) == {"name", "sha256", "bytes"}, "invalid artifact fields for %s" % target)
        size = fragment["size"]
        evidence.require(set(size) == {"target", "archive_bytes", "archive_sha256", "executable_bytes", "executable_sha256"}, "invalid size fields for %s" % target)
        evidence.require(size["target"] == target, "size target differs from fragment target for %s" % target)
        artifacts.append({"target": target, **artifact})
        sizes.append(size)
        latency[target] = fragment["latency"]
        resources[target] = fragment["resources"]
        _require_commands(fragment["commands"], target)
        commands.extend(fragment["commands"])

    browser_input = evidence.read_json(browsers_path)
    evidence.require(set(browser_input) == {"browsers", "commands"}, "browser evidence fields must be browsers and commands")
    browsers = browser_input["browsers"]
    evidence.require(isinstance(browsers, dict) and set(browsers) == evidence.REQUIRED_RELEASE_BROWSERS, "browser evidence must contain exactly the six required browsers")
    _require_commands(browser_input["commands"], "browser evidence")
    commands.extend(browser_input["commands"])

    results = evidence.read_json(results_path)
    evidence.require(isinstance(results, list), "results evidence must be a JSON list")
    evidence.require(len(results) == len(evidence.REQUIRED_RELEASE_GATES) and {item.get("gate") for item in results} == evidence.REQUIRED_RELEASE_GATES, "results evidence must contain exactly the twelve required gates")
    evidence.require(all(item.get("status") == "pass" for item in results), "every release gate must pass")
    evidence.require(all(set(item) == {"gate", "status", "evidence"} and isinstance(item.get("evidence"), str) and evidence.RETAINED_EVIDENCE_URL.fullmatch(item["evidence"]) for item in results), "release gates must contain retained GitHub Actions evidence URLs")

    inventory = evidence.read_json(inventory_path)
    evidence.require(set(inventory) == {"resolved_package_count", "asset_bytes"}, "inventory must contain exactly resolved_package_count and asset_bytes")
    manifest = {
        "schema_version": 1,
        "commit": commit,
        "lockfile_sha256": lockfile,
        "tools": tools,
        "targets": targets,
        "browsers": browsers,
        "artifacts": artifacts,
        "commands": commands,
        "measurements": {
            "latency": latency,
            "resources": resources,
            "sizes": {"schema_version": 1, **inventory, "targets": sizes},
        },
        "results": sorted(results, key=lambda item: item["gate"]),
        "responsible_maintainer": maintainer,
        "approval": {"approved": True, "approved_by": approved_by},
    }
    evidence.validate_manifest(manifest, release=True, expected_commit=commit, expected_lockfile_sha256=lockfile)
    return manifest


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--targets-dir", type=Path, required=True)
    parser.add_argument("--browsers", type=Path, required=True)
    parser.add_argument("--results", type=Path, required=True)
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--maintainer", required=True)
    parser.add_argument("--approved-by", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    manifest = assemble(args.targets_dir, args.browsers, args.results, args.inventory, args.maintainer, args.approved_by)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(evidence.canonical_json(manifest), encoding="utf-8")


if __name__ == "__main__":
    main()
