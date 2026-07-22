# 0047 — Define the read-only dashboard data contract

Labels: `wayfinder:research`

Status: closed

Assignee: yersonargotev

## Parent map

- [Wayfind the local analytics dashboard](0045-wayfind-local-analytics-dashboard.md)

## Question

What stable, presentation-neutral read model should supply every MVP metric,
filter, drill-down, freshness warning, and reconciliation alert without duplicating
finance rules or opening SQLite for writes?

Inventory the reusable canonical query/report/export seams, define metric and
filter semantics, identify missing read-only capabilities, specify schema-version
failure behavior, and preserve exact money, quantity, currency, valuation, and
provenance boundaries. The answer should decide what belongs in Rust responses
versus browser-only presentation state, not implement either layer.

## Blocked by

- None.

## Resolution

Research is captured in
[Read-only dashboard data contract](../research/read-only-dashboard-data-contract.md).

Adopt a Rust-owned, presentation-neutral `tracky.dashboard.v1` read model with
one aggregate dashboard resource and one canonical transaction drill-down
resource. Rust owns metric definitions, filter applicability, monthly
zero-filling, exact checked arithmetic, investment as-of selection, freshness,
reconciliation, and typed alerts; the browser owns only presentation and
ephemeral interaction state.

Preserve exact values across the JavaScript boundary by encoding every
minor-unit amount as a base-10 JSON string and retaining quantities, prices,
rates, and differences as canonical decimal strings. Keep every value adjacent
to its actual currency and never aggregate or convert unlike currencies.

Build every response within one SQLite read transaction through a shared strict
read-only open-and-validate seam. The dashboard must never call migrations, use
an immutable connection, create a missing database, or expose raw database
errors. Because the current `user_version = 1` does not distinguish later
conditional Rust migrations, implementation must add a persistent schema
generation through the normal writable path, or use an exact required-capability
probe until that marker exists, and fail closed with operational guidance when
the schema is incompatible.

Reuse the canonical finance, transaction, investment-report, reconciliation,
registry, and instrument seams, but add filter-aware monthly aggregation,
currency-aware cursor drill-down, structured alert keys, exact transport
serialization, and filterable consolidated investment reporting. Do not expose
the generic export as a browser API or use allocation-only positions as the
dashboard's as-of authority.
