# 0028 — Track the complete CDT lifecycle

Labels: `ready-for-agent`

## Parent

- Research: `docs/research/investment-tracking-model.md`

## What to build

Support a CDT from constitution through renewal or redemption so the user can see active principal, contractual dates, investment income, and returned capital without counting the same money multiple times.

The workflow must distinguish new capital, capitalized returns, interest, withholding or deductions, and principal returned at maturity. It should remain useful without estimating daily accrued interest or providing tax advice.

## Acceptance criteria

- [x] A CDT position records its institution or issuer, principal, currency, constitution date, maturity date, agreed rate, and payment or renewal terms when known.
- [x] Constituting a CDT allocates confirmed investment capital and opens a position without creating a consumption expense.
- [x] Renewing unchanged principal does not create a new external contribution; additional principal and capitalized interest remain distinguishable.
- [x] Redemption separates returned principal, gross interest, withholding or other deductions, and net cash received.
- [x] Returned principal is not reported as income, while confirmed interest is available as investment income.
- [x] Active, matured, renewed, and redeemed CDT positions can be listed and inspected with complete provenance.
- [x] Invalid dates, negative principal, inconsistent maturity events, and duplicate redemptions fail without partial mutation.
- [x] Focused tests cover constitution, added capital, renewal, interest capitalization, withholding, redemption, and monthly contribution semantics.

## Blocked by

- `0027-track-instruments-and-multi-currency-positions-at-cost.md`

## Completion evidence

- `src/cdt.rs` and `src/cli.rs` expose `cdts constitute/list/inspect/renew/redeem/replace-operation` through `tracky.cdts.v1`, deriving current principal and `active`/`matured`/`renewed`/`redeemed` state from active append-only operations.
- `migrations/0001_review_first_schema.sql` persists CDT positions, operation revisions/heads, exact contractual components, durable deduction links, and a primary-keyed allocation-consumption claim written atomically with constitution or added-capital renewal.
- `src/investments.rs` freezes consumed allocation economics; `src/storage.rs` symmetrically prevents a CDT deduction identity from later becoming an independent expense. `docs/adr/0003-exact-investment-quantities-and-cost.md` records `i64` minor-unit and canonical decimal-rate limits.
- Synthetic CLI/storage coverage in `tests/cdt_lifecycle_cli.rs` and `tests/storage_migrations.rs` covers complete/partial terms, exact rates, unchanged and added-capital renewal, reinvested/cash interest, partial/total redemption, withholding/deductions, report non-duplication, states, provenance, corrections, durable identities, duplicate events, and atomic failures.
- Final verification: `cargo fmt --check`, `cargo test`, and `cargo clippy --all-targets --all-features -- -D warnings`; repeated Standards and Spec review against `a8a36e1` completed with no findings.
