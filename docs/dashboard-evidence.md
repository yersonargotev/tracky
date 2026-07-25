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

## Unified non-publishing release dry run

Dispatch **Validate unified release dry run** with a full source SHA, its exact
Cargo package version, and the SHA-256 of its committed `Cargo.lock`. The
workflow serializes dry runs, checks all three values after every checkout, and
uses read-only repository permissions. Pull requests that change the dry-run
contract run the same DAG automatically against their exact head commit.

Rust formatting, all-target tests, strict Clippy, dependency policy, and the
release-evidence contracts run in parallel with one native Cargo Dist build for
each supported target. The quality job executes its shell checks from one
canonical command catalog and retains `release-dry-run-quality-evidence-<sha>`
only after those commands and the pinned dependency-policy action pass. Each
build retains a SHA- and target-bound bundle with
the archive, Cargo Dist checksum, Cargo Dist manifest, release identity, and
semantic manifest. The retained Cargo Dist host manifest is regenerated with
`--no-local-paths` and checked for the expected target archive, checksum, and
release linkage. Native downstream jobs download those bundles, fail on any
identity, checksum, Cargo Dist manifest, or semantic-field substitution, then
exercise the extracted CLI and packaged runtime. Build and runtime jobs execute
through a recording shell helper, so the exact successful command arrays are
retained inside their native bundles instead of reconstructed by the final
assembler. No downstream job invokes Cargo Dist.

The same run also gates on current Safari, Firefox, and Chromium. Each browser
job downloads one of the retained native bundles, compares its release identity,
revalidates its transport and semantic manifests, and exercises the extracted
package without invoking Cargo or Cargo Dist. The canonical
`release-dry-run-current-browser-evidence-<sha>` artifact binds all three lane
results to the accepted source SHA and lockfile digest.

The final `release-dry-run-evidence-<sha>` artifact is produced only after
quality, native runtime, and all three current-browser paths pass. Its canonical
JSON records the exact source/version/lock identity, tool and browser/driver
versions, retained artifact IDs and digests, complete semantic manifests,
runtime measurements, commands, objective gate outcomes, same-run job URLs, and
job timings. The assembler fetches only the current run's GitHub job and artifact
metadata, rejects missing or duplicate jobs/artifacts/browser lanes, stale
identities, failed gates, substituted bytes, and placeholder values, then emits
the same evidence as human-readable Markdown.

This automated evidence requires no maintainer, approval, or `approved-by`
identity. Both files retain `mode: dry-run` and `published: false` because they
describe the completed pre-publication gates.

For a controlled rehearsal, dispatch the same workflow with the optional
`prerelease_tag` set to `v<package-version>-rc.<positive integer>`. Tracky's
protected lightweight tag must first be created by an authorized maintainer at
the exact accepted SHA. Only after the final evidence job passes, the
publication job downloads the same verified native bundles and evidence from
that workflow run, revalidates their exact identity, archive bytes, checksums,
semantic manifests, and gate outcomes, verifies or creates the tag where
repository policy permits, then hosts a GitHub prerelease. It never invokes
Cargo Dist or rebuilds an archive.

Publication is retry-safe within the retained workflow run: an existing tag is
accepted only at the original SHA; matching uploaded assets are preserved,
missing assets are uploaded, and any unexpected or byte-different asset fails
closed. The job never clobbers assets. Its
`release-prerelease-publication-<sha>` artifact records the final release URL,
tag, identity, and GitHub asset IDs, sizes, and digests. Controlled `-rc.` tags
are ignored by the legacy Cargo Dist tag workflow so they cannot start a second
fresh build. Provenance attestation remains disabled until repository
eligibility and verification support are explicitly confirmed.

After release hosting, the reusable **Validate and publish Homebrew from a
hosted release** workflow re-downloads the durable GitHub Release assets rather
than any native build output. It validates the published release target,
complete dry-run identity and gates, archive bytes, Cargo Dist checksum files,
and API digests, then generates `Formula/tracky.rb` with the exact hosted URLs
and archive SHA-256 values. A rehearsal formula uses the full tag version, such
as `0.2.3-rc.1`, while its non-interactive test asserts the committed binary
version, such as `tracky 0.2.3`.

macOS ARM and Linux x86-64 jobs have no tap credential. Both must pass
fail-closed `brew style`, strict online audit, local formula installation,
`brew test`, installed-version, and help-smoke checks before the final job can
run. Only that final job targets the `homebrew` environment and reads
`HOMEBREW_TAP_TOKEN`; the environment has no required reviewer. Immediately
before an optimistic, lease-protected tap push it re-downloads the release,
regenerates the formula, and compares the retained preparation evidence. An
identical tap formula is a no-op. The same workflow can be dispatched manually
with an exact tag, full source SHA, and package version to recover a tap failure
from the hosted assets without rebuilding or replacing the release.

## Release manifest

Before assembling release proof, dispatch **Build dashboard release candidate**
with the exact commit SHA. It builds each native Cargo Dist
archive, exercises the packaged CLI, runs the deterministic 120-month/100,000-
transaction runtime and 100 refresh cycles, and retains per-target fragments for
90 days. A target job fails instead of fabricating a passing fragment.

Candidate fragments are inputs to release proof, not approval by themselves.
Installer, Homebrew, and all six real-browser lanes still require retained
evidence before the release manifest can pass. Automated axe checks in those
browser lanes are the release accessibility gate; personal Tracky releases do
not require separate manual screen-reader attestations.

## Retained browser evidence

The unified release dry run makes current Safari, Firefox, and Chromium blocking
release gates over the exact retained packages. Minimum-version duplication is
kept outside the normal release path in **Test full dashboard browser
compatibility**. That compatibility workflow runs all six minimum/current lanes
every Monday, can be dispatched manually for an exact full SHA, and is also
required automatically for pull requests that change dashboard rendering,
assets, HTTP/browser harnesses, support metadata, or either browser workflow.
This is the targeted path for requiring broader compatibility coverage on a
dashboard-sensitive change.

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

Assemble the release manifest from the retained native candidate fragments,
browser evidence, and the required automated gate results:

```sh
python3 scripts/dashboard_candidate_manifest.py \
  --targets-dir target-fragments \
  --browsers browsers.json \
  --results results.json \
  --inventory inventory.json \
  --maintainer "$GITHUB_ACTOR" \
  --approved-by "$APPROVER" \
  --output dashboard-verification.json
```

## Publication gate

Release evidence is produced outside the source tree from retained real-target
output; it is never hand-marked complete by CI. After every automated gate has
passed, dispatch **Approve dashboard release proof** on the exact commit
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
