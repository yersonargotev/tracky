# Investment reconciliation JSON contract

All `snapshots` actions require `--json` and return `tracky.investment-reconciliation.v1`. Stable actions are `record`, `list`, `inspect`, `compare`, `adjust`, `replace-adjustment`, and `adjustment-history`.

A snapshot is immutable provider evidence, not an economic operation or editable position. It contains an RFC 3339 observation time, optional provider effective date, source, optional provider reference, provenance, and unique account/instrument/currency observations. Exact quantities and prices are canonical decimal strings (38 digits, at most 18 fractional places); cash and observed values are non-negative `i64` minor units. A value always has a three-letter valuation currency. Missing prices and values remain absent.

Comparison derives state at the provider effective date, or otherwise the calendar date of the observation, from active allocation, CDT, brokerage, and reviewed-adjustment heads effective by that date. It reports observed/derived quantities, cost, cash, and comparable observed valuation; it never invents FX or a price. When an observed value and quantity are present, the same observation can value the derived quantity proportionally in that valuation currency; no cross-currency subtraction occurs.

Statuses are `matched`, `quantity_mismatch`, `cash_mismatch`, `missing_derived_position`, `missing_snapshot_position`, `valuation_unavailable`, `currency_mismatch`, and `stale`. Zero through seven calendar days after observation is `fresh`; a later date is `stale`.

Adjustments are separately typed, explicitly reasoned operations for confirmed missing history. Revisions are append-only with an active head. The first reconciliation baseline is captured with the snapshot, preserving its original status and difference after an adjustment or correction.
