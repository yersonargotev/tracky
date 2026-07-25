#!/usr/bin/env python3
"""Run the preserved non-browser quality gates and retain their exact commands."""

import argparse
import json
import re
import shlex
import subprocess
from pathlib import Path


SHA = re.compile(r"[0-9a-f]{40}")
DIGEST = re.compile(r"[0-9a-f]{64}")
SEMVER = re.compile(r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?")
PLACEHOLDER = re.compile(
    r"(?:\b(?:todo|tbd|placeholder)\b|\bnot[- ]?recorded\b"
    r"|^\s*(?:unknown|unassigned)\s*$)",
    re.IGNORECASE,
)
COMMANDS = [
    {
        "gate": "formatting",
        "argv": ["cargo", "fmt", "--all", "--", "--check"],
    },
    {
        "gate": "all-target-tests",
        "argv": ["cargo", "test", "--locked", "--all-targets"],
    },
    {
        "gate": "strict-clippy",
        "argv": [
            "cargo",
            "clippy",
            "--locked",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
    },
    {
        "gate": "evidence-contracts",
        "argv": [
            "python3",
            "-m",
            "unittest",
            "tests/dashboard_evidence_tool.py",
            "tests/dashboard_browser_evidence.py",
            "tests/dashboard_candidate_manifest.py",
            "tests/dashboard_candidate_runtime.py",
            "tests/release_command_evidence.py",
            "tests/release_quality_evidence.py",
            "tests/release_dry_run.py",
        ],
    },
    {
        "gate": "static-and-artifact-budgets",
        "argv": ["python3", "scripts/dashboard_evidence.py", "check"],
    },
    {
        "gate": "static-and-artifact-budgets",
        "argv": [
            "python3",
            "scripts/dashboard_evidence.py",
            "inventory",
            "--check",
        ],
    },
]
REQUIRED_GATES = {
    "formatting",
    "all-target-tests",
    "strict-clippy",
    "dependency-policy",
    "evidence-contracts",
    "static-and-artifact-budgets",
}
TOOL_PROBES = {
    "cargo": ["cargo", "--version"],
    "clippy": ["cargo", "clippy", "--version"],
    "python": ["python3", "--version"],
    "rust": ["rustc", "--version"],
    "rustfmt": ["rustfmt", "--version"],
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def shell_join(command):
    return shlex.join(command)


def concrete(value, label):
    require(isinstance(value, str) and value.strip(), "%s is missing" % label)
    require(PLACEHOLDER.search(value) is None, "%s contains a placeholder" % label)
    return value.strip()


def validate_identity(value):
    require(
        isinstance(value, dict)
        and set(value)
        == {
            "schema_version",
            "source_sha",
            "package_version",
            "lockfile_sha256",
        }
        and value["schema_version"] == 1
        and SHA.fullmatch(str(value["source_sha"])) is not None
        and SEMVER.fullmatch(str(value["package_version"])) is not None
        and DIGEST.fullmatch(str(value["lockfile_sha256"])) is not None,
        "release identity is invalid",
    )
    return value


def validate_quality_evidence(value, identity):
    validate_identity(identity)
    require(
        isinstance(value, dict)
        and set(value)
        == {"schema_version", "identity", "tools", "commands", "gates"}
        and value["schema_version"] == 1,
        "quality evidence fields are invalid",
    )
    require(value["identity"] == identity, "quality evidence identity differs")
    expected_commands = [shell_join(item["argv"]) for item in COMMANDS]
    require(
        value["commands"] == expected_commands,
        "quality evidence command catalog differs",
    )
    require(
        isinstance(value["tools"], dict)
        and set(value["tools"]) == set(TOOL_PROBES),
        "quality evidence tool versions are incomplete",
    )
    for name, version in value["tools"].items():
        concrete(name, "quality tool name")
        concrete(version, "%s tool version" % name)
    gates = value["gates"]
    require(
        isinstance(gates, list)
        and len(gates) == len(REQUIRED_GATES)
        and {item.get("gate") for item in gates} == REQUIRED_GATES,
        "quality evidence gate matrix is incomplete or duplicated",
    )
    for item in gates:
        require(
            set(item) == {"gate", "status", "source"}
            and item["status"] == "pass"
            and item["source"]
            in {"executed-command", "successful-prerequisite-job-step"},
            "quality evidence contains an unsuccessful gate",
        )
    return value


def default_execute(command):
    subprocess.run(command, check=True)


def default_version_probe(command):
    return subprocess.check_output(command, text=True, stderr=subprocess.STDOUT).strip()


def run_quality_gates(identity, execute=default_execute, version_probe=default_version_probe):
    validate_identity(identity)
    for item in COMMANDS:
        execute(item["argv"])
    tools = {
        name: concrete(version_probe(command), "%s tool version" % name)
        for name, command in sorted(TOOL_PROBES.items())
    }
    gates = [
        {
            "gate": name,
            "status": "pass",
            "source": (
                "successful-prerequisite-job-step"
                if name == "dependency-policy"
                else "executed-command"
            ),
        }
        for name in sorted(REQUIRED_GATES)
    ]
    return validate_quality_evidence({
        "schema_version": 1,
        "identity": identity,
        "tools": tools,
        "commands": [shell_join(item["argv"]) for item in COMMANDS],
        "gates": gates,
    }, identity)


def main(argv=None, execute=default_execute, version_probe=default_version_probe):
    parser = argparse.ArgumentParser()
    parser.add_argument("--identity", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    identity = json.loads(args.identity.read_text(encoding="utf-8"))
    value = run_quality_gates(
        identity,
        execute=execute,
        version_probe=version_probe,
    )
    args.output.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
