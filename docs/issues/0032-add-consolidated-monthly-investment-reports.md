# 0032 — Add consolidated monthly investment reports

Labels: `ready-for-agent`

## Parent

- Research: `docs/research/investment-tracking-model.md`

## What to build

Add a consolidated investment report that answers how much new capital was invested during a period, how it was allocated, what returned, what income or costs occurred, and what positions were known at period end.

The report must complement rather than redefine the existing income/expense summary. It should expose incomplete allocation and stale valuation explicitly and never sum unlike currencies without a sourced conversion.

## Acceptance criteria

- [ ] Date-range reporting exposes external capital contributed, gross acquisitions, reinvestment, capital withdrawn, and net external contribution separately.
- [ ] Depositing into a brokerage and then buying a security with the same cash cannot double-count invested capital.
- [ ] Interest, dividends, realized results, fees, commissions, and withholding are separately visible and reconcile to their underlying events.
- [ ] Closing positions show quantity, historical cost, cost currency, latest observed value, valuation currency, observation date, and freshness when available.
- [ ] Pending allocation and unreconciled snapshot differences remain visible in report output.
- [ ] Multi-currency totals remain separated unless an explicit dated conversion source is included.
- [ ] Existing expense/income and excluded-transfer totals remain compatible and exclude confirmed investment principal.
- [ ] Stable CLI/JSON output and focused tests cover USD or digital-dollar acquisition, CDT, brokerage lifecycle, missing values, stale snapshots, and cross-currency reporting.

## Blocked by

- `0028-track-complete-cdt-lifecycle.md`
- `0029-track-complete-brokerage-investment-lifecycle.md`
- `0030-reconcile-investment-positions-and-dated-valuations.md`
