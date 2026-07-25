#!/usr/bin/env python3
"""Decide whether one trusted event should run or publish the release DAG."""

import argparse
import re
import subprocess
from pathlib import Path


SHA = re.compile(r"[0-9a-f]{40}")
VERSION_LINE = re.compile(r'version\s*=\s*"([^"]+)"(?:\s*#.*)?')


def package_version_text(manifest):
    in_package = False
    for raw_line in manifest.splitlines():
        line = raw_line.strip()
        if line.startswith("[") and line.endswith("]"):
            in_package = line == "[package]"
            continue
        if in_package:
            match = VERSION_LINE.fullmatch(line)
            if match:
                return match.group(1)
    raise ValueError("Cargo.toml [package] version is missing")


def version_at(root, revision):
    result = subprocess.run(
        ["git", "-C", str(root), "show", "%s:Cargo.toml" % revision],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode:
        raise ValueError("before SHA does not contain Cargo.toml")
    return package_version_text(result.stdout)


def release_trigger(root, event_name, before_sha=None):
    root = Path(root)
    if event_name == "pull_request":
        return {"run_dag": True, "publish": False, "reason": "pull-request"}
    if event_name == "workflow_dispatch":
        return {"run_dag": True, "publish": True, "reason": "manual-recovery"}
    if event_name != "push":
        raise ValueError("unsupported release event: %s" % event_name)
    if SHA.fullmatch(before_sha or "") is None:
        raise ValueError("push before SHA must be a full lowercase SHA")
    previous = version_at(root, before_sha)
    current = package_version_text((root / "Cargo.toml").read_text(encoding="utf-8"))
    if previous == current:
        return {"run_dag": False, "publish": False, "reason": "version-unchanged"}
    return {"run_dag": True, "publish": True, "reason": "version-changed"}


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path, default=Path.cwd())
    parser.add_argument("--event-name", required=True)
    parser.add_argument("--before-sha")
    parser.add_argument("--github-output", type=Path, required=True)
    args = parser.parse_args(argv)
    value = release_trigger(args.source_root, args.event_name, args.before_sha)
    with args.github_output.open("a", encoding="utf-8") as output:
        for key in ("run_dag", "publish", "reason"):
            item = value[key]
            output.write("%s=%s\n" % (key, str(item).lower() if isinstance(item, bool) else item))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
