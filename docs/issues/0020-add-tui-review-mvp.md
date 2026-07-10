# 0020 — Add a TUI review MVP

Labels: `ready-for-agent`

## Parent

- Product goal: Tracky should become comfortable for daily personal finance review while keeping CLI/JSON as the source of truth.

## What to build

Build a minimal terminal UI for reviewing imported candidates and canonical transactions. The TUI should wrap the same storage and review behavior as the CLI; it must not create a separate review model.

The first MVP should support import batch selection, candidate list, inspect details, accept/reject, categorize income/expense, mark transfers, and show report summaries.

## Acceptance criteria

- [ ] The TUI can open an existing SQLite database and list import batches/candidates.
- [ ] The TUI shows provenance/evidence summaries without exposing secrets or requiring raw PDF files.
- [ ] The TUI can run the same accept/reject/category/income/transfer actions available in the CLI.
- [ ] The TUI can show monthly report summaries from the canonical ledger.
- [ ] CLI/JSON commands remain usable and tested as the automation contract.
- [ ] Tests or documented manual verification cover the main TUI review path.

## Blocked by

- `0018-add-monthly-finance-reports.md`
- `0019-add-review-ergonomics-and-safe-batch-actions.md`
- `0022-close-generic-candidate-accept-review-bypass.md`
- `0023-suggest-cross-batch-transfer-actions.md`
