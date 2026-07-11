# Consolidated investment report JSON contract

`tracky reports investments --db PATH --from YYYY-MM-DD --to YYYY-MM-DD --json` is a read-only inclusive date-range query. It returns schema `tracky.investment-report.v1`; invalid dates and inverted ranges return stable validation errors without opening the database for writes.

The stable top-level sections are `capital_external`, `acquisitions_and_reinvestment`, `returns_and_income`, `costs_and_withholding`, `closing_positions`, and `pending_and_reconciliation`. Every monetary total is `{currency, amount_minor}` and arrays remain grouped by currency; Tracky performs no FX, stablecoin parity, or implicit conversion. Exact quantities remain canonical decimal strings.

External contributions come only from confirmed canonical investment contributions. Brokerage deposits and later purchases therefore cannot count the same capital twice. Gross acquisitions describe asset purchases/constitutions, not new capital. CDT unchanged renewal principal is excluded; additional principal and capitalized interest remain separately attributable. Returned principal is never income. Interest, dividends, realized results, fees, withholding, other deductions, and net cash remain separate components from active CDT and brokerage operation heads.

Closing state uses the latest snapshot whose provider-effective date (or observation date) is on or before `--to`; later observations are excluded. Reconciliation exposes current and original baseline differences. If no applicable snapshot exists, derived quantity and historical cost still appear with `no_snapshot`, `unavailable`, and a missing-valuation pending item.

Pending/partial contribution allocations, pending provider events, missing snapshot positions, unreconciled differences, missing values, and stale values are never suppressed.
