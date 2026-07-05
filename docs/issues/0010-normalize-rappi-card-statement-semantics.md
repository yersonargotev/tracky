# 0010 — Normalize RappiCard statement semantics for review

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`
- Follow-up from real asset review on 2026-07-05.

## What to build

Make RappiCard candidates safe to review as credit-card activity. Purchases, subscriptions, restaurants, supermarkets, fees, and interest from a Rappi credit-card statement should be represented as expense-like card charges, while statement payments such as PSE payments should be represented as liability-reducing payment candidates, not ordinary expenses.

This slice should preserve review-first behavior: the parser may improve signs, direction hints, or semantic hints, but `import pdf` must still create candidates only. No reports, categories, or transfer automation are required here.

## Acceptance criteria

- [x] RappiCard purchase rows are emitted with a review-safe semantic hint that distinguishes card charges from income.
- [x] RappiCard payment rows such as `PAGOS POR PSE` are emitted with a review-safe semantic hint that distinguishes card payments from purchases.
- [x] Existing Nequi amount/balance behavior remains unchanged.
- [x] JSON output remains backward-compatible where possible and documents any new field or enum used for card semantics.
- [x] Tests cover at least one Rappi purchase row, one Rappi PSE payment row, and one Rappi installment/recurring row using redacted row fixtures only.
- [x] `import pdf` still creates no canonical transactions.

## Blocked by

- `0007-add-candidate-review-cli.md`
