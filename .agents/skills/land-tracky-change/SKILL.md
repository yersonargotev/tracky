---
name: land-tracky-change
description: Land Tracky changes through implementation, fixed-point review, an intentional commit, PR CI, and merge. Use when a Tracky issue or approved specification should be implemented, opened as a pull request, or merged; include release metadata when publication is part of the request.
---

# Land Tracky Change

Treat **land** as the completion bar: the requested behavior is implemented, independently reviewed against both repository standards and its specification, merged with green CI, and synchronized locally.

An explicit request to create or merge the PR authorizes the routine GitHub mutations in that scope. Ask only when the issue/version is missing, the scope would broaden, or a maintainer override was not authorized.

## 1. Bind the change

1. Confirm this is `yersonargotev/tracky` and read the active repository instructions.
2. Read the originating issue/specification and its linked decisions.
3. Record the fixed point, acceptance criteria, privacy boundaries, migration requirements, and whether the user also requested a release.
4. Inspect the current branch, worktree, remotes, and existing PR state.

Complete when every requested behavior has a checkable acceptance criterion and unrelated work is excluded.

## 2. Implement the smallest complete slice

1. Use CodeGraph before code exploration when `.codegraph/` exists; use targeted reads for manifests, documentation, configuration, and scripts.
2. Use TDD at the public CLI, JSON, database, or parser seam that proves the behavior.
3. Preserve Tracky's local-first and review-first boundaries. Use synthetic or redacted fixtures and sandboxed databases; keep real financial files, passwords, and user configuration outside the repository.
4. Keep migrations forward-compatible and prove existing records survive.
5. Update user documentation for observable behavior.

If publication is requested or the change affects user-visible behavior, packaging, dependencies, supported platforms, release workflows, generated evidence, or release metadata, read [release-impact.md](references/release-impact.md) before finishing this step.

Complete when every acceptance criterion has a focused passing test and every changed line traces to the requested scope.

## 3. Verify and review

1. Run focused checks while iterating.
2. Run every current command in `.github/PULL_REQUEST_TEMPLATE.md` and every locally reproducible gate in `.github/workflows/ci.yml`.
3. Run repository-specific evidence, inventory, license, or generated-file checks affected by the diff. Regenerate accepted artifacts and prove a second generation is clean.
4. Review the diff from the fixed point along separate Standards and Spec axes. Use the code-review skill when available; otherwise inspect documented repository standards and the originating issue independently, reporting every actionable finding under its axis.
5. Resolve every actionable finding, rerun affected checks, and repeat the review until both axes report zero pending findings.

Complete when the full required suite passes, generated artifacts are stable, and both reviews are clean.

## 4. Commit and open the PR

1. Inspect the complete diff and status.
2. Create one intentional commit unless the issue requires independently revertible slices.
3. Push the feature branch and open a PR that links the issue, summarizes behavior, and lists the checks actually run.
4. Monitor every required check. Diagnose a failure from its logs, make only the root-cause fix, then repeat Step 3 for the new diff.

Complete when the PR head is reviewed, its required checks are green, and no review thread or local change is pending.

## 5. Merge and synchronize

1. Confirm the PR still targets the intended base and the merge result contains only approved commits.
2. Use the repository's accepted merge method. If owner-only self-review blocks an otherwise green PR, use an explicitly authorized maintainer override and record it.
3. Confirm the issue closed when the PR claims it.
4. Synchronize local `main` with `origin/main`; require a clean worktree and identical SHA.

Complete when GitHub and local `main` agree on the merged commit and the user receives PR, commit, CI, and issue links.

If the user requested publication, continue with the `release-tracky` skill from this exact merged SHA.
