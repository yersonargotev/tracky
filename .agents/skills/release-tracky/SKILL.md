---
name: release-tracky
description: Release Tracky from a merged exact commit through native candidates, browser and manual accessibility evidence, approved proof, tag publication, GitHub Release, and Homebrew. Use when publishing a Tracky version, creating a release tag, or recovering a failed Tracky release workflow.
---

# Release Tracky

Treat **exact** as the invariant: one final commit and lockfile bind the candidates, browsers, human sign-off, proof, tag, release assets, and package-manager formula.

An explicit request to publish authorizes routine workflow dispatches, protected-environment approvals, tag creation, GitHub Release publication, and Homebrew publication for that version. Stop for a missing version or scope, genuine human-only test results, or evidence that would require changing the frozen commit.

Before dispatching any workflow, read `docs/dashboard-evidence.md` and the current commit's `.github/workflows/dashboard-release-candidate.yml`, `dashboard-release-browsers.yml`, `dashboard-release-accessibility.yml`, `dashboard-release-proof.yml`, and `release.yml`. They are the source of truth for names, inputs, protected environments, and retained artifacts.

## 1. Freeze the release commit

1. Require clean local `main`, synchronized with `origin/main`.
2. Confirm the package version is unused, matches the intended tag, and is aligned across `Cargo.toml`, `Cargo.lock`, README, changelog, and version tests.
3. Run the complete repository suite and wait for CI on the merged SHA.
4. Record the full commit, `Cargo.lock` SHA-256, intended tag, and CI URL.

If version or release-note alignment requires a source edit, return to `land-tracky-change` for a release-preparation PR and restart from its merged SHA.

Complete when the release commit is immutable in practice: every later input must name that exact SHA, and no source edit remains.

## 2. Produce retained machine evidence

1. Dispatch **Build dashboard release candidate** with the full SHA and wait for every native target.
2. Dispatch **Test dashboard release browsers** with the same SHA and wait for all six required lanes.
3. Download the canonical candidate fragments and `browsers.json`; verify their commit and lockfile before using them.
4. Record run IDs, URLs, artifact names, archive hashes, and tool versions outside the source tree.

Complete when both canonical artifacts cover every supported target and browser lane with passing status and the exact bindings.

## 3. Obtain human-only accessibility evidence

1. Present the exact candidate archives, hashes, operating environments, and full checklist to the human tester.
2. Wait for explicit results for every Safari/VoiceOver and Firefox/Orca row. Automation is supporting evidence, not a human attestation.
3. Build the signed submission from the tester's actual findings, validate it locally, host it at a hash-pinned HTTPS URL, and dispatch **Retain dashboard manual accessibility evidence**.
4. Approve the `dashboard-release` environment only after checking dispatcher identity, submission hash, candidate provenance, and every signed row.
5. Download the canonical `manual-accessibility.json`.

Complete when the protected retention run passes and its canonical artifact remains bound to the frozen SHA and lockfile.

## 4. Assemble and approve the proof

1. Create the eleven non-manual gate records from retained GitHub Actions URLs.
2. Compute a fresh package count and embedded asset byte total from the accepted source tree.
3. Assemble `dashboard-verification.json` with `scripts/dashboard_candidate_manifest.py`.
4. Run release validation, render Markdown, and verify the hosted copy byte-for-byte.
5. Dispatch **Approve dashboard release proof** on the frozen commit, approve the protected environment, and require the retained proof artifact.

Complete when a successful proof run retains JSON and Markdown named for the frozen commit.

## 5. Tag and publish

1. Confirm the intended local and remote tag do not exist.
2. Create an annotated tag on the frozen SHA and push only that tag.
3. Monitor the **Release** workflow. Approve `homebrew` only after packaged-proof verification and GitHub Release hosting pass.
4. If packaged verification reports an archive size/hash mismatch, read [archive-recovery.md](references/archive-recovery.md) and follow it before any retry.
5. Diagnose any other failure from the failed job logs; preserve the tag and exact SHA unless evidence proves the tag itself is wrong.

Complete when the release workflow, including Homebrew and announcement, succeeds.

## 6. Verify publication and clean up

1. Confirm the release is public, non-draft, non-prerelease unless intended, and targets the peeled tag commit.
2. Verify the expected asset set, proof digest, both native archive checksums, and native `tracky --version`.
3. Confirm the Homebrew tap formula version, URLs, and SHA-256 values match the release archives. Use downloaded files or temporary prefixes; preserve the operator's existing installation and configuration.
4. Confirm the originating issue is closed, local `main` remains clean and synchronized, and all temporary dashboards, containers, audio servers, listeners, and Docker Desktop processes started for validation are stopped.
5. Report PR, issue, commit, proof, release run, release, and Homebrew commit links.

Complete when every published surface resolves to the exact release and no validation process or repository change is left behind.
