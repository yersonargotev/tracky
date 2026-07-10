# 0029 — Track the complete brokerage investment lifecycle

Labels: `ready-for-agent`

## Parent

- Research: `docs/research/investment-tracking-model.md`

## What to build

Model a brokerage such as trii as owned investment cash plus positions in securities. A deposit, a purchase made with that deposit, and a later withdrawal must form one reconcilable lifecycle rather than three unrelated income or expense events.

Users should be able to record purchases, sales, dividends, fees, retentions, and withdrawals while seeing cash and security positions and avoiding double-counted capital contributions.

## Acceptance criteria

- [x] A brokerage account can expose owned cash separately from its security positions.
- [x] Cash entering the brokerage from a daily-use account counts once as external investment capital and remains available until allocated or withdrawn.
- [x] A security purchase reduces brokerage cash, increases instrument quantity and cost, and does not count the same deposit as new capital again.
- [x] A sale reduces quantity, increases brokerage cash, preserves proceeds and fees, and exposes the realized result without treating all proceeds as income.
- [x] Dividends, related charges, and withholding remain separately inspectable; reinvested dividends are not external capital.
- [x] Withdrawing brokerage cash to another owned account does not create income and updates the contribution/withdrawal view consistently.
- [x] Position quantities and cash cannot become inconsistent through duplicate, excessive, or out-of-order lifecycle actions.
- [x] Focused tests cover deposit, buy, sell, dividend, reinvestment, fees, withholding, withdrawal, and double-count prevention.

## Blocked by

- `0027-track-instruments-and-multi-currency-positions-at-cost.md`


## Completion evidence

- `src/brokerage.rs` and `src/cli.rs` expose `brokerages open/deposit/buy/sell/dividend/withdraw/replace-operation/list/inspect` through `tracky.brokerage.v1`; cash and positions replay exclusively from active operation heads.
- `migrations/0001_review_first_schema.sql` persists brokerage accounts and append-only operation revisions, and extends the primary-keyed allocation-consumption claim across brokerage deposits and CDT actions.
- Exact quantities use canonical decimals with at most nine fractional digits; monetary components use `i64` minor units. ADR 0003 documents deterministic moving weighted-average disposal without tax claims.
- Synthetic isolated coverage in `tests/brokerage_lifecycle_cli.rs` verifies the lifecycle, exact realized result, deductions, double-count prevention, insufficient balances and atomic failures.
- Final verification uses `cargo fmt --check`, `cargo test`, and strict Clippy; review fixed point is `8dc51ff`.
