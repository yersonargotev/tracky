# Cargo Dist archive recovery

Use this branch only when `Verify approved packaged dashboard evidence` reports an archive byte-count or SHA-256 mismatch.

First confirm `Cargo.toml` still pins Cargo Dist 0.32.0. That version preserved build-time mtimes in Tracky's `.tar.xz` archives during v0.2.1, so two builds of one commit had different compressed bytes while containing the same executable and source-controlled files. With another version, treat this as a hypothesis and prove the actual difference before applying the branch.

## Prove equivalence first

Download the successful `build-local-artifacts` outputs from the failed release run and compare them with the approved candidates.

For every target, require:

- identical executable SHA-256 and byte count;
- the exact allowed archive file set;
- source-identical README, license, and third-party notices;
- the expected executable mode and architecture;
- differences attributable only to archive metadata or compression.

An executable or content difference blocks publication. Diagnose or rebuild from a new commit; do not rebind the proof.

## Rebind exact release archives

When equivalence passes:

1. Copy the canonical target fragments to a new temporary proof directory.
2. Replace only `artifact.bytes`, `artifact.sha256`, `size.archive_bytes`, and `size.archive_sha256` with measurements from the retained release-run archives.
3. Preserve executable hashes, runtime measurements, browser evidence, accessibility evidence, commands, commit, and lockfile.
4. Assemble a new manifest with `scripts/dashboard_candidate_manifest.py`.
5. Run both:

   ```sh
   python3 scripts/dashboard_evidence.py validate --release \
     --commit "$COMMIT" --lockfile-sha256 "$LOCKFILE_SHA256" \
     dashboard-verification.json
   python3 scripts/dashboard_evidence.py verify-artifacts \
     dashboard-verification.json --artifacts release-artifacts
   ```

6. Host and hash-pin the corrected manifest, dispatch **Approve dashboard release proof**, and approve `dashboard-release`.
7. Use `gh run rerun "$RELEASE_RUN_ID" --failed`. This preserves the successful upstream build artifacts. A full rerun would rebuild new archives and invalidate the rebound proof.

Complete when attempt 2 verifies those retained archives before hosting or Homebrew publication begins.
