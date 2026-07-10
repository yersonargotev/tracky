# 0028 — Track the complete CDT lifecycle

Labels: `ready-for-agent`

## Parent

- Research: `docs/research/investment-tracking-model.md`

## What to build

Support a CDT from constitution through renewal or redemption so the user can see active principal, contractual dates, investment income, and returned capital without counting the same money multiple times.

The workflow must distinguish new capital, capitalized returns, interest, withholding or deductions, and principal returned at maturity. It should remain useful without estimating daily accrued interest or providing tax advice.

## Acceptance criteria

- [ ] A CDT position records its institution or issuer, principal, currency, constitution date, maturity date, agreed rate, and payment or renewal terms when known.
- [ ] Constituting a CDT allocates confirmed investment capital and opens a position without creating a consumption expense.
- [ ] Renewing unchanged principal does not create a new external contribution; additional principal and capitalized interest remain distinguishable.
- [ ] Redemption separates returned principal, gross interest, withholding or other deductions, and net cash received.
- [ ] Returned principal is not reported as income, while confirmed interest is available as investment income.
- [ ] Active, matured, renewed, and redeemed CDT positions can be listed and inspected with complete provenance.
- [ ] Invalid dates, negative principal, inconsistent maturity events, and duplicate redemptions fail without partial mutation.
- [ ] Focused tests cover constitution, added capital, renewal, interest capitalization, withholding, redemption, and monthly contribution semantics.

## Blocked by

- `0027-track-instruments-and-multi-currency-positions-at-cost.md`
