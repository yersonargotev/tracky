# 0012 — Review own-account transfers and card payments

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`
- Domain decision: card payments from Nequi to RappiCard or Nu are transfers, not expenses or income.

## What to build

Add a review action for movements between owned accounts. The first target is a Nequi outflow such as `COMPRA PSE EN BANCO` matched with a RappiCard `PAGOS POR PSE` card-payment row for the same amount/date. Reviewing the pair should create a canonical transfer/payment link, not two independent expenses.

This should be explicit and auditable: Tracky may suggest candidate pairs, but the user or agent confirms the transfer.

## Acceptance criteria

- [x] Tracky can list likely own-account transfer/card-payment pairs as JSON using date, amount, owned account metadata, and semantic hints.
- [x] A review command can accept a pair as a transfer/card payment.
- [x] Accepted transfer pairs preserve provenance for both source candidates.
- [x] Accepted transfer pairs do not inflate income or expense totals.
- [x] The command refuses to pair candidates that are already accepted/rejected or belong to unresolved non-owned accounts.
- [x] Tests cover a Nequi-to-RappiCard PSE payment pair and a non-match case.

## Blocked by

- `0011-add-owned-account-registry.md`
