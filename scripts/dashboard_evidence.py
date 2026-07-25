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
            "%s differs from the packaged artifact for %s"
            % (field, inspected["target"]),
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


def check_all():
    baseline = read_json(BASELINE)
    validate_baseline(baseline)
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
    compare = sub.add_parser("compare")
    compare.add_argument("--current", type=Path, required=True)
    measurement = sub.add_parser("measure")
    measurement.add_argument("--artifacts", type=Path, required=True)
    measurement.add_argument("--assets", type=Path, default=ASSETS)
    measurement.add_argument("--output", type=Path, required=True)
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
    elif args.command == "compare":
        baseline = read_json(BASELINE)
        validate_baseline(baseline)
        compare_measurements(read_json(args.current), baseline)
    elif args.command == "measure":
        args.output.write_text(canonical_json(measure(args.artifacts, args.assets)), encoding="utf-8")
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
