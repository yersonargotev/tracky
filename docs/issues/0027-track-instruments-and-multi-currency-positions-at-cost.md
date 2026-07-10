# 0027 — Track instruments and multi-currency positions at cost

Labels: `ready-for-agent`

## Parent

- Research: `docs/research/investment-tracking-model.md`

## What to build

Let users identify what an investment contribution acquired and see the resulting position at historical cost. The same model must support fiat currency, dollar-referenced digital assets, securities, fixed-income instruments, and an explicit generic fallback without treating unlike instruments as interchangeable.

Allocations may cross currencies and must preserve both the cash consideration and the acquired quantity. Tracky should derive an effective rate when possible and keep fees or other costs distinct from principal.

## Acceptance criteria

- [ ] Users can create, list, and inspect stable investment instruments with type, denomination currency, provider or issuer, and optional provider identifier.
- [ ] A confirmed contribution can be allocated wholly or partially to one or more instruments, with any unallocated remainder remaining visible.
- [ ] Multi-currency allocations preserve cash amount/currency and acquired quantity/unit without silently converting or rounding either side.
- [ ] USD fiat, USDC, COPW, and other assets remain distinct instruments unless the user explicitly identifies the acquired asset.
- [ ] Tracky exposes positions by account and instrument with quantity, accumulated cost, cost currency, and latest contributing operation.
- [ ] Fees linked to an acquisition can be represented separately from principal and cannot be counted twice in cost or expenses.
- [ ] Allocation edits and corrections preserve audit history and provenance and reject over-allocation atomically.
- [ ] Focused tests cover partial allocation, cross-currency acquisition, effective-rate derivation, asset distinction, position cost, and invalid reconciliation.

## Blocked by

- `0026-accept-investment-contributions-pending-allocation.md`
