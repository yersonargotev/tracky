#!/usr/bin/env python3
"""Assess unified production-release latency after enough completed releases."""

import argparse
import json
import math
import re
import statistics
from datetime import datetime
from pathlib import Path


def timestamp(value):
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def run_source(item):
    if item.get("event") == "push":
        return item.get("head_sha")
    if item.get("event") == "workflow_dispatch":
        match = re.fullmatch(
            r"Release Tracky ([0-9a-f]{40})",
            item.get("display_title", ""),
        )
        return match.group(1) if match else None
    return None


def release_slo(history, releases, current_run, current_jobs, current_source):
    stable_sources = {
        item["target_commitish"]
        for item in releases
        if item.get("draft") is False and item.get("prerelease") is False
    }
    durations_by_source = {}
    for item in history:
        source = run_source(item)
        if item.get("name") != "Release Tracky" or source not in stable_sources:
            continue
        durations_by_source.setdefault(
            source,
            (timestamp(item["updated_at"]) - timestamp(item["created_at"])).total_seconds(),
        )
    completed = [
        timestamp(job["completed_at"])
        for job in current_jobs
        if job.get("completed_at")
    ]
    if not completed:
        raise ValueError("current release has no completed jobs")
    durations_by_source[current_source] = (
        max(completed) - timestamp(current_run["created_at"])
    ).total_seconds()
    durations = sorted(durations_by_source.values())
    value = {
        "schema_version": 1,
        "sample_count": len(durations),
        "assessed": len(durations) >= 10,
        "target_seconds": {"minimum": 600, "maximum": 900},
    }
    if value["assessed"]:
        value.update({
            "p50_seconds": round(statistics.median(durations), 3),
            "p95_seconds": round(durations[math.ceil(0.95 * len(durations)) - 1], 3),
        })
        value["status"] = "met" if value["p95_seconds"] <= 900 else "not-met"
    return value


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--workflow-history", type=Path, required=True)
    parser.add_argument("--releases", type=Path, required=True)
    parser.add_argument("--current-run", type=Path, required=True)
    parser.add_argument("--current-jobs", type=Path, required=True)
    parser.add_argument("--current-source", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    history = json.loads(args.workflow_history.read_text(encoding="utf-8"))
    jobs = json.loads(args.current_jobs.read_text(encoding="utf-8"))
    value = release_slo(
        history["workflow_runs"],
        json.loads(args.releases.read_text(encoding="utf-8")),
        json.loads(args.current_run.read_text(encoding="utf-8")),
        jobs["jobs"],
        args.current_source,
    )
    args.output.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
