# Dashboard evidence foundation

This workflow freezes the last dashboard-free Cargo Dist artifacts before
dashboard code or dependencies are introduced. The immutable source of truth is
`evidence/dashboard/baseline.json`; its commit, toolchain, lockfile, release run,
hashes, byte counts, and archive allowlists must not be moved by a change that
benefits from a larger baseline. The exact accepted archives and per-target JSON
are retained in the technical
[`dashboard-baseline-f95fdaf` release](https://github.com/yersonargotev/tracky/releases/tag/dashboard-baseline-f95fdaf).
Its Intel artifact remains only as immutable historical evidence; Intel macOS is
not an active build, measurement, or support target.

## Fast pull-request gates

```sh
python3 scripts/dashboard_evidence.py check
python3 -m unittest tests/dashboard_evidence_tool.py
python3 -m unittest tests/dashboard_browser_evidence.py
cargo deny check advisories bans licenses sources
```

`check` validates the baseline and the machine-readable release-evidence
template, then proves that `dependency-inventory.json` matches the locked
resolved graph and that notices exist. Regenerate the inventory and the reviewed
full-license-text notices only with the pinned tools:

```sh
python3 scripts/dashboard_evidence.py inventory
cargo-about generate --frozen evidence/dashboard/third-party-notices.hbs \
  | python3 -c 'import sys; print("\n".join(line.rstrip() for line in sys.stdin.read().rstrip().splitlines()))' \
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

Build both supported Cargo Dist targets with the versions recorded by the
baseline, then measure and compare them:

```sh
python3 scripts/dashboard_evidence.py measure \
  --artifacts target/distrib --output current.json
python3 scripts/dashboard_evidence.py compare --current current.json
```

The comparison enforces the accepted limits independently: assets at most
250 KiB; no more than 60 added resolved packages; and both the absolute and
20-percent ceilings for every binary and archive. Omitting a target fails.

## Release manifest

Before manual browser and accessibility sign-off, dispatch **Build dashboard
release candidate** with the exact commit SHA. It builds each native Cargo Dist
archive, exercises the packaged CLI, runs the deterministic 120-month/100,000-
transaction runtime and 100 refresh cycles, and retains per-target fragments for
90 days. A target job fails instead of fabricating a passing fragment.

Candidate fragments are inputs to release proof, not approval by themselves.
Installer, Homebrew, all six real-browser lanes, and manual accessibility
results still require retained evidence before the release manifest can pass.

## Retained browser evidence

After **Build dashboard release candidate** passes, dispatch **Test dashboard
release browsers** with the same full commit SHA. The workflow builds the Cargo
Dist packages for that commit and runs six independent lanes against the
extracted packaged executable: minimum and current Safari, Firefox ESR and
current Firefox, and minimum and current Chromium. Safari uses the installed
SafariDriver on GitHub's pinned/current macOS images; Firefox and Chromium are
installed explicitly for their matrix lanes. Every lane records the browser and
driver versions reported by WebDriver and rejects a version below the documented
support floor.

Each lane fails closed on the browser interaction flow, progressive rendering
without JavaScript, loopback/security invariants, process lifecycle cleanup, and
axe automated accessibility checks. Its raw JSON is retained for 90 days even
when the lane fails. The final job accepts only six passing, non-duplicate lane
results bound to the dispatched commit and its `Cargo.lock` SHA-256, then retains
`browsers.json` together with all raw results as
`dashboard-browser-evidence-<commit>`.

Download `browsers.json` from that artifact and pass it unchanged as `--browsers`
to `scripts/dashboard_candidate_manifest.py`. The assembler rechecks its commit
and lockfile binding against the native candidate fragments; CI's faster
Chromium/Firefox/WebKit debug-build flow remains a pull-request gate and is not
release evidence.

Copy `evidence/dashboard/dashboard-verification.template.json`, populate it only
from retained command output, and validate/render it with:

```sh
python3 scripts/dashboard_evidence.py validate dashboard-verification.json
python3 scripts/dashboard_evidence.py render dashboard-verification.json \
  --output dashboard-verification.md
python3 scripts/dashboard_evidence.py validate --release \
  --commit "$(git rev-parse HEAD)" \
  --lockfile-sha256 "$(python3 -c 'import hashlib; print(hashlib.sha256(open("Cargo.lock", "rb").read()).hexdigest())')" \
  dashboard-verification.json
```

The normal validator permits `not_run` while implementation slices are still in
progress. Release validation fails unless every recorded gate passes and an
identified maintainer approves the evidence. The JSON Schema beside the template
is the interchange contract; the Python validator is the fail-closed CI entry
point and the Markdown renderer consumes that same validated input.

Release proof must name all six supported browser lanes (`safari-minimum`,
`safari-latest`, `firefox-esr-minimum`, `firefox-latest`, `chromium-minimum`,
and `chromium-latest`) with versions at or above the documented minimums. For
each Cargo Dist target, `measurements.latency` records the warm-up/run counts and
all p95 dashboard budgets, `measurements.resources` records the idle/peak and
100-cycle stability budgets, and `measurements.sizes` records the binary and
archive hashes and byte counts. Release validation rejects missing targets,
out-of-budget values, placeholder evidence, or measurements that disagree with
the artifact records.
Each passing gate links the retained Tracky GitHub Actions run (or job) that
produced its raw output; arbitrary HTTPS locations are not accepted. During the
tag workflow, executable hashes and sizes are re-read from the packaged binaries,
while asset bytes and resolved-package counts are rebound to the accepted source
tree and inventory.

The named manual WCAG release-candidate inputs live in
`evidence/dashboard/manual-accessibility-checklist.md`. They intentionally remain
`not run` during implementation; browser automation must not pre-claim
VoiceOver, Orca, keyboard, zoom, contrast, target, motion, announcement, or
non-color passage.

## Publication gate

Release evidence is produced outside the source tree from retained real-target
output; it is never hand-marked complete by CI. After every automated and manual
gate has passed, dispatch **Approve dashboard release proof** on the exact commit
to be tagged, supplying an HTTPS URL and SHA-256 for
`dashboard-verification.json`. The protected `dashboard-release` environment is
the maintainer approval boundary. That workflow validates the accepted commit
and lockfile, renders the Markdown form, and retains both under an artifact name
bound to the commit.

On a release tag, Cargo Dist builds fresh artifacts. Before `host`, Homebrew, or
announcement jobs can run, `verify-dashboard-release` downloads the approved
proof for that exact SHA, validates it with:

```sh
python3 scripts/dashboard_evidence.py validate --release \
  --commit "$GITHUB_SHA" --lockfile-sha256 "$LOCKFILE_SHA256" \
  dashboard-verification.json
python3 scripts/dashboard_evidence.py verify-artifacts \
  dashboard-verification.json --artifacts target/distrib
```

The second command binds every recorded byte count and hash to the newly built
archive, its Cargo Dist checksum, exact file allowlist, safe paths, and executable
mode. Publication fails closed when proof is missing, stale, incomplete,
unapproved, or differs from the artifacts. Both JSON and Markdown files join the
release artifact set permanently.
