# Tracky local issues

These Markdown files are the local tracker for the review-first PDF import milestone. Issue numbers preserve dependency order; the sections below describe current state.

## Completed slices

A slice is **completed** when every acceptance criterion in its issue file is checked (`[x]`) and the implementation/evidence is present in the repository history. This tracker has no `done` label: checked criteria plus the evidence section in the issue file are the completion record. The `ready-for-agent` label remains visible on completed files because it means “specified and ready for an implementation agent,” not “unfinished.”

1. `0001-define-review-first-import-contract.md` — contract first.
2. `0002-extract-pdf-inspection-core.md` — productize spike logic behind reusable seams.
3. `0003-add-pdf-inspect-cli-json.md` — read-only product CLI.
4. `0004-create-sqlite-review-first-schema.md` — storage schema.
5. `0005-add-import-pdf-persistence.md` — write candidates, not canonical transactions.
6. `0006-flag-possible-duplicate-candidates.md` — transaction-level duplicate signals.
7. `0007-add-candidate-review-cli.md` — accept/reject path to canonical transactions.
8. `0008-document-agent-workflow-and-next-slices.md` — usage docs and next slices.
9. `0009-add-user-facing-command-reference.md` — repository-level command reference.
10. `0010-normalize-rappi-card-statement-semantics.md` — safe RappiCard semantics.
11. `0011-add-owned-account-registry.md` — register user-owned accounts.
12. `0012-review-own-account-transfers.md` — review own-account transfers/card payments.
13. `0013-accept-income-with-source.md` — typed income review.
14. `0014-accept-expenses-with-categories.md` — typed expense review.
15. `0015-support-transaction-splits.md` — split transaction lines.
16. `0016-add-manual-transactions-cli.md` — manual ledger entries.
17. `0017-add-transaction-list-and-edit-review-cli.md` — ledger inspection/editing.
18. `0018-add-monthly-finance-reports.md` — monthly finance reports.
19. `0019-add-review-ergonomics-and-safe-batch-actions.md` — safe batch review actions.
20. `0022-close-generic-candidate-accept-review-bypass.md` — typed review guardrails.
21. `0023-suggest-cross-batch-transfer-actions.md` — cross-batch transfer suggestions.
22. `0024-restore-strict-clippy-cleanliness.md` — strict Clippy cleanliness.
23. `0025-reconcile-completed-issue-metadata.md` — tracker metadata reconciliation.

## Pending queue

These issues retain unchecked criteria and are not represented as completed:

1. `0020-add-tui-review-mvp.md` — minimal TUI review surface; intentionally not started in this slice.
2. `0021-add-export-backup-and-import-safety.md` — backup, integrity, and export commands.
3. `0026-accept-investment-contributions-pending-allocation.md` — typed investment contribution review with explicit pending allocation.
4. `0027-track-instruments-and-multi-currency-positions-at-cost.md` — instrument registry, allocation, and historical-cost positions.
5. `0028-track-complete-cdt-lifecycle.md` — CDT constitution, renewal, income, withholding, and redemption.
6. `0029-track-complete-brokerage-investment-lifecycle.md` — brokerage cash, securities, income, costs, and withdrawals.
7. `0030-reconcile-investment-positions-and-dated-valuations.md` — dated provider snapshots and reviewed reconciliation.
8. `0031-import-and-reconcile-investment-provider-documents.md` — review-first trii, Wenia, and CDT document adapters.
9. `0032-add-consolidated-monthly-investment-reports.md` — monthly investment flows, positions, and valuation freshness.
10. `0033-extend-export-backup-and-integrity-for-investments.md` — operational safety for investment data.
11. `0034-add-investment-review-and-reporting-to-tui.md` — investment workflow in the shared TUI model.

Implementation should proceed from the dependency frontier: any issue whose listed blockers are complete may start. The investment-tracking expansion is grounded in `docs/research/investment-tracking-model.md`; provider adapters additionally require representative, user-authorized artifacts or privacy-safe fixtures derived from them.

The agent-facing workflow for the implemented PDF inspect/import/review path is documented in `docs/agents/pdf-import-workflow.md`.
