#!/usr/bin/env python3
"""Assemble exact-SHA release-browser lane results into canonical evidence."""

import argparse
import json
import re
from pathlib import Path


LANES = {
    "safari-minimum": ("safari", (26, 0)),
    "safari-latest": ("safari", (26, 0)),
    "firefox-esr-minimum": ("firefox", (153,)),
    "firefox-latest": ("firefox", (153,)),
    "chromium-minimum": ("chromium", (150,)),
    "chromium-latest": ("chromium", (150,)),
}
GATES = {
    "browser-flow",
    "progressive-no-javascript",
    "security",
    "lifecycle",
    "automated-accessibility",
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


def assemble(results_dir, commit, lockfile_sha256):
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
    require(len(results) == len(LANES), "results directory must contain exactly six lane results")
    by_lane = {result.get("lane"): result for result in results}
    require(set(by_lane) == set(LANES), "lane results must cover the exact six-lane matrix")

    browsers = {}
    commands = []
    for lane in sorted(LANES):
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
            and {gate.get("gate") for gate in gates} == GATES,
            "%s must report every fail-closed browser gate" % lane,
        )
        require(all(gate.get("status") == "pass" for gate in gates), "%s contains a failed browser gate" % lane)
        command = result.get("command")
        require(isinstance(command, str) and command, "%s must record its command" % lane)
        browsers[lane] = browser["version"]
        commands.append(command)

    return {
        "commit": commit,
        "lockfile_sha256": lockfile_sha256,
        "browsers": browsers,
        "commands": commands,
    }


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", type=Path, required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--lockfile-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    value = assemble(args.results_dir, args.commit, args.lockfile_sha256)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(canonical_json(value), encoding="utf-8")


if __name__ == "__main__":
    main()
