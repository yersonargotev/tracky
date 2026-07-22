#!/usr/bin/env python3
"""Deterministic evidence, inventory, and size gates for Tracky's dashboard."""

import argparse
import hashlib
import json
import subprocess
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "evidence" / "dashboard"
BASELINE = EVIDENCE / "baseline.json"
TEMPLATE = EVIDENCE / "dashboard-verification.template.json"
SCHEMA = EVIDENCE / "dashboard-verification.schema.json"
INVENTORY = EVIDENCE / "dependency-inventory.json"
NOTICES_FILE = ROOT / "THIRD-PARTY-NOTICES"
ASSETS = ROOT / "src" / "dashboard_assets"
TARGETS = {
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
}
HASH_LENGTH = 64
MAX_ASSET_BYTES = 250 * 1024
MAX_DEPENDENCY_DELTA = 60
MAX_BINARY_BYTES_DELTA = int(2.5 * 1024 * 1024)
MAX_ARCHIVE_BYTES_DELTA = 1024 * 1024
MAX_BINARY_RATIO = 1.20
MAX_ARCHIVE_RATIO = 1.20
REQUIRED_RELEASE_GATES = {
    "semantic-conformance",
    "database-immutability",
    "http-security",
    "process-lifecycle",
    "browser-flows",
    "accessibility-automation",
    "dependency-policy",
    "static-and-artifact-budgets",
    "packaged-security",
    "performance-and-resources",
    "archive-and-installers",
    "manual-accessibility",
}
REQUIRED_RELEASE_BROWSERS = {"safari", "firefox-esr", "chromium"}
REQUIRED_MEASUREMENT_GROUPS = {"latency", "resources", "sizes"}


def read_json(path):
    with Path(path).open(encoding="utf-8") as handle:
        return json.load(handle)


def canonical_json(value):
    return json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def validate_baseline(value):
    require(value.get("kind") == "dashboard-free-cargo-dist-baseline", "invalid baseline kind")
    require(len(value.get("commit", "")) == 40, "baseline commit must be a full SHA")
    require(len(value.get("lockfile_sha256", "")) == HASH_LENGTH, "invalid lockfile hash")
    require(value.get("resolved_package_count", 0) > 0, "missing package count")
    targets = value.get("targets", [])
    require({item.get("target") for item in targets} == TARGETS, "baseline targets differ from Cargo Dist targets")
    for item in targets:
        for field in ("archive_bytes", "executable_bytes"):
            require(item.get(field, 0) > 0, "%s missing for %s" % (field, item.get("target")))
        for field in ("archive_sha256", "executable_sha256"):
            require(len(item.get(field, "")) == HASH_LENGTH, "%s invalid for %s" % (field, item.get("target")))
        require(item.get("archive_contents") == ["LICENSE", "README.md", "tracky"], "unexpected baseline archive contents")
    require(value.get("reproduce"), "baseline reproduction commands are required")


def validate_manifest(value, release=False):
    required = {
        "schema_version", "commit", "lockfile_sha256", "tools", "targets", "browsers",
        "artifacts", "commands", "measurements", "results", "responsible_maintainer", "approval",
    }
    require(set(value) == required, "manifest fields do not match the schema")
    require(value["schema_version"] == 1, "unsupported manifest schema")
    require(len(value["commit"]) == 40 and all(c in "0123456789abcdef" for c in value["commit"]), "manifest commit must be a full SHA")
    require(len(value["lockfile_sha256"]) == HASH_LENGTH and all(c in "0123456789abcdef" for c in value["lockfile_sha256"]), "invalid manifest lockfile hash")
    require(
        isinstance(value["targets"], list)
        and len(value["targets"]) == len(TARGETS)
        and set(value["targets"]) == TARGETS,
        "manifest must name every supported target exactly once",
    )
    require(isinstance(value["tools"], dict) and value["tools"] and all(isinstance(name, str) and isinstance(version, str) and name and version for name, version in value["tools"].items()), "manifest tool versions are required")
    require(isinstance(value["browsers"], dict) and all(isinstance(name, str) and isinstance(version, str) and name and version for name, version in value["browsers"].items()), "invalid browser versions")
    require(isinstance(value["commands"], list) and value["commands"] and all(isinstance(command, str) and command for command in value["commands"]), "manifest commands are required")
    require(isinstance(value["measurements"], dict), "manifest measurements must be an object")
    require(isinstance(value["results"], list) and value["results"], "manifest results are required")
    require(isinstance(value["responsible_maintainer"], str) and value["responsible_maintainer"], "responsible maintainer is required")
    require(set(value["approval"]) == {"approved", "approved_by"} and isinstance(value["approval"]["approved"], bool), "invalid approval fields")
    for artifact in value["artifacts"]:
        require(set(artifact) == {"target", "name", "sha256", "bytes"}, "invalid artifact fields")
        require(artifact["target"] in TARGETS, "artifact has unknown target")
        require(isinstance(artifact["name"], str) and artifact["name"], "artifact name is required")
        require(isinstance(artifact["sha256"], str) and len(artifact["sha256"]) == HASH_LENGTH and all(c in "0123456789abcdef" for c in artifact["sha256"]), "invalid artifact hash")
        require(isinstance(artifact["bytes"], int) and not isinstance(artifact["bytes"], bool) and artifact["bytes"] >= 0, "invalid artifact size")
    for result in value["results"]:
        require(set(result) == {"gate", "status", "evidence"}, "invalid result fields")
        require(result["status"] in {"pass", "fail", "not_run"}, "invalid result status")
        require(bool(result["gate"] and result["evidence"]), "result gate and evidence are required")
    if release:
        require(REQUIRED_RELEASE_BROWSERS <= set(value["browsers"]), "release browser versions are incomplete")
        require(REQUIRED_MEASUREMENT_GROUPS <= set(value["measurements"]), "release measurements are incomplete")
        require(len(value["artifacts"]) == len(TARGETS) and {item["target"] for item in value["artifacts"]} == TARGETS, "release artifacts must cover every target exactly once")
        require(len(value["results"]) == len(REQUIRED_RELEASE_GATES) and {item["gate"] for item in value["results"]} == REQUIRED_RELEASE_GATES, "release result matrix is incomplete")
        require(all(item["status"] == "pass" for item in value["results"]), "release evidence contains an incomplete or failed gate")
        require(value["approval"].get("approved") is True, "release evidence is not approved")
        require(bool(value["approval"].get("approved_by")), "release approval identity is required")
        require(value["responsible_maintainer"] != "unassigned", "responsible maintainer identity is required")


def validate_schema_contract(schema):
    properties = schema["properties"]
    require(set(schema["required"]) == {
        "schema_version", "commit", "lockfile_sha256", "tools", "targets", "browsers",
        "artifacts", "commands", "measurements", "results", "responsible_maintainer", "approval",
    }, "JSON Schema top-level fields drifted from the CI validator")
    require(set(properties["targets"]["items"]["enum"]) == TARGETS, "JSON Schema targets drifted from the CI validator")
    require(set(schema["$defs"]["artifact"]["required"]) == {"target", "name", "sha256", "bytes"}, "JSON Schema artifact fields drifted from the CI validator")
    require(set(schema["$defs"]["artifact"]["properties"]["target"]["enum"]) == TARGETS, "JSON Schema artifact targets drifted from the CI validator")
    require(set(schema["$defs"]["result"]["properties"]["status"]["enum"]) == {"pass", "fail", "not_run"}, "JSON Schema result statuses drifted from the CI validator")


def dependency_inventory():
    output = subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"], cwd=ROOT, text=True
    )
    metadata = json.loads(output)
    resolved = {node["id"] for node in metadata["resolve"]["nodes"]}
    packages = []
    for package in metadata["packages"]:
        if package["id"] not in resolved or package["name"] == "tracky":
            continue
        packages.append({
            "name": package["name"],
            "version": package["version"],
            "license": package.get("license") or "UNKNOWN",
            "repository": package.get("repository"),
            "source": package.get("source"),
        })
    packages.sort(key=lambda item: (item["name"], item["version"], item["source"] or ""))
    lock_hash = hashlib.sha256((ROOT / "Cargo.lock").read_bytes()).hexdigest()
    inventory = {"schema_version": 1, "lockfile_sha256": lock_hash, "packages": packages}
    return canonical_json(inventory)


def write_or_check(path, content, check):
    if check:
        require(path.exists() and path.read_text(encoding="utf-8") == content, "%s is stale; regenerate inventory" % path)
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")


def compare_measurements(current, baseline):
    require(current.get("schema_version") == 1, "unsupported current measurement schema")
    require(current.get("asset_bytes", -1) >= 0, "asset byte measurement is required")
    require(current["asset_bytes"] <= MAX_ASSET_BYTES, "embedded static assets exceed 250 KiB")
    require(current.get("resolved_package_count", 0) > 0, "resolved package count is required")
    require(current["resolved_package_count"] - baseline["resolved_package_count"] <= MAX_DEPENDENCY_DELTA, "dependency delta exceeds 60")
    baseline_targets = {item["target"]: item for item in baseline["targets"]}
    require({item.get("target") for item in current.get("targets", [])} == TARGETS, "current measurements must include every target")
    for item in current["targets"]:
        require(item.get("archive_bytes", 0) > 0 and item.get("executable_bytes", 0) > 0, "artifact sizes are required for %s" % item.get("target"))
        require(len(item.get("archive_sha256", "")) == HASH_LENGTH, "archive hash is required for %s" % item.get("target"))
        require(len(item.get("executable_sha256", "")) == HASH_LENGTH, "executable hash is required for %s" % item.get("target"))
        frozen = baseline_targets[item["target"]]
        binary_delta = item["executable_bytes"] - frozen["executable_bytes"]
        archive_delta = item["archive_bytes"] - frozen["archive_bytes"]
        require(binary_delta <= MAX_BINARY_BYTES_DELTA and item["executable_bytes"] <= frozen["executable_bytes"] * MAX_BINARY_RATIO, "binary budget exceeded for %s" % item["target"])
        require(archive_delta <= MAX_ARCHIVE_BYTES_DELTA and item["archive_bytes"] <= frozen["archive_bytes"] * MAX_ARCHIVE_RATIO, "archive budget exceeded for %s" % item["target"])


def hash_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def measure(artifacts, assets):
    packages = read_json(INVENTORY)["packages"]
    targets = []
    for target in sorted(TARGETS):
        archive = artifacts / ("tracky-%s.tar.xz" % target)
        require(archive.is_file(), "missing Cargo Dist archive %s" % archive)
        with tarfile.open(archive, "r:xz") as bundle:
            members = [member for member in bundle.getmembers() if member.isfile()]
            executable = next(member for member in members if Path(member.name).name == "tracky")
            executable_file = bundle.extractfile(executable)
            require(executable_file is not None, "missing executable in %s" % archive)
            executable_hash = hashlib.sha256(executable_file.read()).hexdigest()
        targets.append({
            "target": target,
            "archive_bytes": archive.stat().st_size,
            "archive_sha256": hash_file(archive),
            "executable_bytes": executable.size,
            "executable_sha256": executable_hash,
            "archive_contents": sorted(Path(member.name).name for member in members),
        })
    asset_bytes = 0
    if assets:
        asset_bytes = sum(path.stat().st_size for path in assets.rglob("*") if path.is_file() and path.suffix in {".html", ".css", ".js"})
    return {"schema_version": 1, "resolved_package_count": len(packages) + 1, "asset_bytes": asset_bytes, "targets": targets}


def render_manifest(value):
    lines = [
        "# Dashboard verification", "",
        "- Commit: `%s`" % value["commit"],
        "- Lockfile: `%s`" % value["lockfile_sha256"],
        "- Maintainer: %s" % value["responsible_maintainer"],
        "- Approved: %s" % ("yes" if value["approval"]["approved"] else "no"),
        "", "## Tools", "",
    ]
    lines.extend("- %s: `%s`" % (name, version) for name, version in sorted(value["tools"].items()))
    lines.extend(["", "## Targets", ""] + ["- `%s`" % target for target in value["targets"]])
    lines.extend(["", "## Browsers", ""])
    lines.extend("- %s: `%s`" % (name, version) for name, version in sorted(value["browsers"].items()))
    if not value["browsers"]:
        lines.append("- Not recorded in this implementation slice.")
    lines.extend(["", "## Artifacts", ""])
    lines.extend(
        "- `%s` / `%s`: %s bytes, `%s`" % (artifact["target"], artifact["name"], artifact["bytes"], artifact["sha256"])
        for artifact in value["artifacts"]
    )
    if not value["artifacts"]:
        lines.append("- Not recorded in this implementation slice.")
    lines.extend(["", "## Measurements", "", "```json", canonical_json(value["measurements"]).rstrip(), "```"])
    lines.extend(["", "## Results", ""])
    for result in value["results"]:
        lines.append("- **%s** — `%s`: %s" % (result["gate"], result["status"], result["evidence"]))
    lines.extend(["", "## Commands", ""] + ["- `%s`" % command for command in value["commands"]])
    return "\n".join(lines) + "\n"


def check_all():
    baseline = read_json(BASELINE)
    validate_baseline(baseline)
    validate_schema_contract(read_json(SCHEMA))
    validate_manifest(read_json(TEMPLATE))
    write_or_check(INVENTORY, dependency_inventory(), True)
    require(NOTICES_FILE.exists() and "THIRD-PARTY NOTICES" in NOTICES_FILE.read_text(encoding="utf-8"), "THIRD-PARTY-NOTICES is missing")
    compare_static(ASSETS, baseline)


def compare_static(assets, baseline):
    package_count = len(read_json(INVENTORY)["packages"]) + 1
    asset_bytes = 0
    if assets.exists():
        asset_bytes = sum(path.stat().st_size for path in assets.rglob("*") if path.is_file() and path.suffix in {".html", ".css", ".js"})
    require(package_count - baseline["resolved_package_count"] <= MAX_DEPENDENCY_DELTA, "dependency delta exceeds 60")
    require(asset_bytes <= MAX_ASSET_BYTES, "embedded static assets exceed 250 KiB")


def main(argv=None):
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("check")
    inventory = sub.add_parser("inventory")
    inventory.add_argument("--check", action="store_true")
    validate = sub.add_parser("validate")
    validate.add_argument("manifest", type=Path)
    validate.add_argument("--release", action="store_true")
    render = sub.add_parser("render")
    render.add_argument("manifest", type=Path)
    render.add_argument("--output", type=Path, required=True)
    compare = sub.add_parser("compare")
    compare.add_argument("--current", type=Path, required=True)
    measurement = sub.add_parser("measure")
    measurement.add_argument("--artifacts", type=Path, required=True)
    measurement.add_argument("--assets", type=Path, default=ASSETS)
    measurement.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.command == "check":
        check_all()
    elif args.command == "inventory":
        write_or_check(INVENTORY, dependency_inventory(), args.check)
    elif args.command == "validate":
        validate_manifest(read_json(args.manifest), args.release)
    elif args.command == "render":
        value = read_json(args.manifest)
        validate_manifest(value)
        args.output.write_text(render_manifest(value), encoding="utf-8")
    elif args.command == "compare":
        baseline = read_json(BASELINE)
        validate_baseline(baseline)
        compare_measurements(read_json(args.current), baseline)
    elif args.command == "measure":
        args.output.write_text(canonical_json(measure(args.artifacts, args.assets)), encoding="utf-8")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print("dashboard evidence error: %s" % error, file=sys.stderr)
        sys.exit(1)
