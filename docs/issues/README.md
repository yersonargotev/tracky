# Tracky local issues

These local Markdown issues are ordered for the review-first PDF import milestone.

1. `0001-define-review-first-import-contract.md` — contract first.
2. `0002-extract-pdf-inspection-core.md` — productize spike logic behind reusable seams.
3. `0003-add-pdf-inspect-cli-json.md` — read-only product CLI.
4. `0004-create-sqlite-review-first-schema.md` — storage schema.
5. `0005-add-import-pdf-persistence.md` — write candidates, not canonical transactions.
6. `0006-flag-possible-duplicate-candidates.md` — transaction-level duplicate signals.
7. `0007-add-candidate-review-cli.md` — accept/reject path to canonical transactions.
8. `0008-document-agent-workflow-and-next-slices.md` — usage docs and next slices.

Implementation should proceed in dependency order unless a later issue's blockers have already been satisfied by another change.
