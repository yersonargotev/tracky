# Release impact

Read this only when the user requests publication or the change alters user-visible behavior, packaging, dependencies, supported platforms, release workflows, or generated evidence.

## Publication in the same request

Before opening the PR:

1. Select the next unused SemVer version from GitHub releases and tags.
2. Keep `Cargo.toml`, `Cargo.lock`, the README version statement, CLI version expectations, and `CHANGELOG.md` aligned.
3. Describe only shipped behavior in the changelog.
4. If `Cargo.lock` changes, regenerate `evidence/dashboard/dependency-inventory.json` with the pinned repository command.
5. Regenerate `THIRD-PARTY-NOTICES` when the dependency graph or package version affects it.
6. Run the generators again in check mode or compare the second output; accepted generated files must be stable.

Complete when the merged commit can be tagged with the selected version without another source change.

## User-visible change without immediate publication

Document the behavior at its current user-facing source and identify the future release-note obligation in the PR. Keep the package version unchanged unless the user asked to cut a release.

## Packaging or release-pipeline change

Run the release workflow's pull-request path and every release-evidence unit
test. Tracky's production workflow is repository-owned rather than generated:
keep Cargo Dist CI generation disabled, retain explicit GitHub hosting, and
verify that no tag-triggered fresh-build workflow is introduced.
