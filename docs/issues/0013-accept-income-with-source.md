# 0013 — Accept income candidates with income sources

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`
- Domain decision: income sources are independent from the account where money lands.

## What to build

Allow accepted income candidates to carry an income source and income kind. This lets Tracky distinguish salary, freelance/client payment, sale, interest, reimbursement, or other income while still preserving the original account movement and provenance.

The first slice should support CLI/JSON review of one imported Nequi inflow at a time.

## Acceptance criteria

- [ ] A user can create/list income sources as stable JSON.
- [ ] `candidates accept` or a narrowly named review command can accept an inflow candidate with an income source and income kind.
- [ ] The accepted canonical transaction preserves the account movement, source document provenance, and selected income metadata.
- [ ] Positive Nequi inflows are not automatically classified as salary; the source/kind is explicit.
- [ ] Transfers from owned accounts are not accepted as income through this path.
- [ ] Tests cover salary-like recurring inflow, smaller non-salary income, and a blocked owned-account transfer attempt.

## Blocked by

- `0011-add-owned-account-registry.md`
