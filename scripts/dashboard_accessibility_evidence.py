#!/usr/bin/env python3
"""Validate, bind, render, and retain manual dashboard accessibility evidence."""

import argparse
import copy
import datetime
import hashlib
import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCHEMA = ROOT / "evidence" / "dashboard" / "manual-accessibility.schema.json"
TEMPLATE = ROOT / "evidence" / "dashboard" / "manual-accessibility.template.json"
HASH = re.compile(r"^[0-9a-f]{64}$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")
DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
SIGNED_AT = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
RUN_URL = re.compile(r"^https://github\.com/yersonargotev/tracky/actions/runs/([1-9]\d*)$")
PLACEHOLDERS = {"", "todo", "tbd", "not run", "not-run", "not_run", "placeholder", "unknown", "n/a", "unassigned"}

COMMON_CHECKS = {
    "Keyboard-only operation",
    "Visible and restored focus",
    "200 percent zoom",
    "320 CSS pixel reflow",
    "WCAG 2.2 AA contrast",
    "Pointer target size",
    "Reduced motion",
    "Refresh and error announcements",
    "No color-only meaning",
}
PLATFORMS = {
    "safari-voiceover": {
        "target": "aarch64-apple-darwin",
        "browser": "Safari",
        "assistive_technology": "VoiceOver",
        "required_check": "VoiceOver with Safari",
        "os_pattern": re.compile(r"macos", re.IGNORECASE),
    },
    "firefox-orca": {
        "target": "x86_64-unknown-linux-gnu",
        "browser": "Firefox",
        "assistive_technology": "Orca",
        "required_check": "Orca with Firefox",
        "os_pattern": re.compile(r"linux|ubuntu|fedora", re.IGNORECASE),
    },
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_json(path):
    with Path(path).open(encoding="utf-8") as handle:
        return json.load(handle)


def canonical_json(value):
    return json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def meaningful(value):
    return isinstance(value, str) and value.strip().casefold() not in PLACEHOLDERS


def validate_json_schema(value, schema, root=None, path="$" ):
    root = schema if root is None else root
    if "$ref" in schema:
        reference = schema["$ref"]
        require(reference.startswith("#/"), "%s uses an unsupported schema reference" % path)
        resolved = root
        for part in reference[2:].split("/"):
            resolved = resolved[part]
        validate_json_schema(value, resolved, root, path)
        return
    if "const" in schema:
        expected = schema["const"]
        require(type(value) is type(expected) and value == expected, "%s does not match the schema constant" % path)
    if "enum" in schema:
        require(value in schema["enum"], "%s is not an allowed schema value" % path)
    expected_type = schema.get("type")
    matches_type = {
        "object": isinstance(value, dict),
        "array": isinstance(value, list),
        "string": isinstance(value, str),
        "integer": isinstance(value, int) and not isinstance(value, bool),
    }.get(expected_type, True)
    require(matches_type, "%s does not match schema type %s" % (path, expected_type))
    if isinstance(value, dict):
        required = set(schema.get("required", []))
        require(required <= set(value), "%s is missing schema-required fields" % path)
        properties = schema.get("properties", {})
        if schema.get("additionalProperties") is False:
            require(set(value) <= set(properties), "%s has schema-forbidden fields" % path)
        for name, child in value.items():
            if name in properties:
                validate_json_schema(child, properties[name], root, "%s.%s" % (path, name))
    elif isinstance(value, list):
        require(len(value) >= schema.get("minItems", 0), "%s has too few schema items" % path)
        if "maxItems" in schema:
            require(len(value) <= schema["maxItems"], "%s has too many schema items" % path)
        if schema.get("uniqueItems"):
            encoded = [json.dumps(item, sort_keys=True) for item in value]
            require(len(encoded) == len(set(encoded)), "%s schema items must be unique" % path)
        if "items" in schema:
            for index, child in enumerate(value):
                validate_json_schema(child, schema["items"], root, "%s[%d]" % (path, index))
    elif isinstance(value, str):
        require(len(value) >= schema.get("minLength", 0), "%s is shorter than the schema minimum" % path)
        if "pattern" in schema:
            require(re.search(schema["pattern"], value) is not None, "%s does not match the schema pattern" % path)
        if schema.get("format") == "date":
            try:
                datetime.date.fromisoformat(value)
            except ValueError as error:
                raise ValueError("%s does not match the schema date format" % path) from error
    elif isinstance(value, int) and not isinstance(value, bool) and "minimum" in schema:
        require(value >= schema["minimum"], "%s is below the schema minimum" % path)


def check_contract():
    schema = read_json(SCHEMA)
    required = {
        "schema_version", "commit", "lockfile_sha256", "candidate_run_id",
        "candidate_run_url", "responsible_maintainer", "maintainer_sign_off", "platforms",
    }
    require(set(schema["required"]) == required, "manual accessibility schema top-level fields drifted")
    require(set(schema["properties"]["platforms"]["required"]) == set(PLATFORMS), "manual accessibility schema platform coverage drifted")
    require(set(schema["properties"]["platforms"]["properties"]) == set(PLATFORMS), "manual accessibility schema platform properties drifted")
    template = read_json(TEMPLATE)
    require(set(template) == required, "manual accessibility template fields drifted")
    require(template["responsible_maintainer"] == "unassigned", "manual accessibility template must not name an approver")
    require(template["maintainer_sign_off"].get("status") == "not_run", "manual accessibility template must not pre-claim maintainer sign-off")
    require(set(template["platforms"]) == set(PLATFORMS), "manual accessibility template platform coverage drifted")
    for name, platform in template["platforms"].items():
        expected = COMMON_CHECKS | {PLATFORMS[name]["required_check"]}
        require({item.get("check") for item in platform["checks"]} == expected, "%s template checks drifted" % name)
        require(all(item.get("status") == "not_run" for item in platform["checks"]), "%s template must not pre-claim a check" % name)
        require(platform["sign_off"].get("status") == "not_run", "%s template must not pre-claim sign-off" % name)


def validate_run_url(url, expected_run_id=None):
    match = RUN_URL.fullmatch(url) if isinstance(url, str) else None
    require(match is not None, "candidate and retained run URLs must name a Tracky Actions run")
    if expected_run_id is not None:
        require(int(match.group(1)) == expected_run_id, "candidate run URL and ID differ")


def validate_platform(name, value):
    spec = PLATFORMS[name]
    require(set(value) == {
        "environment", "target", "operating_system", "browser",
        "assistive_technology", "artifact", "tester", "date", "checks", "sign_off",
    }, "%s environment fields are incomplete" % name)
    for field in ("environment", "operating_system", "tester"):
        require(meaningful(value[field]), "%s %s is missing or placeholder" % (name, field))
    require(value["target"] == spec["target"], "%s target is not the required packaged candidate" % name)
    require(spec["os_pattern"].search(value["operating_system"]) is not None, "%s operating system is invalid" % name)
    browser = value["browser"]
    require(set(browser) == {"name", "version"} and browser["name"] == spec["browser"] and meaningful(browser["version"]), "%s browser and version are invalid" % name)
    assistive = value["assistive_technology"]
    require(set(assistive) == {"name", "version"} and assistive["name"] == spec["assistive_technology"] and meaningful(assistive["version"]), "%s assistive technology and version are invalid" % name)
    artifact = value["artifact"]
    expected_name = "tracky-%s.tar.xz" % spec["target"]
    require(set(artifact) == {"name", "sha256"} and artifact["name"] == expected_name and isinstance(artifact["sha256"], str) and HASH.fullmatch(artifact["sha256"]), "%s artifact binding is invalid" % name)
    require(isinstance(value["date"], str) and DATE.fullmatch(value["date"]), "%s date must use YYYY-MM-DD" % name)

    checks = value["checks"]
    required_checks = COMMON_CHECKS | {spec["required_check"]}
    require(isinstance(checks, list) and len(checks) == len(required_checks), "%s must contain exactly its required checks" % name)
    require({item.get("check") for item in checks if isinstance(item, dict)} == required_checks, "%s must contain exactly its required checks" % name)
    for item in checks:
        require(set(item) == {"check", "status", "findings", "evidence"}, "%s check fields are incomplete" % name)
        require(item["status"] == "pass", "%s manual checks must pass" % name)
        require(meaningful(item["findings"]) and meaningful(item["evidence"]), "%s check findings and evidence cannot be placeholder" % name)

    sign_off = value["sign_off"]
    require(set(sign_off) == {"status", "signed_by", "signed_at"}, "%s sign-off fields are incomplete" % name)
    require(sign_off["status"] == "pass" and meaningful(sign_off["signed_by"]), "%s evidence must be signed" % name)
    require(sign_off["signed_by"] == value["tester"], "%s tester must sign their evidence" % name)
    require(isinstance(sign_off["signed_at"], str) and SIGNED_AT.fullmatch(sign_off["signed_at"]), "%s signed timestamp is invalid" % name)


def validate_submission(value, expected_commit, expected_lockfile_sha256):
    validate_json_schema(value, read_json(SCHEMA))
    require(set(value) == {
        "schema_version", "commit", "lockfile_sha256", "candidate_run_id",
        "candidate_run_url", "responsible_maintainer", "maintainer_sign_off", "platforms",
    }, "manual accessibility submission fields do not match the schema")
    require(value["schema_version"] == 1, "unsupported manual accessibility schema")
    require(isinstance(value["commit"], str) and COMMIT.fullmatch(value["commit"]), "submission commit must be a full SHA")
    require(value["commit"] == expected_commit, "manual accessibility evidence does not match the accepted commit")
    require(isinstance(value["lockfile_sha256"], str) and HASH.fullmatch(value["lockfile_sha256"]), "invalid submission lockfile hash")
    require(value["lockfile_sha256"] == expected_lockfile_sha256, "manual accessibility evidence does not match the accepted lockfile")
    require(isinstance(value["candidate_run_id"], int) and not isinstance(value["candidate_run_id"], bool) and value["candidate_run_id"] > 0, "candidate run ID is invalid")
    validate_run_url(value["candidate_run_url"], value["candidate_run_id"])
    require(meaningful(value["responsible_maintainer"]), "responsible maintainer is missing or placeholder")
    maintainer_sign_off = value["maintainer_sign_off"]
    require(maintainer_sign_off["status"] == "pass" and meaningful(maintainer_sign_off["signed_by"]), "responsible maintainer must sign the evidence")
    require(maintainer_sign_off["signed_by"] == value["responsible_maintainer"], "responsible maintainer signature identity differs")
    require(SIGNED_AT.fullmatch(maintainer_sign_off["signed_at"]), "responsible maintainer signed timestamp is invalid")
    require(isinstance(value["platforms"], dict) and set(value["platforms"]) == set(PLATFORMS), "Safari/VoiceOver and Firefox/Orca evidence are both mandatory")
    for name in sorted(PLATFORMS):
        validate_platform(name, value["platforms"][name])


def validate_canonical(value):
    require(set(value) == {
        "schema_version", "commit", "lockfile_sha256", "candidate_run_id",
        "candidate_run_url", "responsible_maintainer", "maintainer_sign_off", "platforms", "status",
        "retained_run_url",
    }, "canonical accessibility evidence fields do not match the schema")
    submission = {key: copy.deepcopy(value[key]) for key in value if key not in {"status", "retained_run_url"}}
    validate_submission(submission, value["commit"], value["lockfile_sha256"])
    require(value["status"] == "pass", "canonical accessibility evidence must pass")
    validate_run_url(value["retained_run_url"])


def verify_candidates(value, candidates_dir):
    candidates_dir = Path(candidates_dir)
    for name in sorted(PLATFORMS):
        platform = value["platforms"][name]
        target = platform["target"]
        fragments = list(candidates_dir.rglob(target + ".json"))
        archives = list(candidates_dir.rglob(platform["artifact"]["name"]))
        require(len(fragments) == 1 and len(archives) == 1, "downloaded candidates must contain exactly one archive and fragment for %s" % target)
        fragment = read_json(fragments[0])
        require(fragment.get("target") == target, "candidate fragment target differs for %s" % target)
        require(fragment.get("commit") == value["commit"], "candidate fragment commit differs for %s" % target)
        require(fragment.get("lockfile_sha256") == value["lockfile_sha256"], "candidate fragment lockfile differs for %s" % target)
        recorded = fragment.get("artifact", {})
        archive = archives[0]
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        require(recorded.get("name") == archive.name and recorded.get("sha256") == digest and recorded.get("bytes") == archive.stat().st_size, "candidate fragment does not match downloaded archive for %s" % target)
        require(platform["artifact"]["sha256"] == digest, "signed accessibility evidence names a different packaged candidate for %s" % target)


def finalize(value, retained_run_url):
    canonical = copy.deepcopy(value)
    canonical["status"] = "pass"
    canonical["retained_run_url"] = retained_run_url
    validate_canonical(canonical)
    return canonical


def render(value):
    validate_canonical(value)
    lines = [
        "# Dashboard manual accessibility evidence", "",
        "- Status: `pass`",
        "- Commit: `%s`" % value["commit"],
        "- Lockfile: `%s`" % value["lockfile_sha256"],
        "- Candidate run: %s" % value["candidate_run_url"],
        "- Retained evidence run: %s" % value["retained_run_url"],
        "- Responsible maintainer: %s" % value["responsible_maintainer"],
        "- Maintainer signed: %s at %s" % (value["maintainer_sign_off"]["signed_by"], value["maintainer_sign_off"]["signed_at"]),
    ]
    for name in sorted(PLATFORMS):
        platform = value["platforms"][name]
        lines.extend([
            "", "## %s" % platform["environment"], "",
            "- Target: `%s`" % platform["target"],
            "- Operating system: %s" % platform["operating_system"],
            "- Browser: %s %s" % (platform["browser"]["name"], platform["browser"]["version"]),
            "- Assistive technology: %s %s" % (platform["assistive_technology"]["name"], platform["assistive_technology"]["version"]),
            "- Candidate SHA-256: `%s`" % platform["artifact"]["sha256"],
            "- Tester/date: %s / %s" % (platform["tester"], platform["date"]),
            "- Signed: %s at %s" % (platform["sign_off"]["signed_by"], platform["sign_off"]["signed_at"]),
            "", "| Check | Status | Findings | Evidence |", "| --- | --- | --- | --- |",
        ])
        for item in sorted(platform["checks"], key=lambda check: check["check"]):
            cells = [str(item[key]).replace("|", "\\|").replace("\n", " ") for key in ("check", "status", "findings", "evidence")]
            lines.append("| %s | %s | %s | %s |" % tuple(cells))
    return "\n".join(lines) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("check")
    validate = sub.add_parser("validate")
    validate.add_argument("submission", type=Path)
    validate.add_argument("--commit", required=True)
    validate.add_argument("--lockfile-sha256", required=True)
    verify = sub.add_parser("verify-candidates")
    verify.add_argument("submission", type=Path)
    verify.add_argument("--commit", required=True)
    verify.add_argument("--lockfile-sha256", required=True)
    verify.add_argument("--candidates-dir", type=Path, required=True)
    finish = sub.add_parser("finalize")
    finish.add_argument("submission", type=Path)
    finish.add_argument("--commit", required=True)
    finish.add_argument("--lockfile-sha256", required=True)
    finish.add_argument("--candidates-dir", type=Path, required=True)
    finish.add_argument("--run-url", required=True)
    finish.add_argument("--output", type=Path, required=True)
    markdown = sub.add_parser("render")
    markdown.add_argument("evidence", type=Path)
    markdown.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)

    if args.command == "check":
        check_contract()
    elif args.command in {"validate", "verify-candidates", "finalize"}:
        value = read_json(args.submission)
        validate_submission(value, args.commit, args.lockfile_sha256)
        if args.command in {"verify-candidates", "finalize"}:
            verify_candidates(value, args.candidates_dir)
        if args.command == "finalize":
            args.output.write_text(canonical_json(finalize(value, args.run_url)), encoding="utf-8")
    else:
        args.output.write_text(render(read_json(args.evidence)), encoding="utf-8")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError) as error:
        print("dashboard accessibility evidence error: %s" % error, file=sys.stderr)
        sys.exit(1)
