#!/usr/bin/env python3
"""Assemble complete same-run evidence from a successful release."""

import argparse
import json
import re
import sys
from datetime import datetime
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parent))
import dashboard_browser_evidence as browser_contract  # noqa: E402
import dashboard_evidence as evidence  # noqa: E402
import release_command_evidence as command_contract  # noqa: E402
import release_quality_evidence as quality_contract  # noqa: E402


REPOSITORY = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+")
SHA = re.compile(r"[0-9a-f]{40}")
DIGEST = re.compile(r"sha256:[0-9a-f]{64}")


def concrete(value, label):
    evidence.require(isinstance(value, str) and value.strip(), "%s is missing" % label)
    evidence.require(
        quality_contract.PLACEHOLDER.search(value) is None,
        "%s contains a placeholder" % label,
    )
    return value


def timestamp(value, label):
    concrete(value, label)
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise ValueError("%s is not an ISO-8601 timestamp" % label) from error


def elapsed_seconds(started_at, completed_at):
    started = timestamp(started_at, "job started_at")
    completed = timestamp(completed_at, "job completed_at")
    seconds = (completed - started).total_seconds()
    evidence.require(seconds >= 0, "job timing is negative")
    return round(seconds, 3)


def required_job_names():
    return {
        "Bind the release identity and target matrix",
        "Preserved release quality gates",
        "Bind current browser evidence",
        *{"Build once (%s)" % target for target in evidence.TARGETS},
        *{"Reuse and test (%s)" % target for target in evidence.TARGETS},
        *{
            "Current browser (%s)" % lane
            for lane in browser_contract.CURRENT_LANES
        },
    }


def validate_workflow_run(value, identity, repository):
    evidence.require(
        isinstance(value, dict)
        and isinstance(value.get("id"), int)
        and value["id"] > 0,
        "workflow run ID is invalid",
    )
    evidence.require(
        isinstance(value.get("run_attempt"), int) and value["run_attempt"] > 0,
        "workflow run attempt is invalid",
    )
    evidence.require(
        SHA.fullmatch(str(value.get("head_sha", ""))) is not None,
        "workflow run head SHA is invalid",
    )
    run_url = concrete(value.get("html_url"), "workflow run URL")
    expected_url = "https://github.com/%s/actions/runs/%s" % (
        repository,
        value["id"],
    )
    evidence.require(run_url == expected_url, "workflow run URL is invalid")
    timestamp(value.get("run_started_at"), "workflow run started_at")
    evidence.require(
        value.get("status") in {"queued", "in_progress", "completed"},
        "workflow run status is invalid",
    )
    evidence.require(
        value.get("conclusion") in {None, "success"},
        "workflow run conclusion is not successful",
    )
    if value["status"] == "completed":
        evidence.require(
            value.get("conclusion") == "success",
            "completed workflow run is not successful",
        )
    return value


def validate_jobs(payload, workflow_run, identity):
    jobs = payload.get("jobs") if isinstance(payload, dict) else None
    evidence.require(isinstance(jobs, list), "workflow jobs payload is invalid")
    required = required_job_names()
    matching = [job for job in jobs if job.get("name") in required]
    names = [job.get("name") for job in matching]
    evidence.require(len(names) == len(set(names)), "workflow job is duplicated")
    evidence.require(
        len(matching) == len(required) and set(names) == required,
        "required job matrix is missing or incomplete",
    )

    result = []
    run_url = workflow_run["html_url"]
    for job in sorted(matching, key=lambda item: item["name"]):
        evidence.require(
            job.get("run_id") == workflow_run["id"],
            "workflow job belongs to another run",
        )
        evidence.require(
            job.get("head_sha") == workflow_run["head_sha"],
            "workflow job belongs to another workflow commit",
        )
        evidence.require(
            job.get("status") == "completed" and job.get("conclusion") == "success",
            "every evidence-producing workflow job must be successful",
        )
        evidence.require(
            isinstance(job.get("id"), int) and job["id"] > 0,
            "workflow job ID is invalid",
        )
        url = concrete(job.get("html_url"), "workflow job URL")
        evidence.require(
            url == "%s/job/%s" % (run_url, job["id"]),
            "workflow job URL is invalid",
        )
        if job["name"] == "Preserved release quality gates":
            steps = job.get("steps")
            dependency_steps = [
                step
                for step in steps
                if step.get("name") == "Enforce dependency and license policy"
            ] if isinstance(steps, list) else []
            evidence.require(
                len(dependency_steps) == 1
                and dependency_steps[0].get("status") == "completed"
                and dependency_steps[0].get("conclusion") == "success",
                "dependency policy step is missing or unsuccessful",
            )
        result.append(
            {
                "name": job["name"],
                "id": job["id"],
                "url": url,
                "status": "success",
                "started_at": job["started_at"],
                "completed_at": job["completed_at"],
                "duration_seconds": elapsed_seconds(
                    job["started_at"], job["completed_at"]
                ),
            }
        )
    return result


def required_artifact_names(source_sha):
    return {
        "release-identity-%s" % source_sha,
        "release-quality-evidence-%s" % source_sha,
        "release-current-browser-evidence-%s" % source_sha,
        *{
            "release-built-%s-%s" % (source_sha, target)
            for target in evidence.TARGETS
        },
        *{
            "release-verified-%s-%s" % (source_sha, target)
            for target in evidence.TARGETS
        },
        *{
            "release-current-browser-%s-%s" % (source_sha, lane)
            for lane in browser_contract.CURRENT_LANES
        },
    }


def validate_retained_artifacts(payload, workflow_run, identity):
    artifacts = payload.get("artifacts") if isinstance(payload, dict) else None
    evidence.require(
        isinstance(artifacts, list),
        "workflow artifacts payload is invalid",
    )
    required = required_artifact_names(identity["source_sha"])
    matching = [item for item in artifacts if item.get("name") in required]
    names = [item.get("name") for item in matching]
    evidence.require(len(names) == len(set(names)), "retained artifact is duplicated")
    evidence.require(
        len(matching) == len(required) and set(names) == required,
        "required retained artifact matrix is missing or incomplete",
    )

    result = []
    for item in sorted(matching, key=lambda value: value["name"]):
        bound_run = item.get("workflow_run")
        evidence.require(
            isinstance(bound_run, dict)
            and bound_run.get("id") == workflow_run["id"],
            "retained artifact belongs to another workflow run",
        )
        evidence.require(
            bound_run.get("head_sha") == workflow_run["head_sha"],
            "retained artifact belongs to another workflow commit",
        )
        evidence.require(item.get("expired") is False, "retained artifact is expired")
        evidence.require(
            isinstance(item.get("id"), int) and item["id"] > 0,
            "retained artifact ID is invalid",
        )
        evidence.require(
            isinstance(item.get("size_in_bytes"), int)
            and item["size_in_bytes"] > 0,
            "retained artifact size is invalid",
        )
        evidence.require(
            DIGEST.fullmatch(str(item.get("digest", ""))) is not None,
            "retained artifact digest is invalid or a placeholder",
        )
        result.append(
            {
                "name": item["name"],
                "id": item["id"],
                "digest": item["digest"],
                "size_in_bytes": item["size_in_bytes"],
                "url": "%s/artifacts/%s" % (workflow_run["html_url"], item["id"]),
            }
        )
    return result


def job_by_name(jobs, name):
    return next(job for job in jobs if job["name"] == name)


def retained_by_name(retained, name):
    return next(item for item in retained if item["name"] == name)


def validate_runtime_measurements(runtime_by_target):
    latency = {
        target: runtime_by_target[target]["latency"] for target in evidence.TARGETS
    }
    resources = {
        target: runtime_by_target[target]["resources"] for target in evidence.TARGETS
    }
    # Size budgets remain a separate quality gate; validate the two independently
    # retained runtime measurement groups here.
    evidence.require(set(latency) == evidence.TARGETS, "runtime latency targets differ")
    evidence.require(
        set(resources) == evidence.TARGETS, "runtime resource targets differ"
    )
    for target in evidence.TARGETS:
        target_latency = latency[target]
        evidence.require(
            set(target_latency)
            == set(evidence.LATENCY_LIMITS_MS) | {"warmups", "runs"},
            "latency metrics are incomplete for %s" % target,
        )
        evidence.require(
            target_latency["warmups"] >= 5 and target_latency["runs"] >= 30,
            "latency sample count is incomplete for %s" % target,
        )
        for name, limit in evidence.LATENCY_LIMITS_MS.items():
            evidence.require(
                evidence.numeric(target_latency[name])
                and 0 <= target_latency[name] <= limit,
                "%s exceeds its release budget for %s" % (name, target),
            )
        target_resources = resources[target]
        expected = set(evidence.RESOURCE_LIMITS) | {
            "cycles",
            "descriptor_growth",
            "memory_growth_bytes",
            "memory_growth_percent",
        }
        evidence.require(
            set(target_resources) == expected,
            "resource metrics are incomplete for %s" % target,
        )
        for name, limit in evidence.RESOURCE_LIMITS.items():
            evidence.require(
                evidence.numeric(target_resources[name])
                and 0 <= target_resources[name] <= limit,
                "%s exceeds its release budget for %s" % (name, target),
            )
        evidence.require(
            target_resources["cycles"] >= 100,
            "resource cycle count is incomplete for %s" % target,
        )
        evidence.require(
            target_resources["descriptor_growth"] <= 0,
            "descriptor growth detected for %s" % target,
        )
        evidence.require(
            target_resources["memory_growth_bytes"] <= 8 * 1024 * 1024
            or target_resources["memory_growth_percent"] <= 5,
            "memory growth exceeds its release budget for %s" % target,
        )
    return {"latency": latency, "resources": resources}


def gate(name, jobs, commands):
    return {
        "gate": name,
        "status": "pass",
        "evidence": [job["url"] for job in jobs],
        "commands": commands,
        "duration_seconds": round(sum(job["duration_seconds"] for job in jobs), 3),
    }


def assemble(
    verified_root,
    identity,
    quality_evidence_path,
    browser_evidence_path,
    browser_results,
    workflow_run,
    workflow_jobs,
    workflow_artifacts,
    repository,
):
    evidence.require(
        set(identity)
        == {
            "schema_version",
            "source_sha",
            "package_version",
            "lockfile_sha256",
        }
        and identity["schema_version"] == 1,
        "release identity fields are invalid",
    )
    evidence.require(
        REPOSITORY.fullmatch(repository) is not None,
        "repository name is invalid",
    )
    validate_workflow_run(workflow_run, identity, repository)
    quality = quality_contract.validate_quality_evidence(
        evidence.read_json(quality_evidence_path),
        identity,
    )
    jobs = validate_jobs(
        workflow_jobs,
        workflow_run,
        identity,
    )
    retained = validate_retained_artifacts(
        workflow_artifacts,
        workflow_run,
        identity,
    )

    prefix = "release-verified-%s-" % identity["source_sha"]
    directories = sorted(
        path
        for path in Path(verified_root).iterdir()
        if path.name.startswith(prefix)
    )
    evidence.require(
        len(directories) == len(evidence.TARGETS),
        "dry run must contain exactly one bundle for every supported target",
    )

    artifacts = []
    runtime_by_target = {}
    found_targets = set()
    tools = {}
    commands = list(quality["commands"])
    build_commands = []
    native_command_evidence = {}
    for directory in directories:
        target = directory.name[len(prefix) :]
        evidence.require(target in evidence.TARGETS, "dry run bundle has unknown target")
        evidence.require(
            target not in found_targets, "dry run bundle target is duplicated"
        )
        found_targets.add(target)
        candidate = directory / "candidate"
        recorded_identity = evidence.read_json(candidate / "release-identity.json")
        semantic = evidence.read_json(candidate / "semantic-manifest.json")
        dist_manifest = candidate / "dist-manifest.json"
        dist_manifest_value = evidence.read_json(dist_manifest)
        archive = candidate / ("tracky-%s.tar.xz" % target)
        runtime = evidence.read_json(directory / "runtime-evidence.json")
        build_command_evidence = command_contract.validate(
            evidence.read_json(candidate / "native-build-evidence.json"),
            identity,
            "native-build",
            target,
        )
        runtime_command_evidence = command_contract.validate(
            evidence.read_json(directory / "native-runtime-evidence.json"),
            identity,
            "native-runtime",
            target,
        )

        evidence.require(recorded_identity == identity, "bundle release identity differs")
        evidence.validate_semantic_archive_manifest(semantic)
        evidence.require(
            semantic["source_sha"] == identity["source_sha"],
            "semantic source SHA differs",
        )
        evidence.require(
            semantic["lockfile_sha256"] == identity["lockfile_sha256"],
            "semantic lockfile differs",
        )
        evidence.require(
            semantic["package_version"] == identity["package_version"],
            "semantic package version differs",
        )
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
        for name, version in semantic["tools"].items():
            concrete(name, "tool name")
            concrete(version, "tool version")
        validate_runtime_fields(runtime, target)
        runtime_by_target[target] = runtime
        commands.extend(runtime["commands"])
        build_commands.extend(build_command_evidence["commands"])
        commands.extend(build_command_evidence["commands"])
        commands.extend(runtime_command_evidence["commands"])
        native_command_evidence[target] = {
            "build": build_command_evidence,
            "runtime": runtime_command_evidence,
        }
        tools[target] = semantic["tools"]
        built_name = "release-built-%s-%s" % (
            identity["source_sha"],
            target,
        )
        verified_name = "release-verified-%s-%s" % (
            identity["source_sha"],
            target,
        )
        artifacts.append(
            {
                "target": target,
                "archive_name": archive.name,
                "archive_bytes": archive.stat().st_size,
                "archive_sha256": archive_sha256,
                "cargo_dist_manifest_sha256": semantic[
                    "cargo_dist_manifest_sha256"
                ],
                "semantic_manifest": semantic,
                "command_evidence": native_command_evidence[target],
                "built_artifact": retained_by_name(retained, built_name),
                "verified_artifact": retained_by_name(retained, verified_name),
            }
        )

    evidence.require(found_targets == evidence.TARGETS, "dry run targets are incomplete")
    measurements = validate_runtime_measurements(runtime_by_target)
    browser_evidence = evidence.read_json(browser_evidence_path)
    browser_contract.validate_canonical_browser_evidence(
        browser_evidence,
        identity["source_sha"],
        identity["lockfile_sha256"],
        "current",
    )
    raw_browser_evidence = browser_contract.assemble(
        browser_results,
        identity["source_sha"],
        identity["lockfile_sha256"],
        profile="current",
    )
    evidence.require(
        raw_browser_evidence == browser_evidence,
        "canonical browser evidence differs from retained raw results",
    )
    browser_results_by_lane = {}
    for path in sorted(Path(browser_results).glob("*.json")):
        value = evidence.read_json(path)
        lane = value["lane"]
        concrete(value["driver"]["name"], "%s driver name" % lane)
        concrete(value["driver"]["version"], "%s driver version" % lane)
        concrete(value["command"], "%s browser command" % lane)
        browser_results_by_lane[lane] = value
        commands.append(value["command"])

    quality_job = job_by_name(jobs, "Preserved release quality gates")
    build_jobs = [
        job_by_name(jobs, "Build once (%s)" % target)
        for target in sorted(evidence.TARGETS)
    ]
    runtime_jobs = [
        job_by_name(jobs, "Reuse and test (%s)" % target)
        for target in sorted(evidence.TARGETS)
    ]
    browser_jobs = [
        job_by_name(jobs, "Current browser (%s)" % lane)
        for lane in sorted(browser_contract.CURRENT_LANES)
    ]
    browser_commands = [
        browser_results_by_lane[lane]["command"]
        for lane in sorted(browser_contract.CURRENT_LANES)
    ]
    runtime_commands = [
        command
        for target in sorted(evidence.TARGETS)
        for command in native_command_evidence[target]["runtime"]["commands"]
    ]
    quality_commands_by_gate = {}
    for item in quality_contract.COMMANDS:
        quality_commands_by_gate.setdefault(item["gate"], []).append(
            quality_contract.shell_join(item["argv"])
        )
    gates = [
        gate("dependency-policy", [quality_job], []),
        gate(
            "static-and-artifact-budgets",
            [quality_job],
            quality_commands_by_gate["static-and-artifact-budgets"],
        ),
        gate("archive-and-installers", build_jobs, build_commands),
        gate(
            "semantic-conformance",
            runtime_jobs,
            runtime_commands,
        ),
        gate(
            "database-immutability",
            runtime_jobs + browser_jobs,
            [*runtime_commands, *browser_commands],
        ),
        gate("packaged-security", runtime_jobs, runtime_commands),
        gate("performance-and-resources", runtime_jobs, runtime_commands),
        gate("browser-flows", browser_jobs, browser_commands),
        gate("http-security", browser_jobs, browser_commands),
        gate("process-lifecycle", browser_jobs, browser_commands),
        gate("accessibility-automation", browser_jobs, browser_commands),
    ]
    evidence.require(
        {item["gate"] for item in gates} == evidence.REQUIRED_RELEASE_GATES,
        "automated release gate matrix is incomplete",
    )
    commands = sorted(set(commands))
    for command in commands:
        concrete(command, "release command")

    return {
        "schema_version": 2,
        "source_sha": identity["source_sha"],
        "package_version": identity["package_version"],
        "lockfile_sha256": identity["lockfile_sha256"],
        "mode": "release",
        "published": False,
        "workflow": {
            "repository": repository,
            "run_id": workflow_run["id"],
            "run_attempt": workflow_run["run_attempt"],
            "run_url": workflow_run["html_url"],
            "workflow_sha": workflow_run["head_sha"],
            "started_at": workflow_run["run_started_at"],
        },
        "jobs": jobs,
        "retained_artifacts": retained,
        "tools": {
            "quality": quality["tools"],
            "native": tools,
        },
        "quality_evidence": quality,
        "browsers": dict(sorted(browser_evidence["browsers"].items())),
        "browser_evidence": {
            lane: browser_results_by_lane[lane]
            for lane in sorted(browser_results_by_lane)
        },
        "artifacts": sorted(artifacts, key=lambda item: item["target"]),
        "measurements": measurements,
        "runtime_evidence": {
            target: runtime_by_target[target] for target in sorted(runtime_by_target)
        },
        "native_command_evidence": {
            target: native_command_evidence[target]
            for target in sorted(native_command_evidence)
        },
        "gates": sorted(gates, key=lambda item: item["gate"]),
        "commands": commands,
    }


def validate_runtime_fields(value, target):
    evidence.require(
        isinstance(value, dict)
        and set(value) == {"target", "latency", "resources", "commands"},
        "runtime evidence fields are invalid for %s" % target,
    )
    evidence.require(value["target"] == target, "runtime evidence target differs")
    evidence.require(
        isinstance(value["commands"], list) and value["commands"],
        "runtime commands are invalid",
    )
    for command in value["commands"]:
        concrete(command, "runtime command")


def render_markdown(value):
    lines = [
        "# Tracky automated release evidence",
        "",
        "- Source SHA: `%s`" % value["source_sha"],
        "- Package version: `%s`" % value["package_version"],
        "- Cargo.lock SHA-256: `%s`" % value["lockfile_sha256"],
        "- Workflow run: [%s](%s)"
        % (value["workflow"]["run_id"], value["workflow"]["run_url"]),
        "- Published: `%s`" % str(value["published"]).lower(),
        "",
        "## Artifacts",
        "",
    ]
    for artifact in value["artifacts"]:
        retained = artifact["verified_artifact"]
        lines.append(
            "- `%s`: `%s` (%s bytes, `%s`); retained artifact "
            "[%s](%s), digest `%s`"
            % (
                artifact["target"],
                artifact["archive_name"],
                artifact["archive_bytes"],
                artifact["archive_sha256"],
                retained["id"],
                retained["url"],
                retained["digest"],
            )
        )
    lines.extend(["", "## Tools", ""])
    for name, version in sorted(value["tools"]["quality"].items()):
        lines.append("- Quality `%s`: `%s`" % (name, version))
    for target, versions in sorted(value["tools"]["native"].items()):
        for name, version in sorted(versions.items()):
            lines.append("- `%s` / `%s`: `%s`" % (target, name, version))
    lines.extend(["", "## Browsers", ""])
    for lane, version in sorted(value["browsers"].items()):
        lines.append("- `%s`: `%s`" % (lane, version))
    lines.extend(["", "## Browser gate outcomes", ""])
    for lane, result in sorted(value["browser_evidence"].items()):
        lines.append(
            "- `%s`: %s `%s`; driver `%s` `%s`; gates `%s`"
            % (
                lane,
                result["browser"]["name"],
                result["browser"]["version"],
                result["driver"]["name"],
                result["driver"]["version"],
                ", ".join(
                    "%s=%s" % (gate_result["gate"], gate_result["status"])
                    for gate_result in result["gates"]
                ),
            )
        )
    lines.extend(["", "## Preserved quality gates", ""])
    for item in value["quality_evidence"]["gates"]:
        lines.append(
            "- **%s** — `%s` (`%s`)"
            % (item["gate"], item["status"], item["source"])
        )
    lines.extend(["", "## Gates", ""])
    for item in value["gates"]:
        lines.append(
            "- **%s** — `%s` (%ss)"
            % (item["gate"], item["status"], item["duration_seconds"])
        )
    lines.extend(["", "## Jobs", ""])
    for job in value["jobs"]:
        lines.append(
            "- [%s](%s) — `%s` (%ss)"
            % (job["name"], job["url"], job["status"], job["duration_seconds"])
        )
    lines.extend(
        [
            "",
            "## Runtime measurements",
            "",
            "```json",
            json.dumps(value["measurements"], indent=2, sort_keys=True),
            "```",
        ]
    )
    lines.extend(["", "## Commands", ""])
    lines.extend("- `%s`" % command for command in value["commands"])
    return "\n".join(lines) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--verified-root", type=Path, required=True)
    parser.add_argument("--identity", type=Path, required=True)
    parser.add_argument("--quality-evidence", type=Path, required=True)
    parser.add_argument("--browser-evidence", type=Path, required=True)
    parser.add_argument("--browser-results", type=Path, required=True)
    parser.add_argument("--workflow-run", type=Path, required=True)
    parser.add_argument("--workflow-jobs", type=Path, required=True)
    parser.add_argument("--workflow-artifacts", type=Path, required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--markdown-output", type=Path, required=True)
    args = parser.parse_args(argv)
    value = assemble(
        args.verified_root,
        evidence.read_json(args.identity),
        args.quality_evidence,
        args.browser_evidence,
        args.browser_results,
        evidence.read_json(args.workflow_run),
        evidence.read_json(args.workflow_jobs),
        evidence.read_json(args.workflow_artifacts),
        args.repository,
    )
    args.output.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    args.markdown_output.write_text(render_markdown(value), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
