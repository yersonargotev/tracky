# 0040 — Add typed atomic batch review actions

Labels: `ready-for-agent`

## Parent

- Safe batch actions: `docs/issues/0019-add-review-ergonomics-and-safe-batch-actions.md`
- Typed income and expense review: `docs/issues/0013-accept-income-with-source.md`,
  `docs/issues/0014-accept-expenses-with-categories.md`

## Problem

Reviewing a representative three-month import required hundreds of individual typed commands.
Existing batch actions cover duplicate rejection and transfer pairs but cannot atomically apply
explicit income sources, income kinds, expense categories, or balanced expense lines.

## What to build

Extend safe batch review so an operator can submit a mixed, explicit set of typed income, expense,
and transfer decisions, inspect a complete dry-run, and apply the exact set atomically through the
same validations as the individual commands.

## Acceptance criteria

- [x] Batch input supports typed income, categorized/split expense, and transfer-pair decisions
  without inferred sources or categories.
- [x] Dry-run executes every individual validation and reports deterministic per-action outcomes
  without writes.
- [x] Apply preflights the complete set and commits all decisions atomically or none.
- [x] Candidate reuse, incompatible actions, stale states, and invalid category/source/account
  references fail before mutation.
- [x] Audit, canonical transaction, split-line, provenance, and transfer-pair results match the
  equivalent individual commands.
- [x] Public tests cover mixed success, rollback, ordering, and replay refusal.

## Blocked by

- `0037-align-transfer-suspicion-and-resolution.md`

## Reconciliation evidence

- `src/storage.rs` prepares typed income and expense decisions through the same validation and row-application helpers used by `accept-income` and `accept-expense`; mixed apply performs that preflight inside one SQLite transaction before any mutation.
- `src/cli.rs` preserves the existing explicit `--action` contract and adds explicit income metadata plus repeatable expense-line triples without inferring sources or categories.
- `tests/batch_review_cli.rs` covers read-only mixed dry-run, deterministic input ordering, atomic rollback on invalid references, candidate reuse, split persistence, transfer persistence, and replay refusal with synthetic privacy-safe data.
