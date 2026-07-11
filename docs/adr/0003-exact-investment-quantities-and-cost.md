# Exact investment quantities and historical cost

Tracky stores investment cash consideration and fees as signed-safe `i64` minor-unit integers paired with an explicit three-letter currency. Acquired quantities are positive plain decimal strings validated into an exact `i128` coefficient and scale, then persisted in canonical decimal form.

Persisted quantities accept at most 38 digits in total and 18 fractional places. Exponent notation, signs, zero, `NaN`, and infinity are rejected. Derived additions are checked and fail rather than overflow. Tracky does not persist monetary values, quantities, or rates as `f64`.

An effective rate is exposed as an exact ratio (`cost_minor_numerator` / `quantity_denominator`) rather than as a rounded decimal. It exists only for allocation records that contain both sides.

CDT lifecycle principal, gross interest, capitalized interest, returned principal, withholding, other deductions, and net cash use the same signed-safe `i64` minor-unit representation with one explicit currency. Agreed CDT rates are non-negative canonical decimal strings in `TEXT`, with the same maximum of 38 total digits and 18 fractional places; signs, exponent notation, `NaN`, infinity, and values outside those limits are rejected. Rates are recorded contractual data only: Tracky does not use them to estimate daily accrual or persist any `f64` result.

Allocations use append-only revisions and a separate active head. Positions are queries over active revisions grouped by account, instrument, and cost currency; they are never edited as standalone balances. A fee has a stable, durable component identity shared with the canonical expense side and is stored separately from principal. It is either `capitalized` into same-currency historical cost with no expense link, or `separate` and linked to one matching canonical expense. Component treatment and expense linkage are immutable across revisions; historical uniqueness and expense-side conflict checks reject reuse and double counting even after replacement.

## Consequences

- Instrument denomination and contribution cost currency may differ without conversion.
- USD fiat, USDC, COPW, securities, fixed income, and generic instruments retain distinct identities.
- Replacements preserve old revisions, correction reason, provenance source, and the replaced revision link.
- One typed allocation action may validate and commit multiple instrument legs atomically.
- Legacy canonical rows keep their original pending-allocation storage column; transaction list/inspect derive pending, partial, or full status from active allocation principal so old databases remain compatible.
- CDT positions are derived from active append-only lifecycle operations anchored to consumed `fixed_income` allocations. Unchanged renewal principal is not another contribution; additional principal requires another allocation, and capitalized interest remains a reinvested return.

### Brokerage disposals

Brokerage quantities use the same canonical decimal representation, internally scaled to at most nine fractional digits. Cash, cost, proceeds, realized results, fees, dividends, withholding and deductions remain `i64` minor units. Security sales use deterministic moving weighted-average historical cost: a partial disposal assigns `accumulated_cost_minor * sold_quantity / held_quantity`, truncating only the indivisible minor-unit remainder; a final disposal receives the complete remaining cost. This is portfolio bookkeeping, not a tax-lot or tax calculation.

### Dated observations and reconciliation adjustments

Provider snapshot quantities and prices use the same canonical decimal boundary. Observed cash and values use non-negative `i64` minor units with explicit currencies. Snapshots are immutable evidence and never economic events. Reviewed missing-history adjustments are separate append-only revisions replayed only in derived reconciliation state.
