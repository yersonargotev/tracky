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

## Semantic native-package manifest

Native release archives have two separate integrity contracts:

- `transport.archive_sha256` identifies the exact compressed archive retained,
  tested, published to GitHub Releases, and referenced by Homebrew;
- the remaining semantic identity describes the meaningful extracted package
  and deliberately excludes tar ordering, timestamps, ownership metadata,
  padding, xz framing, and compressed byte count.

Generate a target-bound manifest on the native runner that built the archive:

```sh
python3 scripts/dashboard_evidence.py semantic-manifest \
  --archive "target/distrib/tracky-${TARGET}.tar.xz" \
  --target "$TARGET" \
  --source-sha "$(git rev-parse HEAD)" \
  --lockfile-sha256 "$(python3 -c 'import hashlib; print(hashlib.sha256(open("Cargo.lock", "rb").read()).hexdigest())')" \
  --cargo-dist-manifest dist-manifest.json \
  --package-version "$PACKAGE_VERSION" \
  --rust-version "$(rustc --version)" \
  --cargo-version "$(cargo --version)" \
  --cargo-dist-version "$(dist --version)" \
  --output "semantic-${TARGET}.json"
python3 scripts/dashboard_evidence.py validate-semantic "semantic-${TARGET}.json" \
  --archive "target/distrib/tracky-${TARGET}.tar.xz" \
  --cargo-dist-manifest dist-manifest.json
```

Generation fails unless the archive has one correctly named root, the exact
release allowlist, safe regular-file paths, no links or special files, mode
`0755` only for `tracky`, mode `0644` for the documentation files, the expected
target architecture, source-controlled documentation bytes, and a packaged
`tracky --version` matching the requested package version. The canonical JSON
records sorted per-file uncompressed sizes and SHA-256 values together with the
source SHA, lockfile digest, target, package version, and Rust/Cargo/Cargo Dist
versions plus the Cargo Dist build-manifest digest. Validation reopens the
downloaded archive and rechecks its exact transport checksum, every semantic
file field, and the packaged version. Run both commands on the matching native
target because version validation executes the packaged binary.

## Production release DAG

A merge to `main` starts the production release DAG only when the parsed Cargo
package version changed from the merge's predecessor. An ordinary merge cannot
republish an existing version. All release runs share one constant concurrency
group with `cancel-in-progress: false`, so a newer merge queues behind an active
release instead of canceling it.

The identity job binds the run to the exact full source SHA, package version,
and committed `Cargo.lock` SHA-256. Rust formatting, all-target tests, strict
Clippy, dependency policy, and the release-evidence contracts run alongside
exactly one native Cargo Dist build for each supported target. Each native build
retains a SHA- and target-bound bundle containing the archive, Cargo Dist
checksum and manifest, release identity, semantic manifest, and recorded build
commands. Native runtime and browser jobs download those bundles and fail on any
identity, checksum, manifest, or semantic substitution; no downstream job
invokes Cargo Dist.

Current Safari, Firefox, and Chromium are blocking lanes in the same DAG. Each
lane exercises the extracted retained package, not a debug rebuild. The
pre-publication evidence is assembled only after quality, both native runtime
paths, and all three current-browser paths pass. It records source/version/lock
identity, tool and browser versions, cache state, artifact IDs and digests,
semantic manifests, runtime measurements, commands, gate outcomes, job URLs,
and queue/job timings. Release artifacts and evidence are retained for 14 days.

Publication downloads the verified native bundles from that same workflow run,
revalidates their exact transport checksums and gate evidence, creates the
stable tag and GitHub Release, and uploads those exact tested archives. It never
rebuilds or substitutes an archive. Matching tags, releases, and assets are
preserved on retry; a conflicting target or byte-different asset fails closed.
Homebrew then validates the hosted release on macOS ARM and Linux x86-64 and
publishes a formula whose URLs and SHA-256 values identify the same archives.
Only the final tap update reads `HOMEBREW_TAP_TOKEN` through the `homebrew`
environment, which is restricted to deployments from `main`.

The terminal production summary records queue and job timings, cache state,
artifact identities, the GitHub Release and asset URLs, and the Homebrew
publication URL. Report p50/p95 release timing as an SLO only after at least ten
completed production releases exist; before then, report individual-run data
without claiming a percentile baseline.

## Recovery

Rerun failed jobs from the original workflow run first. Successful upstream
jobs and their retained same-run artifacts are reused, so recovery does not
rebuild or rewrite tested archives. While the 14-day artifacts remain available,
publication reconciliation preserves matching remote state and fills only
missing matching state.

If a new run is required, manually dispatch the production release workflow
with the explicit 40-character lowercase SHA from `main`. The dispatch executes
the same serialized, non-canceling DAG and derives version and lock identity
from that commit; it does not accept caller-supplied archive identity. Never
replace retained archive identity or waive a transport mismatch. The exact
transport SHA-256 belongs to the one archive that was built, tested, and
published.

## Retained browser evidence

The production release DAG makes current Safari, Firefox, and Chromium blocking
release gates over the exact retained packages. Minimum-version duplication is
kept outside the normal release path in **Test full dashboard browser
compatibility**. That compatibility workflow runs all six minimum/current lanes
every Monday, can be dispatched manually for an exact full SHA, and is also
required automatically for pull requests that change dashboard rendering,
assets, HTTP/browser harnesses, or support metadata. This is the targeted path
for requiring broader compatibility coverage on a dashboard-sensitive change.

The compatibility workflow runs minimum and current Safari, Firefox ESR and
current Firefox, and minimum and current Chromium. Safari uses the installed
SafariDriver on GitHub's pinned/current macOS images; Firefox and Chromium are
installed explicitly for their matrix lanes. Every lane records the browser and
driver versions reported by WebDriver and rejects a version below the documented
support floor.

Each current and compatibility lane fails closed on the browser interaction
flow, progressive rendering without JavaScript, loopback/security invariants,
database and HTTP read-only behavior, process lifecycle cleanup, and axe
automated accessibility checks. Compatibility raw JSON is retained for 90 days
even when a lane fails. Its final job accepts only six passing, non-duplicate
lane results bound to the resolved commit and its `Cargo.lock` SHA-256, then
retains `browsers.json` together with all raw results as
`dashboard-browser-evidence-<commit>`.

The compatibility artifact is diagnostic evidence for support-floor changes;
it is not copied into or approved separately for a production release. CI's
faster Chromium/Firefox/WebKit debug-build flow remains a pull-request gate and
is not release evidence.
