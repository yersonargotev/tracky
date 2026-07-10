# 0026 — Accept investment contributions pending allocation

Labels: `ready-for-agent`

## Parent

- Research: `docs/research/investment-tracking-model.md`

## What to build

Add the first complete investment-review path: a user can confirm that a candidate or manual outflow is capital destined for investment even when the exact instrument or acquired quantity is not known yet.

The confirmed principal must remain traceable, appear as an investment contribution, and stay out of consumption expenses and income. Unknown allocation is an explicit review state, not a reason to guess an instrument or degrade the movement to an expense.

## Acceptance criteria

- [x] A typed CLI/JSON action can confirm a reviewable outflow as an investment contribution without using the generic candidate-accept path.
- [x] A manual equivalent can record an investment contribution when no imported candidate exists.
- [x] A contribution can remain pending allocation while retaining source account, date, amount, currency, description, and provenance.
- [x] Confirmed investment principal is excluded from expense and income totals and is exposed separately in date-range reporting.
- [x] Existing expense, income, and own-account-transfer behavior and JSON contracts remain compatible.
- [x] Invalid or incompatible actions fail atomically without changing candidate or canonical state.
- [x] Focused CLI/JSON and storage tests cover candidate confirmation, manual entry, pending allocation, reporting, provenance, and non-mutation failures.

## Blocked by

- None — can start immediately.

## Completion evidence

- `src/cli.rs` and `src/storage.rs` expose `candidates accept-investment` and `transactions add-investment` through the existing stable review/manual JSON envelopes.
- `migrations/0001_review_first_schema.sql` persists `investment_allocation_status = pending_allocation`; canonical rows retain the signed source-account movement and imported or manual audit link.
- `reports summary` adds deterministic `investment_contribution_totals[]` by currency while existing income, expense, category, net-cash-flow, and transfer calculations remain unchanged.
- Synthetic integration coverage lives in `tests/candidate_review_cli.rs`, `tests/manual_transactions_cli.rs`, `tests/finance_report_cli.rs`, and `tests/storage_migrations.rs`, including incompatible-action non-mutation and legacy-schema migration.
- Final verification: `cargo fmt --check`, `cargo test`, and `cargo clippy --all-targets --all-features -- -D warnings`.
