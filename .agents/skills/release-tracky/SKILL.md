---
name: release-tracky
description: Release Tracky through the serialized version-changing main-merge workflow, retained same-run assets, GitHub Release, and Homebrew. Use when publishing the next Tracky version or recovering a failed Tracky release workflow.
---

# Release Tracky

Treat **exact** as the invariant: one merged commit and lockfile bind the native builds, current-browser evidence, tag, release assets, and Homebrew formula.

An explicit request to publish authorizes the version-changing merge and routine workflow recovery for that version. The workflow creates the stable tag, GitHub Release, and Homebrew update automatically. Stop for a missing version or scope, or evidence that would require changing the merged commit.

Before publishing or recovering, read `docs/dashboard-evidence.md` and the current commit's `.github/workflows/release.yml` and `.github/workflows/homebrew.yml`. They are the source of truth for triggers, inputs, environments, and retained artifacts.

## 1. Prepare the version-changing merge

1. Require clean local `main`, synchronized with `origin/main`.
2. Confirm the package version is unused, matches the intended tag, and is aligned across `Cargo.toml`, `Cargo.lock`, README, changelog, and version tests.
3. Land version and release-note alignment through `land-tracky-change`; do not create or push the tag manually.
4. Run the complete repository suite, merge to `main`, and record the full merged SHA, `Cargo.lock` SHA-256, intended tag, PR, and CI URL.

An ordinary merge with an unchanged parsed package version must not publish.

Complete when the version-changing commit is on `main`, no source edit remains, and the production release run has started for that exact SHA.

## 2. Monitor the single production DAG

1. Confirm the run is in the constant release concurrency group and is queued, not canceled, behind any active release.
2. Require quality plus one Cargo Dist build for each native target; downstream runtime jobs must reuse those retained bundles.
3. Require current Safari, Firefox, and Chromium over the extracted retained packages. The separate six-lane compatibility workflow is not part of the normal release path.
4. Confirm the pre-publication evidence binds source/version/lock identity, cache state, artifact IDs and digests, exact transport checksums, tool/browser versions, commands, gates, URLs, and timings.

Complete when quality, both native targets, native runtime, and all three current-browser lanes pass without a downstream rebuild.

## 3. Verify automatic publication

1. Confirm publication downloads assets retained by the same run and never invokes Cargo Dist.
2. Confirm the stable tag and public GitHub Release target the exact merged SHA.
3. Verify every release asset matches its retained transport SHA-256; never rebind or waive compressed-byte identity.
4. Confirm automatic Homebrew validation passes on macOS ARM and Linux x86-64 before the tap update reads its environment token.
5. Confirm the terminal summary includes queue/job timings, cache state, artifact identities, GitHub publication URLs, and the Homebrew URL. Assess p50/p95 timing only after at least ten completed production releases.

Complete when the GitHub Release and Homebrew formula resolve to the one tested archive per native target. Workflow artifacts and evidence remain recoverable for 14 days.

## 4. Recover without changing identity

1. Diagnose the failed job and preserve the exact merged SHA, tag, and matching remote state.
2. Prefer `gh run rerun <run-id> --failed`; successful jobs and retained same-run artifacts are reused instead of rebuilt.
3. If a new run is required, dispatch **Release** with the explicit 40-character lowercase SHA from `main`. The workflow derives version and lock identity and executes the same serialized, non-canceling DAG.
4. Treat missing 14-day artifacts or any tag, release, asset, or checksum conflict as a blocker. Do not replace an archive, rebind evidence, or force-update a tag.

Complete when reconciliation preserves matching state, fills only missing matching state, and the original production DAG succeeds.

## 5. Verify publication and clean up

1. Confirm the release is public, non-draft, `prerelease=false`, and targets the peeled tag commit.
2. Verify the expected asset set, both exact native archive checksums, and native `tracky --version`.
3. Confirm the Homebrew tap formula version, URLs, and SHA-256 values match the release archives. Use downloaded files or temporary prefixes; preserve the operator's existing installation and configuration.
4. Confirm the originating issue is closed, local `main` remains clean and synchronized, and all temporary dashboards, containers, audio servers, listeners, and Docker Desktop processes started for validation are stopped.
5. Report PR, issue, exact commit, release run, retained evidence, release, assets, and Homebrew commit links.

Complete when every published surface resolves to the exact release and no validation process or repository change is left behind.
