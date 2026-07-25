#!/usr/bin/env python3
"""Assemble exact-SHA release-browser lane results into canonical evidence."""

import argparse
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dashboard_release_browser import GATES  # noqa: E402

LANES = {
    "safari-minimum": ("safari", (26, 0)),
    "safari-latest": ("safari", (26, 0)),
    "firefox-esr-minimum": ("firefox", (153,)),
    "firefox-latest": ("firefox", (153,)),
    "chromium-minimum": ("chromium", (150,)),
    "chromium-latest": ("chromium", (150,)),
}
CURRENT_LANES = {
    "safari-latest",
    "firefox-latest",
    "chromium-latest",
}
SHA = re.compile(r"[0-9a-f]{40}")
LOCKFILE_SHA256 = re.compile(r"[0-9a-f]{64}")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def canonical_json(value):
    return json.dumps(value, indent=2, sort_keys=True) + "\n"


def version_tuple(value):
    match = re.search(r"\d+(?:\.\d+)*", value)
    require(match is not None, "browser version must contain a numeric version")
    return tuple(int(part) for part in match.group(0).split("."))


def profile_lanes(profile):
    require(profile in {"full", "current"}, "unknown browser evidence profile")
    return set(LANES) if profile == "full" else CURRENT_LANES


def fail_closed_result(lane, commit, lockfile_sha256, browser):
    require(lane in LANES, "unknown browser lane")
    require(LANES[lane][0] == browser, "browser does not match lane")
    require(SHA.fullmatch(commit) is not None, "commit must be a full lowercase SHA")
    require(
        LOCKFILE_SHA256.fullmatch(lockfile_sha256) is not None,
        "lockfile_sha256 must be lowercase SHA-256",
    )
    return {
        "schema_version": 1,
        "lane": lane,
        "commit": commit,
        "lockfile_sha256": lockfile_sha256,
        "browser": {"name": browser, "version": "not-recorded"},
        "driver": {"name": "not-recorded", "version": "not-recorded"},
        "command": "workflow failed before the browser harness command",
        "gates": [
            {"gate": gate, "status": "fail" if index == 0 else "not-run"}
            for index, gate in enumerate(GATES)
        ],
    }


def validate_canonical_browser_evidence(value, commit, lockfile_sha256, profile):
    required_lanes = profile_lanes(profile)
    require(
        isinstance(value, dict)
        and set(value) == {"commit", "lockfile_sha256", "browsers", "commands"},
        "canonical browser evidence fields are invalid",
    )
    require(value["commit"] == commit, "browser evidence source SHA differs")
    require(
        value["lockfile_sha256"] == lockfile_sha256,
        "browser evidence lockfile differs",
    )
    browsers = value["browsers"]
    require(
        isinstance(browsers, dict) and set(browsers) == required_lanes,
        "browser evidence lanes differ from the %s profile" % profile,
    )
    for lane, version in browsers.items():
        require(
            isinstance(version, str)
            and version
            and version_tuple(version) >= LANES[lane][1],
            "%s is below its supported minimum" % lane,
        )
    require(
        isinstance(value["commands"], list)
        and len(value["commands"]) == len(required_lanes)
        and all(
            isinstance(command, str) and command for command in value["commands"]
        ),
        "canonical browser evidence commands are invalid",
    )
    return value


def assemble(results_dir, commit, lockfile_sha256, profile="full"):
    required_lanes = profile_lanes(profile)
    require(SHA.fullmatch(commit) is not None, "commit must be a full lowercase SHA")
    require(
        LOCKFILE_SHA256.fullmatch(lockfile_sha256) is not None,
        "lockfile_sha256 must be lowercase SHA-256",
    )
    results = []
    for path in sorted(Path(results_dir).glob("*.json")):
        result = json.loads(path.read_text(encoding="utf-8"))
        require(isinstance(result, dict), "lane result must be an object: %s" % path.name)
        results.append(result)
    expected = "six" if profile == "full" else "three current"
    require(
        len(results) == len(required_lanes),
        "results directory must contain exactly %s lane results" % expected,
    )
    by_lane = {result.get("lane"): result for result in results}
    require(
        set(by_lane) == required_lanes,
        "lane results must cover the exact %s-lane matrix" % expected,
    )

    browsers = {}
    commands = []
    for lane in sorted(required_lanes):
        result = by_lane[lane]
        require(result.get("schema_version") == 1, "%s has an unsupported schema" % lane)
        require(result.get("commit") == commit, "%s is not bound to the accepted commit" % lane)
        require(
            result.get("lockfile_sha256") == lockfile_sha256,
            "%s is not bound to the accepted lockfile" % lane,
        )
        browser = result.get("browser")
        expected_name, minimum = LANES[lane]
        require(
            isinstance(browser, dict)
            and browser.get("name") == expected_name
            and isinstance(browser.get("version"), str),
            "%s did not record the expected real browser" % lane,
        )
        require(version_tuple(browser["version"]) >= minimum, "%s is below its supported minimum" % lane)
        driver = result.get("driver")
        require(
            isinstance(driver, dict)
            and isinstance(driver.get("name"), str)
            and driver["name"]
            and isinstance(driver.get("version"), str)
            and driver["version"],
            "%s did not record its WebDriver version" % lane,
        )
        gates = result.get("gates")
        require(
            isinstance(gates, list)
            and len(gates) == len(GATES)
            and {gate.get("gate") for gate in gates} == set(GATES),
            "%s must report every fail-closed browser gate" % lane,
        )
        require(all(gate.get("status") == "pass" for gate in gates), "%s contains a failed browser gate" % lane)
        command = result.get("command")
        require(isinstance(command, str) and command, "%s must record its command" % lane)
        browsers[lane] = browser["version"]
        commands.append(command)

    return validate_canonical_browser_evidence({
        "commit": commit,
        "lockfile_sha256": lockfile_sha256,
        "browsers": browsers,
        "commands": commands,
    }, commit, lockfile_sha256, profile)


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--initialize", action="store_true")
    parser.add_argument("--results-dir", type=Path)
    parser.add_argument("--lane")
    parser.add_argument("--browser")
    parser.add_argument("--commit", required=True)
    parser.add_argument("--lockfile-sha256", required=True)
    parser.add_argument("--profile", choices=("full", "current"), default="full")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.initialize:
        if args.results_dir or not args.lane or not args.browser:
            parser.error("--initialize requires --lane and --browser only")
        value = fail_closed_result(
            args.lane,
            args.commit,
            args.lockfile_sha256,
            args.browser,
        )
    else:
        if args.lane or args.browser or not args.results_dir:
            parser.error("collection requires --results-dir")
        value = assemble(
            args.results_dir,
            args.commit,
            args.lockfile_sha256,
            profile=args.profile,
        )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(canonical_json(value), encoding="utf-8")


if __name__ == "__main__":
    main()
