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
9. `0009-add-user-facing-command-reference.md` — repository-level command reference for the review-first CLI path.
10. `0010-normalize-rappi-card-statement-semantics.md` — make RappiCard purchase/payment candidates safe to review.
11. `0011-add-owned-account-registry.md` — register user-owned accounts for resolution and transfer logic.
12. `0012-review-own-account-transfers.md` — review Nequi ↔ card payments as transfers, not expenses/income.
13. `0013-accept-income-with-source.md` — accept inflows with explicit income source/kind metadata.
14. `0014-accept-expenses-with-categories.md` — accept purchase candidates as categorized expenses.
15. `0015-support-transaction-splits.md` — split one canonical transaction across multiple category lines.
16. `0016-add-manual-transactions-cli.md` — add manual expenses, income, and transfers when PDFs miss data.
17. `0017-add-transaction-list-and-edit-review-cli.md` — inspect and safely correct canonical ledger metadata.
18. `0018-add-monthly-finance-reports.md` — report income, expenses, transfers, net, and category totals.
19. `0019-add-review-ergonomics-and-safe-batch-actions.md` — summarize batches, compare duplicates, and apply safe explicit actions.
20. `0020-add-tui-review-mvp.md` — wrap the CLI-backed review/report flows in a minimal TUI.
21. `0021-add-export-backup-and-import-safety.md` — backup, integrity-check, and export the local SQLite ledger.
22. `0022-close-generic-candidate-accept-review-bypass.md` — require typed review metadata instead of legacy generic promotion.
23. `0023-suggest-cross-batch-transfer-actions.md` — include valid transfer counterparts from other PDF import batches.
24. `0024-restore-strict-clippy-cleanliness.md` — clear strict Clippy failures before adding the TUI surface.
25. `0025-reconcile-completed-issue-metadata.md` — make historical issue completion state reliable.

Implementation should proceed in dependency order unless a later issue's blockers have already been satisfied by another change.

The agent-facing workflow for the implemented PDF inspect/import/review path is documented in `docs/agents/pdf-import-workflow.md`.
