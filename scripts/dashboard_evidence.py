#!/usr/bin/env python3
"""Deterministic evidence, inventory, and size gates for Tracky's dashboard."""

import argparse
import hashlib
import json
import re
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
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
    "x86_64-unknown-linux-gnu",
}
# The immutable dashboard-free baseline predates the Intel support removal.
FROZEN_BASELINE_TARGETS = TARGETS | {"x86_64-apple-darwin"}
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
}
REQUIRED_RELEASE_BROWSERS = {
    "safari-minimum", "safari-latest", "firefox-esr-minimum",
    "firefox-latest", "chromium-minimum", "chromium-latest",
}
RETAINED_EVIDENCE_URL = re.compile(
    r"^https://github\.com/yersonargotev/tracky/actions/runs/[1-9]\d*"
    r"(?:/job/[1-9]\d*)?(?:[?#].*)?$"
)
BROWSER_MINIMUMS = {
    "safari-minimum": (26, 0),
    "firefox-esr-minimum": (153,),
    "chromium-minimum": (150,),
}
REQUIRED_MEASUREMENT_GROUPS = {"latency", "resources", "sizes"}
REQUIRED_ARCHIVE_FILES = {"tracky", "README.md", "LICENSE", "THIRD-PARTY-NOTICES"}
LATENCY_LIMITS_MS = {
    "readiness_p95_ms": 500,
    "initial_snapshot_p95_ms": 1_500,
    "refresh_p95_ms": 1_500,
    "navigation_p95_ms": 2_000,
    "drill_down_p95_ms": 250,
    "filter_interaction_p95_ms": 100,
}
RESOURCE_LIMITS = {
    "idle_rss_bytes": 64 * 1024 * 1024,
    "peak_rss_bytes": 128 * 1024 * 1024,
    "idle_cpu_percent": 1,
    "threads": 8,
    "descriptors": 32,
}
def read_json(path):
    with Path(path).open(encoding="utf-8") as handle:
        return json.load(handle)


def canonical_json(value):
    return json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def numeric(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def version_tuple(value):
    match = re.search(r"\d+(?:\.\d+)*", value)
    require(match is not None, "browser version must contain a numeric version")
    return tuple(int(part) for part in match.group().split("."))


def validate_release_measurements(measurements, artifacts):
    require(set(measurements) == REQUIRED_MEASUREMENT_GROUPS, "release measurements are incomplete")
    latency = measurements["latency"]
    resources = measurements["resources"]
    require(set(latency) == TARGETS, "latency measurements must cover every target")
    require(set(resources) == TARGETS, "resource measurements must cover every target")
    for target in TARGETS:
        target_latency = latency[target]
        require(
            set(target_latency) == set(LATENCY_LIMITS_MS) | {"warmups", "runs"},
            "latency metrics are incomplete for %s" % target,
        )
        require(
            numeric(target_latency["warmups"])
            and numeric(target_latency["runs"])
            and target_latency["warmups"] >= 5
            and target_latency["runs"] >= 30,
            "latency sample count is incomplete for %s" % target,
        )
        for name, limit in LATENCY_LIMITS_MS.items():
            require(numeric(target_latency[name]) and 0 <= target_latency[name] <= limit, "%s exceeds its release budget for %s" % (name, target))

        target_resources = resources[target]
        expected_resources = set(RESOURCE_LIMITS) | {
            "cycles", "descriptor_growth", "memory_growth_bytes", "memory_growth_percent",
        }
        require(set(target_resources) == expected_resources, "resource metrics are incomplete for %s" % target)
        for name, limit in RESOURCE_LIMITS.items():
            require(numeric(target_resources[name]) and 0 <= target_resources[name] <= limit, "%s exceeds its release budget for %s" % (name, target))
        require(numeric(target_resources["cycles"]) and target_resources["cycles"] >= 100, "resource cycle count is incomplete for %s" % target)
        require(numeric(target_resources["descriptor_growth"]) and target_resources["descriptor_growth"] <= 0, "descriptor growth detected for %s" % target)
        require(
            (numeric(target_resources["memory_growth_bytes"]) and target_resources["memory_growth_bytes"] <= 8 * 1024 * 1024)
            or (numeric(target_resources["memory_growth_percent"]) and target_resources["memory_growth_percent"] <= 5),
            "memory growth exceeds its release budget for %s" % target,
        )

    sizes = measurements["sizes"]
    baseline = read_json(BASELINE)
    validate_baseline(baseline)
    compare_measurements(sizes, baseline)
    recorded_sizes = {item["target"]: item for item in sizes["targets"]}
    for artifact in artifacts:
        measured = recorded_sizes[artifact["target"]]
        require(
            artifact["bytes"] == measured["archive_bytes"]
            and artifact["sha256"] == measured["archive_sha256"],
            "artifact and size evidence differ for %s" % artifact["target"],
        )


def validate_baseline(value):
    require(value.get("kind") == "dashboard-free-cargo-dist-baseline", "invalid baseline kind")
    require(len(value.get("commit", "")) == 40, "baseline commit must be a full SHA")
    require(len(value.get("lockfile_sha256", "")) == HASH_LENGTH, "invalid lockfile hash")
    require(value.get("resolved_package_count", 0) > 0, "missing package count")
    targets = value.get("targets", [])
    require(
        {item.get("target") for item in targets} == FROZEN_BASELINE_TARGETS,
        "frozen baseline targets changed",
    )
    for item in targets:
        for field in ("archive_bytes", "executable_bytes"):
            require(item.get(field, 0) > 0, "%s missing for %s" % (field, item.get("target")))
        for field in ("archive_sha256", "executable_sha256"):
            require(len(item.get(field, "")) == HASH_LENGTH, "%s invalid for %s" % (field, item.get("target")))
        require(item.get("archive_contents") == ["LICENSE", "README.md", "tracky"], "unexpected baseline archive contents")
    require(value.get("reproduce"), "baseline reproduction commands are required")


def validate_manifest(
    value,
    release=False,
    expected_commit=None,
    expected_lockfile_sha256=None,
):
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
        require(expected_commit is not None, "release validation requires the accepted commit")
        require(
            value["commit"] == expected_commit,
            "release evidence does not match the accepted commit",
        )
        require(
            expected_lockfile_sha256 is not None,
            "release validation requires the accepted lockfile",
        )
        require(
            value["lockfile_sha256"] == expected_lockfile_sha256,
            "release evidence does not match the accepted lockfile",
        )
        require(set(value["browsers"]) == REQUIRED_RELEASE_BROWSERS, "release browser versions are incomplete")
        for name, minimum in BROWSER_MINIMUMS.items():
            require(version_tuple(value["browsers"][name]) >= minimum, "%s is below the supported minimum" % name)
        require(version_tuple(value["browsers"]["safari-latest"]) >= BROWSER_MINIMUMS["safari-minimum"], "latest Safari evidence is invalid")
        require(version_tuple(value["browsers"]["firefox-latest"]) >= BROWSER_MINIMUMS["firefox-esr-minimum"], "latest Firefox evidence is invalid")
        require(version_tuple(value["browsers"]["chromium-latest"]) >= BROWSER_MINIMUMS["chromium-minimum"], "latest Chromium evidence is invalid")
        require(len(value["artifacts"]) == len(TARGETS) and {item["target"] for item in value["artifacts"]} == TARGETS, "release artifacts must cover every target exactly once")
        require(
            all(
                item["name"] == "tracky-%s.tar.xz" % item["target"]
                for item in value["artifacts"]
            ),
            "release artifacts must use the exact Cargo Dist archive names",
        )
        require(len(value["results"]) == len(REQUIRED_RELEASE_GATES) and {item["gate"] for item in value["results"]} == REQUIRED_RELEASE_GATES, "release result matrix is incomplete")
        require(all(item["status"] == "pass" for item in value["results"]), "release evidence contains an incomplete or failed gate")
        require(
            all(RETAINED_EVIDENCE_URL.fullmatch(item["evidence"]) for item in value["results"]),
            "release results must link retained Tracky Actions evidence",
        )
        validate_release_measurements(value["measurements"], value["artifacts"])
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
    require(set(properties["browsers"]["properties"]) == REQUIRED_RELEASE_BROWSERS, "JSON Schema browsers drifted from the CI validator")
    require(set(properties["measurements"]["properties"]) == REQUIRED_MEASUREMENT_GROUPS, "JSON Schema measurements drifted from the CI validator")
    require(set(schema["$defs"]["artifact"]["required"]) == {"target", "name", "sha256", "bytes"}, "JSON Schema artifact fields drifted from the CI validator")
    require(set(schema["$defs"]["artifact"]["properties"]["target"]["enum"]) == TARGETS, "JSON Schema artifact targets drifted from the CI validator")
    require(schema["$defs"]["artifact"]["properties"]["name"].get("pattern"), "JSON Schema artifact names must be constrained")
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


def verify_executable_architecture(content, target):
    require(len(content) >= 20, "archive executable header is incomplete")
    if target.endswith("apple-darwin"):
        require(content[:4] == b"\xcf\xfa\xed\xfe", "archive executable is not a 64-bit little-endian Mach-O")
        cpu_type = struct.unpack("<I", content[4:8])[0]
        expected_cpu = 0x0100000C if target.startswith("aarch64") else 0x01000007
        require(cpu_type == expected_cpu, "archive executable architecture does not match %s" % target)
    else:
        require(content[:6] == b"\x7fELF\x02\x01", "archive executable is not a 64-bit little-endian ELF")
        require(struct.unpack("<H", content[18:20])[0] == 62, "archive executable architecture does not match %s" % target)


def inspect_release_archive_contents(archive, target, expected_root=ROOT):
    require(target in TARGETS, "archive has unknown target")
    with tarfile.open(archive, "r:xz") as bundle:
        members = bundle.getmembers()
        files = [member for member in members if member.isfile()]
        archive_root = "tracky-%s" % target
        require(
            all(not member.issym() and not member.islnk() for member in members),
            "archive must not contain links",
        )
        require(
            all(member.isdir() or member.isfile() for member in members),
            "archive contains a non-file entry",
        )
        require(
            all(
                member.name
                and "\\" not in member.name
                and ".." not in Path(member.name).parts
                and not Path(member.name).is_absolute()
                for member in members
            ),
            "archive contains an unsafe path",
        )
        expected_paths = {"%s/%s" % (archive_root, name) for name in REQUIRED_ARCHIVE_FILES}
        names = [member.name.rstrip("/") for member in files]
        require(
            len(names) == len(expected_paths) and set(names) == expected_paths,
            "archive contents differ from the release allowlist",
        )
        require(
            {member.name.rstrip("/") for member in members if member.isdir()} <= {archive_root},
            "archive directory layout differs from Cargo Dist",
        )
        file_records = []
        executable = None
        executable_content = None
        for member in sorted(files, key=lambda item: item.name):
            name = Path(member.name).name
            expected_mode = 0o755 if name == "tracky" else 0o644
            require(
                stat.S_IMODE(member.mode) == expected_mode,
                "%s permissions must be %04o" % (name, expected_mode),
            )
            packaged = bundle.extractfile(member)
            require(packaged is not None, "%s could not be read from archive" % name)
            content = packaged.read()
            if name == "tracky":
                executable = member
                executable_content = content
                verify_executable_architecture(content, target)
            else:
                require(
                    content == (expected_root / name).read_bytes(),
                    "%s in archive differs from the accepted source" % name,
                )
            file_records.append({
                "path": name,
                "bytes": member.size,
                "sha256": hashlib.sha256(content).hexdigest(),
                "mode": "%04o" % expected_mode,
            })
        require(executable is not None and executable_content is not None, "archive executable is missing")
    measurement = {
        "target": target,
        "archive_bytes": archive.stat().st_size,
        "archive_sha256": hash_file(archive),
        "executable_bytes": executable.size,
        "executable_sha256": hashlib.sha256(executable_content).hexdigest(),
        "archive_contents": sorted(Path(name).name for name in names),
    }
    return measurement, file_records, executable_content


def inspect_release_archive(archive, target, expected_root=ROOT):
    measurement, _, _ = inspect_release_archive_contents(archive, target, expected_root)
    return measurement


def packaged_version(executable_content):
    with tempfile.TemporaryDirectory() as raw:
        executable = Path(raw) / "tracky"
        executable.write_bytes(executable_content)
        executable.chmod(0o755)
        result = subprocess.run(
            [str(executable), "--version"],
            check=True,
            capture_output=True,
            text=True,
        )
        return result.stdout.strip()


def validate_cargo_dist_manifest(value, target, package_version):
    require(value.get("dist_version") == "0.32.0", "Cargo Dist manifest version differs")
    artifacts = value.get("artifacts")
    require(isinstance(artifacts, dict), "Cargo Dist manifest artifacts are missing")
    archive_name = "tracky-%s.tar.xz" % target
    checksum_name = archive_name + ".sha256"
    require(
        set(artifacts) == {archive_name, checksum_name},
        "Cargo Dist manifest artifacts are not target-bound",
    )
    archive = artifacts.get(archive_name)
    checksum = artifacts.get(checksum_name)
    require(isinstance(archive, dict), "Cargo Dist manifest archive is missing")
    require(isinstance(checksum, dict), "Cargo Dist manifest checksum is missing")
    require(
        archive.get("name") == archive_name
        and archive.get("kind") == "executable-zip"
        and archive.get("target_triples") == [target]
        and archive.get("checksum") == checksum_name,
        "Cargo Dist manifest archive contract differs",
    )
    require(
        checksum.get("name") == checksum_name
        and checksum.get("kind") == "checksum"
        and checksum.get("target_triples") == [target],
        "Cargo Dist manifest checksum contract differs",
    )
    releases = value.get("releases")
    require(
        isinstance(releases, list) and len(releases) == 1,
        "Cargo Dist manifest must contain exactly one release",
    )
    release = releases[0]
    require(
        release.get("app_name") == "tracky"
        and release.get("app_version") == package_version
        and set(release.get("artifacts", [])) == {archive_name, checksum_name},
        "Cargo Dist manifest release linkage differs",
    )

    def reject_local_paths(item):
        if isinstance(item, dict):
            for key, nested in item.items():
                if key == "path" and isinstance(nested, str):
                    path = Path(nested)
                    require(
                        not path.is_absolute() and ".." not in path.parts,
                        "Cargo Dist manifest contains a local artifact path",
                    )
                reject_local_paths(nested)
        elif isinstance(item, list):
            for nested in item:
                reject_local_paths(nested)

    reject_local_paths(value)


def validate_semantic_provenance(
    source_sha,
    lockfile_sha256,
    cargo_dist_manifest_sha256,
    package_version,
    tools,
):
    require(
        re.fullmatch(r"[0-9a-f]{40}", source_sha) is not None,
        "source SHA must be a full lowercase commit SHA",
    )
    require(
        re.fullmatch(r"[0-9a-f]{64}", lockfile_sha256) is not None,
        "lockfile digest must be lowercase SHA-256",
    )
    require(
        re.fullmatch(r"[0-9a-f]{64}", cargo_dist_manifest_sha256) is not None,
        "Cargo Dist manifest digest must be lowercase SHA-256",
    )
    require(
        re.fullmatch(r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?", package_version) is not None,
        "package version must be SemVer",
    )
    require(
        isinstance(tools, dict)
        and set(tools) == {"rust", "cargo", "cargo-dist"}
        and tools == dict(sorted(tools.items()))
        and all(isinstance(value, str) and value for value in tools.values()),
        "Rust, Cargo, and Cargo Dist versions are required in deterministic order",
    )


def semantic_archive_manifest(
    archive,
    target,
    source_sha,
    lockfile_sha256,
    cargo_dist_manifest_sha256,
    package_version,
    tools,
    expected_root=ROOT,
    version_probe=None,
):
    tools = dict(sorted(tools.items()))
    validate_semantic_provenance(
        source_sha,
        lockfile_sha256,
        cargo_dist_manifest_sha256,
        package_version,
        tools,
    )
    measurement, files, executable_content = inspect_release_archive_contents(
        archive, target, expected_root
    )
    reported_version = (version_probe or packaged_version)(executable_content)
    require(
        reported_version == "tracky %s" % package_version,
        "packaged executable version differs from package version",
    )
    manifest = {
        "schema_version": 1,
        "source_sha": source_sha,
        "lockfile_sha256": lockfile_sha256,
        "cargo_dist_manifest_sha256": cargo_dist_manifest_sha256,
        "target": target,
        "package_version": package_version,
        "tools": tools,
        "files": sorted(files, key=lambda item: item["path"]),
        "transport": {
            "archive_name": archive.name,
            "archive_bytes": measurement["archive_bytes"],
            "archive_sha256": measurement["archive_sha256"],
        },
    }
    validate_semantic_archive_manifest(manifest)
    return manifest


def validate_semantic_archive_manifest(value):
    require(
        set(value) == {
            "schema_version", "source_sha", "lockfile_sha256", "target",
            "cargo_dist_manifest_sha256", "package_version", "tools", "files",
            "transport",
        },
        "semantic manifest fields do not match the schema",
    )
    require(value["schema_version"] == 1, "unsupported semantic manifest schema")
    require(value["target"] in TARGETS, "semantic manifest has unknown target")
    validate_semantic_provenance(
        value["source_sha"],
        value["lockfile_sha256"],
        value["cargo_dist_manifest_sha256"],
        value["package_version"],
        value["tools"],
    )
    files = value["files"]
    require(
        [item.get("path") for item in files] == sorted(REQUIRED_ARCHIVE_FILES),
        "semantic manifest files differ from the release allowlist",
    )
    for item in files:
        require(
            set(item) == {"path", "bytes", "sha256", "mode"},
            "semantic manifest file fields are invalid",
        )
        require(isinstance(item["bytes"], int) and item["bytes"] >= 0, "semantic file size is invalid")
        require(re.fullmatch(r"[0-9a-f]{64}", item["sha256"]) is not None, "semantic file hash is invalid")
        expected_mode = "0755" if item["path"] == "tracky" else "0644"
        require(item["mode"] == expected_mode, "semantic file permissions are invalid")
    transport = value["transport"]
    require(
        set(transport) == {"archive_name", "archive_bytes", "archive_sha256"},
        "semantic transport fields are invalid",
    )
    require(
        transport["archive_name"] == "tracky-%s.tar.xz" % value["target"],
        "semantic transport archive name is invalid",
    )
    require(
        isinstance(transport["archive_bytes"], int) and transport["archive_bytes"] > 0,
        "semantic transport archive size is invalid",
    )
    require(
        re.fullmatch(r"[0-9a-f]{64}", transport["archive_sha256"]) is not None,
        "semantic transport archive hash is invalid",
    )


def semantic_archive_identity(value):
    validate_semantic_archive_manifest(value)
    return {key: value[key] for key in sorted(value) if key != "transport"}


def verify_semantic_archive_manifest(
    value,
    archive,
    cargo_dist_manifest,
    expected_root=ROOT,
    version_probe=None,
):
    validate_semantic_archive_manifest(value)
    require(
        hash_file(cargo_dist_manifest) == value["cargo_dist_manifest_sha256"],
        "Cargo Dist manifest checksum differs from the semantic manifest",
    )
    measurement, files, executable_content = inspect_release_archive_contents(
        archive,
        value["target"],
        expected_root,
    )
    require(files == value["files"], "archive files differ from the semantic manifest")
    require(
        archive.name == value["transport"]["archive_name"],
        "archive name differs from the semantic manifest",
    )
    require(
        measurement["archive_bytes"] == value["transport"]["archive_bytes"],
        "archive size differs from the semantic manifest",
    )
    require(
        measurement["archive_sha256"] == value["transport"]["archive_sha256"],
        "archive checksum differs from the semantic manifest",
    )
    require(
        (version_probe or packaged_version)(executable_content)
        == "tracky %s" % value["package_version"],
        "packaged executable version differs from package version",
    )


def verify_dist_checksum(archive):
    checksum = archive.with_name(archive.name + ".sha256")
    require(checksum.is_file(), "Cargo Dist checksum is missing for %s" % archive.name)
    fields = checksum.read_text(encoding="utf-8").strip().split()
    require(
        len(fields) == 2 and fields[1].lstrip("*") == archive.name,
        "Cargo Dist checksum does not name %s" % archive.name,
    )
    require(fields[0] == hash_file(archive), "Cargo Dist checksum mismatch for %s" % archive.name)


def verify_packaged_size_measurement(measured, inspected):
    for field in (
        "archive_bytes", "archive_sha256", "executable_bytes", "executable_sha256",
    ):
        require(
            measured[field] == inspected[field],
            "%s differs from the packaged artifact for %s" % (field, inspected["target"]),
        )


def verify_manifest_artifacts(value, artifacts):
    validate_manifest(value)
    validate_release_measurements(value["measurements"], value["artifacts"])
    recorded_sizes = {
        item["target"]: item for item in value["measurements"]["sizes"]["targets"]
    }
    for recorded in value["artifacts"]:
        archive = artifacts / recorded["name"]
        require(archive.is_file(), "manifest artifact is missing: %s" % recorded["name"])
        require(archive.stat().st_size == recorded["bytes"], "manifest artifact size mismatch: %s" % recorded["name"])
        require(hash_file(archive) == recorded["sha256"], "manifest artifact hash mismatch: %s" % recorded["name"])
        verify_dist_checksum(archive)
        inspected = inspect_release_archive(archive, recorded["target"])
        measured = recorded_sizes[recorded["target"]]
        verify_packaged_size_measurement(measured, inspected)

    sizes = value["measurements"]["sizes"]
    asset_bytes = sum(
        path.stat().st_size
        for path in ASSETS.rglob("*")
        if path.is_file() and path.suffix in {".html", ".css", ".js"}
    )
    require(sizes["asset_bytes"] == asset_bytes, "asset bytes differ from the accepted source")
    require(
        sizes["resolved_package_count"] == len(read_json(INVENTORY)["packages"]) + 1,
        "resolved package count differs from the accepted inventory",
    )


def measure(artifacts, assets):
    packages = read_json(INVENTORY)["packages"]
    targets = []
    for target in sorted(TARGETS):
        archive = artifacts / ("tracky-%s.tar.xz" % target)
        require(archive.is_file(), "missing Cargo Dist archive %s" % archive)
        verify_dist_checksum(archive)
        targets.append(inspect_release_archive(archive, target))
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
    validate.add_argument("--commit")
    validate.add_argument("--lockfile-sha256")
    render = sub.add_parser("render")
    render.add_argument("manifest", type=Path)
    render.add_argument("--output", type=Path, required=True)
    compare = sub.add_parser("compare")
    compare.add_argument("--current", type=Path, required=True)
    measurement = sub.add_parser("measure")
    measurement.add_argument("--artifacts", type=Path, required=True)
    measurement.add_argument("--assets", type=Path, default=ASSETS)
    measurement.add_argument("--output", type=Path, required=True)
    verify_artifacts = sub.add_parser("verify-artifacts")
    verify_artifacts.add_argument("manifest", type=Path)
    verify_artifacts.add_argument("--artifacts", type=Path, required=True)
    semantic = sub.add_parser("semantic-manifest")
    semantic.add_argument("--archive", type=Path, required=True)
    semantic.add_argument("--target", choices=sorted(TARGETS), required=True)
    semantic.add_argument("--source-sha", required=True)
    semantic.add_argument("--lockfile-sha256", required=True)
    semantic.add_argument("--cargo-dist-manifest", type=Path, required=True)
    semantic.add_argument("--package-version", required=True)
    semantic.add_argument("--rust-version", required=True)
    semantic.add_argument("--cargo-version", required=True)
    semantic.add_argument("--cargo-dist-version", required=True)
    semantic.add_argument("--source-root", type=Path, default=ROOT)
    semantic.add_argument("--output", type=Path, required=True)
    validate_semantic = sub.add_parser("validate-semantic")
    validate_semantic.add_argument("manifest", type=Path)
    validate_semantic.add_argument("--archive", type=Path, required=True)
    validate_semantic.add_argument("--cargo-dist-manifest", type=Path, required=True)
    validate_semantic.add_argument("--source-root", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    if args.command == "check":
        check_all()
    elif args.command == "inventory":
        write_or_check(INVENTORY, dependency_inventory(), args.check)
    elif args.command == "validate":
        validate_manifest(
            read_json(args.manifest),
            args.release,
            expected_commit=args.commit,
            expected_lockfile_sha256=args.lockfile_sha256,
        )
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
    elif args.command == "verify-artifacts":
        verify_manifest_artifacts(read_json(args.manifest), args.artifacts)
    elif args.command == "semantic-manifest":
        validate_cargo_dist_manifest(
            read_json(args.cargo_dist_manifest),
            args.target,
            args.package_version,
        )
        manifest = semantic_archive_manifest(
            args.archive,
            args.target,
            args.source_sha,
            args.lockfile_sha256,
            hash_file(args.cargo_dist_manifest),
            args.package_version,
            {
                "rust": args.rust_version,
                "cargo": args.cargo_version,
                "cargo-dist": args.cargo_dist_version,
            },
            expected_root=args.source_root,
        )
        args.output.write_text(canonical_json(manifest), encoding="utf-8")
    elif args.command == "validate-semantic":
        verify_semantic_archive_manifest(
            read_json(args.manifest),
            args.archive,
            args.cargo_dist_manifest,
            expected_root=args.source_root,
        )
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print("dashboard evidence error: %s" % error, file=sys.stderr)
        sys.exit(1)
