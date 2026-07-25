# Release-pipeline simplification

Research date: 2026-07-24

## Question

How can Tracky preserve Rust CI, native macOS/Linux packages, and meaningful
dashboard confidence while replacing the current manual, rebuild-heavy release
process with one merge-to-release workflow that normally finishes in roughly
10–15 minutes?

## Executive recommendation

Use **one workflow run, one source SHA, and one build of each native package**.
On a version-changing merge to `main`, start Linux quality checks and the two
Cargo Dist native builds in parallel. Pass the resulting immutable workflow
artifacts to package tests, three current-browser smoke lanes, publication, and
Homebrew. Create `v<package.version>` only after every gate passes, then publish
the already-tested archives and generated formula. Do not rebuild after tagging.

Keep the existing pull-request CI as the pre-merge gate, including formatting,
all-target tests, Clippy with warnings denied, release compilation, and its three
browser-engine flow. In the release path, retain three **current-browser** lanes
(Safari, Firefox, and Chromium) but remove the three duplicate minimum-version
lanes. Run minimum-version compatibility on a schedule and on explicit dashboard
changes. This preserves an engine-diverse packaged gate without rebuilding the
same packages for it.

Remove the `dashboard-release` proof workflow and the required-reviewer gates on
both `dashboard-release` and `homebrew`. They are human pauses, not additional
tests. Keep publication credentials scoped to the final jobs, and protect those
jobs with `needs`, exact-SHA/version checks, minimal token permissions, and
concurrency. GitHub environments may still hold the tap secret without required
reviewers; environment protection is what causes jobs to wait, while environment
secrets remain an access-control mechanism. [GitHub deployments and environments](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments)

Treat the 10–15 minute target as a **service objective to measure**, not a
guarantee. One observed v0.2.3 path took about 37m32s across four serial workflow
runs, but it does not measure the proposed integrated graph. Hosted-runner queue
time, cold Rust compilation, Homebrew update/network time, and macOS availability
remain outside the workflow's control.

## Independent assessment of the six hypotheses

| Hypothesis | Verdict | Independent basis and conditions |
| --- | --- | --- |
| Preserve CI/tests/Clippy and macOS/Linux packages while simplifying | **Supported** | These are separable gates: repository CI already defines the Rust commands, and Cargo Dist already defines exactly two native targets. The conditions are that publication depends on every quality/build/package-test job and no target is removed. Parallel execution does not weaken dependency gating. |
| Reduce release browser lanes | **Supported with conditions** | The present six lanes pair minimum/current versions of three engines and rebuild packages. Live v0.2.3 evidence shows each actual browser lane took only 24–35 seconds after 5–6 minute rebuild jobs, so retain three current packaged engines and remove the minimum duplicates; move minimum-version checks to scheduled/targeted runs. Reject this change if every documented minimum must block every release. |
| Remove manual proof and Homebrew approvals | **Supported with conditions** | Current manual proof validates machine-produced fields and environments add reviewer waits; neither official Cargo Dist nor Homebrew guidance requires per-release human approval. Removal is reasonable only after all objective checks are automated, publication secrets remain isolated/restricted, and recovery is documented. This is not a claim that automation has zero supply-chain risk. |
| Reuse exact-SHA artifacts rather than rebuild | **Supported** | GitHub workflow artifacts are designed for inter-job handoff and are immutable in current upload-artifact generations; Cargo Dist documents custom local artifact jobs. The conditions are verified checkout SHA, SHA-bearing artifact names/metadata, explicit per-file checksum failure, and publishing the downloaded object rather than invoking another build. |
| Validate semantic archive content instead of cross-build compressed bytes | **Supported** | Cargo Dist promises ordinary build equivalence, not bit-for-bit reproducibility, while its package contract defines meaningful layout. Exact archive checksums must still be retained for transport/Homebrew integrity. Semantic comparison is appropriate only across builds; the single tested/published archive must remain byte-identical from build through release. |
| One merge→build→test→release→Homebrew workflow in 10–15 minutes | **Supported with conditions; duration unverified** | One DAG can remove manual transitions and run quality/native builds in parallel. The observed v0.2.3 path took ~37m32s, but much of that was serial workflow handoff/rebuild work; hosted runner queues/network installs remain uncontrolled. The architecture and target are plausible, but 10–15 minutes remains an SLO hypothesis until integrated p50/p95 measurements confirm it. |

## Repository facts (current state)

1. Tracky is version `0.2.3`; Cargo Dist `0.32.0` builds
   `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`, with shell and Homebrew
   installers and `yersonargotev/homebrew-tap` as the tap.
   [`Cargo.toml:1-11`](../../Cargo.toml),
   [`Cargo.toml:36-57`](../../Cargo.toml).
2. Normal CI runs formatting, `cargo test --locked --all-targets`, three
   Playwright CLI browser flows, Clippy with `-D warnings`, and a release build
   serially in one Ubuntu job. A second Ubuntu job performs dependency/evidence
   checks. [`ci.yml:16-90`](../../.github/workflows/ci.yml).
3. Candidate release packages are manually dispatched for an accepted SHA,
   built on macOS and Linux, extracted, package-tested, measured, and retained
   for 90 days. [`dashboard-release-candidate.yml:1-80`](../../.github/workflows/dashboard-release-candidate.yml),
   [`dashboard-release-candidate.yml:147-157`](../../.github/workflows/dashboard-release-candidate.yml).
4. The separate browser workflow manually accepts the same SHA but **rebuilds**
   macOS and Linux packages, then runs six lanes: minimum/current Safari,
   Firefox, and Chromium. [`dashboard-release-browsers.yml:1-68`](../../.github/workflows/dashboard-release-browsers.yml),
   [`dashboard-release-browsers.yml:70-113`](../../.github/workflows/dashboard-release-browsers.yml).
5. The proof workflow is another manual dispatch. Its job references the
   `dashboard-release` environment, downloads a caller-supplied manifest URL,
   validates it, and uploads proof named for the workflow commit.
   [`dashboard-release-proof.yml:1-50`](../../.github/workflows/dashboard-release-proof.yml).
6. The tag-triggered Cargo Dist workflow then builds the native and global
   artifacts **again**, fetches the approved proof, and compares that proof with
   the newly built archives before hosting. [`release.yml:42-46`](../../.github/workflows/release.yml),
   [`release.yml:92-214`](../../.github/workflows/release.yml),
   [`release.yml:216-276`](../../.github/workflows/release.yml).
7. Homebrew publication waits on a distinct `homebrew` environment, checks out
   the tap with `HOMEBREW_TAP_TOKEN`, downloads the formula, runs `brew update`
   and a non-failing style fix, commits, and pushes. [`release.yml:336-381`](../../.github/workflows/release.yml).
8. The evidence contract explicitly requires all six browser lanes, maintainer
   approval, exact compressed-archive bytes/hashes, and a fresh tag build.
   [`dashboard-evidence.md:58-96`](../dashboard-evidence.md),
   [`dashboard-evidence.md:111-173`](../dashboard-evidence.md).
9. The successful v0.2.3 path provides live repository timing evidence: CI
   [run 30137083741](https://github.com/yersonargotev/tracky/actions/runs/30137083741)
   ran 00:46:47–00:52:55Z, candidate
   [run 30137311674](https://github.com/yersonargotev/tracky/actions/runs/30137311674)
   ran 00:53:03–01:01:31Z, browsers
   [run 30137646185](https://github.com/yersonargotev/tracky/actions/runs/30137646185)
   ran 01:02:05–01:09:23Z, and release
   [run 30137997660](https://github.com/yersonargotev/tracky/actions/runs/30137997660)
   ran 01:12:02–01:24:19Z: about 37m32s end to end. Within the browser run,
   the two rebuild jobs consumed roughly 5–6 minutes each, while each actual
   browser lane took only 24–35 seconds and collection took 8 seconds.

The current process therefore has four human/external transitions (candidate
dispatch, browser dispatch, proof assembly/dispatch, tag push), at least three
native package builds for the same SHA (candidate, browser, tag), nine jobs in
the browser workflow alone (two package builds, six lanes, and collection), and
two separate environment references that become approval points if their
repository settings configure required reviewers.

## Proposed workflow graph

Keep `.github/workflows/ci.yml` for pull requests. Replace the three dashboard
release workflows and generated tag-only release workflow with one
`release.yml` triggered by a push to `main`, with this graph:

```text
version/check ─┬─ quality-linux (fmt, all-target tests, Clippy, evidence/unit)
               ├─ build-package [macOS arm64, Linux x86_64]
               │    └─ semantic-package-test [same matrix artifacts]
               │          └─ packaged browser smoke [Safari, Firefox, Chromium]
               └──────────────────────────────────────────┐
                                                          v
                       publish GitHub Release + attest exact archives
                                                          v
                              validate/push generated Homebrew formula
```

### Trigger and version discipline

The first job must derive the version from committed `Cargo.toml`, require a
matching change in the merge (or an explicit release marker), reject an existing
`v<version>` tag/release, and export the full `github.sha`. Without that rule,
every ordinary merge would attempt to republish `0.2.3`. Cargo Dist documents
that GitHub releases require a tagged commit and supports dispatch-based release
creation, but a merge-triggered policy remains Tracky-owned.
[Cargo Dist dispatch releases](https://axodotdev.github.io/cargo-dist/book/reference/config.html#dispatch-releases),
[Cargo Dist GitHub release configuration](https://axodotdev.github.io/cargo-dist/book/reference/config.html#github-release)

Create the tag only in the publication job, explicitly targeting the already
checked/tested SHA. The version must therefore be finalized in the PR. Use a
workflow concurrency group such as `release-main`, without cancelling an active
publication, to prevent two merges racing the single latest Homebrew formula.

### Preserve Rust quality without serializing the critical path

Retain the exact repository commands:

```sh
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked --release --bin tracky
```

For release merges, run formatting/tests/Clippy on Ubuntu in parallel with the
two native Cargo Dist builds rather than before them. Publication `needs` all of
them, so parallelism changes latency, not the fail-closed result. The standalone
release build can be omitted from the merge workflow because each Cargo Dist job
performs a release-profile build; keep it in PR CI as the fast package-independent
compile gate.

### Build once and reuse the exact SHA artifacts

Each native matrix job must check out `${{ github.sha }}`, verify
`git rev-parse HEAD`, run Cargo Dist once for its target, semantic-validate the
archive, and upload the archive, Cargo Dist checksum, build manifest, and semantic
manifest under a name containing the full SHA and target. The downstream test,
browser, release, and formula jobs download these artifacts rather than calling
`dist build` again.

This is a supported GitHub Actions data flow: artifacts are intended to pass data
between jobs, v4+ artifacts are immutable, and upload-artifact exposes an
artifact ID, URL, and SHA-256 digest. Download-artifact recalculates and checks
the transport digest, although GitHub documents a mismatch as a warning; Tracky
should additionally compare the expected per-file checksums and fail explicitly.
[GitHub artifact sharing and validation](https://docs.github.com/en/actions/tutorials/store-and-share-data),
[actions/upload-artifact outputs and immutability](https://github.com/actions/upload-artifact#outputs)

Cargo Dist explicitly supports custom local-build jobs if they produce the
expected archives/checksums and upload them for later stages, and
`dist manifest --artifacts=local --no-local-paths` reports the expected names.
That contract supports a custom unified workflow without abandoning Cargo Dist's
package layout or generated installers.
[Cargo Dist custom local builds](https://axodotdev.github.io/cargo-dist/book/reference/config.html#build-local-artifacts)

### Validate archive meaning, not accidental compression bytes

Keep SHA-256 checksums for the **published archive**: they protect download
integrity and are what the generated Homebrew formula consumes. Stop using a
previous independent build's `.tar.xz` byte count/hash as proof that a later
build is equivalent. Cargo Dist's own introduction describes expected build
repeatability as normal build equivalence, explicitly not bit-level precision.
[Cargo Dist introduction](https://axodotdev.github.io/cargo-dist/book/introduction.html)

Instead, generate a deterministic semantic manifest from the one archive that
will be tested and published. Fail unless it has:

- one safe top-level directory matching the archive basename;
- exactly `tracky`, `README.md`, `LICENSE`, and `THIRD-PARTY-NOTICES` (the
  configured includes), with no absolute paths, `..`, links, devices, or extras;
- the expected executable mode only on `tracky`;
- uncompressed byte length and SHA-256 for every regular file;
- `tracky --version` equal to the release version and packaged CLI/dashboard
  smoke tests passing after extraction;
- the full source SHA, target triple, lockfile digest, Rust/Cargo/Cargo Dist
  versions, and Cargo Dist manifest linkage.

Canonicalize this JSON by sorted path and sorted keys. Ignore tar member order,
timestamps, owners/groups, padding, xz framing, and compressed byte count when
asking whether two builds have the same meaning. This preserves the useful
allowlist and executable-content checks already present in Tracky's candidate
code while eliminating a false reproducibility requirement. Cargo Dist requires
tar archives to use a same-named root directory, providing a first-party package
layout invariant. [Cargo Dist archive contract](https://axodotdev.github.io/cargo-dist/book/reference/config.html#build-local-artifacts)

Because the recommended workflow publishes the exact archive it tested, it
normally does not need cross-build equivalence at all. Semantic comparison is
still useful for the frozen size baseline and for diagnosing a deliberate
rebuild; archive checksum verification remains mandatory within one artifact.

### Reduce browser lanes without dropping browser-engine confidence

PR CI currently exercises Chromium, Firefox, and WebKit against a debug binary,
while release evidence repeats minimum/current pairs for Safari, Firefox, and
Chromium against packages. Keep **three current packaged lanes** in the release
graph: Safari latest on the already-required macOS artifact, and Firefox latest
plus Chromium latest on the Linux artifact. Each should cover the existing
interaction flow, progressive/no-JavaScript rendering, loopback security,
lifecycle cleanup, and automated accessibility gates.

This conclusion is supported by the v0.2.3 browser run: package rebuilds took
roughly 5–6 minutes, but each of the six browser lanes took only 24–35 seconds
and collection took 8 seconds. Once the builds are reused, running three current
engines in parallel adds little critical-path time and gives materially stronger
evidence than a single Chromium lane.
[Tracky browser run 30137646185](https://github.com/yersonargotev/tracky/actions/runs/30137646185)

Move only the minimum-version duplicates outside the release critical path:

- scheduled weekly: Safari minimum, Firefox ESR minimum, and Chromium minimum;
- manual/on-demand: the complete minimum/current six-lane matrix;
- required PR labels/path filter for changes to dashboard assets, HTTP/security
  behavior, browser harnesses, or documented support floors.

This deliberately changes “minimum and current for every release” into “all
three current engines for every release plus continuous minimum-version
monitoring.” If product policy requires every documented minimum to block every
release, do not remove those lanes; the simplification must come only from
artifact reuse and workflow integration.

### Remove manual proof and Homebrew approvals

Delete the hand-assembled approval identity from the release schema. Automated
results, exact SHA, semantic manifests, logs, artifact IDs/digests, and the
release run URL are sufficient machine evidence. Keep the frozen dashboard
baseline and dependency/license checks where they still provide objective gates.

Required reviewers on environments explicitly pause jobs until a person approves
them. Removing those protection rules removes the pause; deleting environments
is not required. Keep `HOMEBREW_TAP_TOKEN` in the `homebrew` environment if
desired, restrict that environment to `main`/release tags, and give only the
Homebrew job access to it.
[GitHub environment protection rules](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments)

Cargo Dist already generates Homebrew formulae that install its prebuilt macOS
and Linux archives; its standard publisher commits those generated files to the
configured tap. No separate human approval is required by Cargo Dist or Homebrew.
[Cargo Dist Homebrew installer](https://axodotdev.github.io/cargo-dist/book/installers/homebrew.html)

Before pushing the formula, make validation fail closed rather than using the
current `brew style ... || true`: run at least `brew style`, `brew audit`, install
from the local formula/tap, and `brew test` or a direct installed `tracky
--version`/smoke check. Homebrew documents `brew audit --strict --online` for
formula conformance and a `test do` block executed by `brew test`; the generated
formula should gain a meaningful non-interactive test if Cargo Dist permits a
stable customization seam.
[Homebrew Formula Cookbook: audit](https://docs.brew.sh/Formula-Cookbook#audit-the-formula),
[Homebrew Formula Cookbook: tests](https://docs.brew.sh/Formula-Cookbook#add-a-test-to-the-formula)

Publish the GitHub Release before the tap commit. Cargo Dist warns that Homebrew
formulae reference hosted release URLs and can briefly fail if the formula is
published first. [Cargo Dist GitHub release ordering](https://axodotdev.github.io/cargo-dist/book/reference/config.html#github-release)

### Optional artifact attestations

Attest the final archives (and optionally the installer/formula) in the build or
publication workflow with `id-token: write` and `attestations: write`, then link
verification instructions from the release. GitHub artifact attestations bind a
subject digest to build provenance and can be verified with GitHub CLI. They are
a useful replacement for a prose “approved by” field, but not for tests, archive
allowlists, checksum verification, or protected credentials.
[GitHub artifact attestations](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations)

Cargo Dist also has an attestation filter for host-phase artifacts, but its
configuration reference notes that local-build-phase attestation is not
currently supported. In a custom unified workflow, direct `actions/attest` on
the already-built archive is clearer.
[Cargo Dist attestation configuration](https://axodotdev.github.io/cargo-dist/book/reference/config.html#github-attestations)

## Latency budget and observability

An achievable warm-cache critical path is approximately:

| Phase | Parallel/serial target | Budget |
| --- | --- | ---: |
| version/check | serial start | < 1 min |
| Linux quality + macOS/Linux package builds | parallel | 6–9 min |
| semantic/package tests + three current browser smokes | parallel after builds | 2–4 min |
| release upload/attestation + Homebrew validation/push | serial | 2–3 min |
| **workflow objective** | | **10–15 min** |

These are planning budgets informed by, but not proven by, the single v0.2.3
path: the existing four-workflow sequence took ~37m32s, while its actual browser
lanes were tens of seconds and duplicated builds were minutes. Add a final summary
that records queue time, job duration, cache hit status, archive sizes/digests,
semantic-manifest hashes, and publication URLs. Review p50/p95 over at least ten
releases before declaring the objective met. Do not enable Cargo Dist build
caching reflexively: its documentation says aggressive invalidation is expected
and even a no-op cache can cost substantial time.
[Cargo Dist `cache-builds`](https://axodotdev.github.io/cargo-dist/book/reference/config.html#cache-builds)

## Migration sequence

1. Add the semantic archive-manifest/check command and tests; make current CI run
   it against a locally built package.
2. Build the unified workflow in dry-run mode: no tag, release, or tap push.
   Prove that both exact artifacts flow through package/browser tests.
3. Add release creation against a prerelease/test version and verify the release
   contains the exact uploaded digests.
4. Make Homebrew validation fail closed, then enable the tap push without a
   required-reviewer rule.
5. Switch version-changing `main` merges to publication; retain manual dispatch
   as recovery only, requiring an explicit full SHA and the same graph.
6. Remove the candidate, browser, and proof workflows plus obsolete approval/
   compressed-byte fields from docs, schema, scripts, and tests.

Do not delete the old workflows before the dry-run and prerelease prove artifact
handoff and Cargo Dist hosting behavior. The generated Cargo Dist workflow may be
regenerated by `dist init`; document that the unified workflow is intentionally
custom or encode supported Cargo Dist custom-job settings so regeneration cannot
silently restore the old design.

## Risks and unresolved questions

- **Release cadence:** merge-to-release requires every release merge to carry a
  unique final Cargo version. Decide whether all merges release or only merges
  with a version change/release marker.
- **Cargo Dist host integration:** verify in a dry run which `dist host` manifest
  inputs are required when local artifacts were produced before the tag existed.
  Cargo Dist supports custom local jobs, but the exact 0.32.0 generated host seam
  must be tested before implementation.
- **Minimum-browser regression window:** scheduled minimum-version failures can
  occur after a release unless dashboard-sensitive PRs explicitly require those
  lanes; current Safari, Firefox, and Chromium still block each release.
- **Homebrew secret scope:** removing approval intentionally increases
  automation; compensate with branch restrictions, job-level permissions,
  concurrency, and a narrowly scoped token for only the tap repository.
- **Tap atomicity:** the GitHub Release can succeed before a tap push fails.
  Recovery must safely rerun only formula validation/push without rebuilding or
  replacing release assets.
- **Artifact retention:** workflow artifacts expire, but GitHub Release assets do
  not follow the workflow-artifact retention setting. Publication must copy the
  exact tested archives before expiry and retain semantic manifests with them.
- **Attestation availability:** GitHub documents plan/repository-visibility
  limits. Treat attestations as optional provenance, not a release prerequisite
  until repository eligibility and CLI verification are confirmed.
- **Time objective:** one path was inspected, not a timing distribution. Cold
  builds, queues, browser downloads, and Homebrew network work may exceed 15
  minutes; optimize from integrated step timing and judge the SLO by p50/p95.

## Conclusion

The largest simplification is not fewer assertions; it is removing duplicated
builds and human handoffs. Tracky can preserve its Rust gates, both supported
native targets, package/runtime checks, a real packaged browser gate, dependency
evidence, checksums, and Homebrew while producing each archive exactly once.
Publish that exact tested artifact, record its semantic contents and provenance,
and move exhaustive cross-browser compatibility to targeted/scheduled coverage.
That design makes a 10–15 minute merge-to-Homebrew path plausible and turns the
remaining uncertainty into measurable runner/build time rather than manual wait.
