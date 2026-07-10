# 0017 — Add canonical transaction list and review-edit CLI

Labels: `ready-for-agent`

## Parent

- Product goal: users need to inspect and correct the canonical ledger after accepting candidates.

## What to build

Add commands to list, inspect, and safely update canonical transactions and their classification metadata. This gives the user a way to review what has already been accepted without editing SQLite directly.

The slice should not implement destructive deletion; corrections should preserve auditability.

## Acceptance criteria

- [x] A user can list canonical transactions by date range, account, category, income source, and type as stable JSON.
- [x] A user can inspect one canonical transaction with provenance, candidate link, category/income/transfer metadata, and split lines.
- [x] A user can update category, income source, description note, or split lines while preserving original imported evidence.
- [x] Updates are auditable and do not erase source provenance.
- [x] The command refuses updates that would unbalance split lines or convert transfers into income/expense without an explicit supported path.
- [x] Tests cover list filters, inspect output, safe metadata update, and invalid update rejection.

## Blocked by

- `0015-support-transaction-splits.md`
- `0016-add-manual-transactions-cli.md`
