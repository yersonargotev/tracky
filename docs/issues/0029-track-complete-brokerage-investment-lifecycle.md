# 0029 — Track the complete brokerage investment lifecycle

Labels: `ready-for-agent`

## Parent

- Research: `docs/research/investment-tracking-model.md`

## What to build

Model a brokerage such as trii as owned investment cash plus positions in securities. A deposit, a purchase made with that deposit, and a later withdrawal must form one reconcilable lifecycle rather than three unrelated income or expense events.

Users should be able to record purchases, sales, dividends, fees, retentions, and withdrawals while seeing cash and security positions and avoiding double-counted capital contributions.

## Acceptance criteria

- [ ] A brokerage account can expose owned cash separately from its security positions.
- [ ] Cash entering the brokerage from a daily-use account counts once as external investment capital and remains available until allocated or withdrawn.
- [ ] A security purchase reduces brokerage cash, increases instrument quantity and cost, and does not count the same deposit as new capital again.
- [ ] A sale reduces quantity, increases brokerage cash, preserves proceeds and fees, and exposes the realized result without treating all proceeds as income.
- [ ] Dividends, related charges, and withholding remain separately inspectable; reinvested dividends are not external capital.
- [ ] Withdrawing brokerage cash to another owned account does not create income and updates the contribution/withdrawal view consistently.
- [ ] Position quantities and cash cannot become inconsistent through duplicate, excessive, or out-of-order lifecycle actions.
- [ ] Focused tests cover deposit, buy, sell, dividend, reinvestment, fees, withholding, withdrawal, and double-count prevention.

## Blocked by

- `0027-track-instruments-and-multi-currency-positions-at-cost.md`
