# 0030 — Reconcile investment positions and dated valuations

Labels: `ready-for-agent`

## Parent

- Research: `docs/research/investment-tracking-model.md`

## What to build

Allow Tracky to compare event-derived positions with a dated observation from an investment provider. A provider snapshot should reveal missing history or stale valuation without silently rewriting confirmed operations.

The user should be able to see what was observed, when it was observed, how it differs from Tracky's derived position, and whether an explicit reviewed adjustment is still needed.

## Acceptance criteria

- [x] Users can record and inspect a dated provider snapshot containing positions, quantities, currencies, observed values, source, and observation time when available.
- [x] Reconciliation compares snapshots with derived cash and instrument positions and reports quantity, cost, and value differences deterministically.
- [x] Reading or comparing a snapshot never mutates confirmed operations or positions.
- [x] An explicit reviewed adjustment can reconcile incomplete history while preserving the original snapshot, difference, reason, and provenance.
- [x] Position values always identify valuation currency, source, observation date, and freshness instead of presenting an undated “current value.”
- [x] Missing prices remain representable; Tracky can still report quantity and historical cost without inventing a market value.
- [x] Focused tests cover matching snapshots, discrepancies, partial history, stale values, explicit adjustments, and read-only non-mutation.

## Blocked by

- `0027-track-instruments-and-multi-currency-positions-at-cost.md`


## Completion evidence

- `src/reconciliation.rs` and `src/cli.rs` expose stable snapshot and reviewed-adjustment actions through `tracky.investment-reconciliation.v1`.
- `migrations/0001_review_first_schema.sql` stores immutable observations, unique provider references and positions, preserved baselines, and append-only adjustment heads.
- `docs/contracts/investment-reconciliation-json.md` documents governing dates, exact representations, statuses, comparable values, and seven-day freshness.
- Synthetic isolated coverage in `tests/investment_reconciliation_cli.rs` verifies observations, missing history, freshness, read-only comparison, duplicates, adjustment reconciliation, original differences, and correction history.
- Final verification uses `cargo fmt --check`, `cargo test`, and strict Clippy; review fixed point is `8c1afc0`.
