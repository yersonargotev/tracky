# Dashboard evidence foundation

This workflow freezes the last dashboard-free Cargo Dist artifacts before
dashboard code or dependencies are introduced. The immutable source of truth is
`evidence/dashboard/baseline.json`; its commit, toolchain, lockfile, release run,
hashes, byte counts, and archive allowlists must not be moved by a change that
benefits from a larger baseline. The exact accepted archives and per-target JSON
are retained in the technical
[`dashboard-baseline-f95fdaf` release](https://github.com/yersonargotev/tracky/releases/tag/dashboard-baseline-f95fdaf).

## Fast pull-request gates

```sh
python3 scripts/dashboard_evidence.py check
python3 -m unittest tests/dashboard_evidence_tool.py
cargo deny check advisories bans licenses sources
```

`check` validates the baseline and the machine-readable release-evidence
template, then proves that `dependency-inventory.json` matches the locked
resolved graph and that notices exist. Regenerate the inventory and the reviewed
full-license-text notices only with the pinned tools:

```sh
python3 scripts/dashboard_evidence.py inventory
cargo-about generate --locked evidence/dashboard/third-party-notices.hbs \
  | python3 -c 'import sys; print("\\n".join(line.rstrip() for line in sys.stdin.read().rstrip().splitlines()))' \
  > THIRD-PARTY-NOTICES
```

The cargo-deny binary is pinned to 0.20.2 in CI. `deny.toml` rejects
vulnerabilities, unsoundness, yanked packages, wildcard direct dependencies,
unknown registries, Git sources, unknown licenses, and duplicate SQLite storage
crates. Transitive unmaintained packages are reported for review because the
current `ttf-parser` advisory has no safe upgrade; direct/workspace abandonment
still fails. Existing general duplicates remain visible as warnings and are
bounded by the frozen package count; this prevents silently converting the
existing graph into an unrelated dependency cleanup.

## Artifact and static-asset comparison

Build all three Cargo Dist targets with the versions recorded by the baseline,
then measure and compare them:

```sh
python3 scripts/dashboard_evidence.py measure \
  --artifacts target/distrib --assets assets/dashboard --output current.json
python3 scripts/dashboard_evidence.py compare --current current.json
```

The comparison enforces the accepted limits independently: assets at most
250 KiB; no more than 60 added resolved packages; and both the absolute and
20-percent ceilings for every binary and archive. Omitting a target fails.

## Release manifest

Copy `evidence/dashboard/dashboard-verification.template.json`, populate it only
from retained command output, and validate/render it with:

```sh
python3 scripts/dashboard_evidence.py validate dashboard-verification.json
python3 scripts/dashboard_evidence.py render dashboard-verification.json \
  --output dashboard-verification.md
python3 scripts/dashboard_evidence.py validate --release dashboard-verification.json
```

The normal validator permits `not_run` while implementation slices are still in
progress. Release validation fails unless every recorded gate passes and an
identified maintainer approves the evidence. The JSON Schema beside the template
is the interchange contract; the Python validator is the fail-closed CI entry
point and the Markdown renderer consumes that same validated input.
