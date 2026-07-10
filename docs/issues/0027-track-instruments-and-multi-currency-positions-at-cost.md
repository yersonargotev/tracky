# 0027 — Track instruments and multi-currency positions at cost

Labels: `ready-for-agent`

## Parent

- Research: `docs/research/investment-tracking-model.md`

## What to build

Let users identify what an investment contribution acquired and see the resulting position at historical cost. The same model must support fiat currency, dollar-referenced digital assets, securities, fixed-income instruments, and an explicit generic fallback without treating unlike instruments as interchangeable.

Allocations may cross currencies and must preserve both the cash consideration and the acquired quantity. Tracky should derive an effective rate when possible and keep fees or other costs distinct from principal.

## Acceptance criteria

- [x] Users can create, list, and inspect stable investment instruments with type, denomination currency, provider or issuer, and optional provider identifier.
- [x] A confirmed contribution can be allocated wholly or partially to one or more instruments, with any unallocated remainder remaining visible.
- [x] Multi-currency allocations preserve cash amount/currency and acquired quantity/unit without silently converting or rounding either side.
- [x] USD fiat, USDC, COPW, and other assets remain distinct instruments unless the user explicitly identifies the acquired asset.
- [x] Tracky exposes positions by account and instrument with quantity, accumulated cost, cost currency, and latest contributing operation.
- [x] Fees linked to an acquisition can be represented separately from principal and cannot be counted twice in cost or expenses.
- [x] Allocation edits and corrections preserve audit history and provenance and reject over-allocation atomically.
- [x] Focused tests cover partial allocation, cross-currency acquisition, effective-rate derivation, asset distinction, position cost, and invalid reconciliation.

## Blocked by

- `0026-accept-investment-contributions-pending-allocation.md`

## Completion evidence

- `src/investments.rs` and `src/cli.rs` expose stable instrument CRUD, atomic single/multi-leg allocation, append-only replacement, contribution inspection, and historical-cost positions through `tracky.investments.v1`.
- `migrations/0001_review_first_schema.sql` adds instrument identities, allocation revisions/heads, exact decimal quantity text, durable fee-component links, and expense-side double-count protection; `docs/adr/0003-exact-investment-quantities-and-cost.md` records exactness limits.
- `src/storage.rs` derives pending/partial/full allocation status for transaction list/inspect while preserving 0026 reports and existing manual/review contracts.
- Synthetic integration coverage in `tests/investment_positions_cli.rs` and `tests/storage_migrations.rs` covers asset distinction, partial/full/multi-leg allocation, cross-currency exactness, effective-rate ratios, aggregation, fees, provenance, corrections, atomic failures, legacy migration, and expense/income/transfer/report compatibility.
- Final verification: `cargo fmt --check`, `cargo test`, and `cargo clippy --all-targets --all-features -- -D warnings`; Standards and Spec review against `33edad1` completed with no findings.
