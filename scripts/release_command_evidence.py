#!/usr/bin/env python3
"""Bind commands that already succeeded to one exact native release job."""

import argparse
import json
import sys
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parent))
import dashboard_evidence as evidence_contract  # noqa: E402
import release_quality_evidence as quality_contract  # noqa: E402


JOB_TYPES = {"native-build", "native-runtime"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def assemble(identity, job_type, target, commands):
    quality_contract.validate_identity(identity)
    require(job_type in JOB_TYPES, "native command evidence job type is invalid")
    require(
        target in evidence_contract.TARGETS,
        "native command evidence target is invalid",
    )
    require(
        isinstance(commands, list) and commands,
        "native command evidence commands are missing",
    )
    normalized = [
        quality_contract.concrete(command, "native command") for command in commands
    ]
    require(
        len(normalized) == len(set(normalized)),
        "native command evidence contains a duplicated command",
    )
    return {
        "schema_version": 1,
        "identity": identity,
        "job_type": job_type,
        "target": target,
        "status": "pass",
        "commands": normalized,
    }


def validate(value, identity, job_type, target):
    require(
        isinstance(value, dict)
        and set(value)
        == {
            "schema_version",
            "identity",
            "job_type",
            "target",
            "status",
            "commands",
        }
        and value["schema_version"] == 1,
        "native command evidence fields are invalid",
    )
    expected = assemble(
        value["identity"],
        value["job_type"],
        value["target"],
        value["commands"],
    )
    require(value == expected, "native command evidence contains a failed status")
    require(value["identity"] == identity, "native command evidence identity differs")
    require(value["job_type"] == job_type, "native command evidence job type differs")
    require(value["target"] == target, "native command evidence target differs")
    return value


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--identity", type=Path, required=True)
    parser.add_argument("--job-type", choices=sorted(JOB_TYPES), required=True)
    parser.add_argument(
        "--target",
        choices=sorted(evidence_contract.TARGETS),
        required=True,
    )
    parser.add_argument("--commands-file", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    identity = json.loads(args.identity.read_text(encoding="utf-8"))
    commands = args.commands_file.read_text(encoding="utf-8").splitlines()
    value = assemble(identity, args.job_type, args.target, commands)
    args.output.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
