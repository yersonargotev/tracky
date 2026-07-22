# Read-only dashboard data contract

Date: 2026-07-21

## Recommendation

Expose one Rust-owned, presentation-neutral read model, versioned independently as
`tracky.dashboard.v1`. Build every response inside one SQLite read transaction
through a shared `open_dashboard_database` seam. The browser may choose labels,
colors, chart geometry, selected drill-down, and URL/history state, but it must not
recalculate finance rules, reconciliation status, freshness, or cross-currency
totals.

The MVP needs two resources:

1. `GET /api/v1/dashboard?start=YYYY-MM-DD&end=YYYY-MM-DD[&currency=...&account=...&category=...]`
   returns filter dimensions, monthly measures, category/account breakdowns,
   closing investment positions, and typed alerts as one consistent snapshot.
2. `GET /api/v1/transactions` accepts the same filters plus `metric`, `month`,
   `cursor`, and `limit`; it returns the canonical rows (or matching expense
   lines) behind a chart value. Order is `(posted_date, id)` and pagination is
   cursor-based.

Manual refresh simply repeats these reads in a new read transaction. It never
runs a migration or writes a refresh marker.

## Existing seams to preserve

| Seam | What is already canonical | Reuse decision |
| --- | --- | --- |
| `summarize_finances` | Inclusive date validation; income, positive expense, net cash flow, category, income-source, excluded-transfer, investment-contribution, and dividend totals ([types](../../src/storage.rs#L379-L445), [queries](../../src/storage.rs#L5892-L6118)) | Extract its query/calculation layer so it accepts dashboard filters and a caller-owned read-only connection. Do not call the current CLI wrapper: it opens writable storage and applies migrations ([CLI](../../src/cli.rs#L2960-L2977), [open helper](../../src/cli.rs#L3863-L3867)). |
| `list_canonical_transactions` | Canonical-only rows with inclusive date, account, category, income-source, and kind filters, deterministically ordered ([filter](../../src/storage.rs#L659-L667), [query](../../src/storage.rs#L5838-L5889)) | Reuse for drill-down after extending it with currency, pagination, and expense-line projection. Its list response currently omits lines and provenance. |
| Registry lists | Stable account, category, and income-source IDs/names ([types](../../src/storage.rs#L43-L97), [account list](../../src/storage.rs#L1178-L1198), [category list](../../src/storage.rs#L1319-L1337)) | Reuse as filter dimensions, omitting masked identifiers unless the final UI demonstrates a need. |
| `investment_reports::report` | Date-range capital, acquisition, return, cost, closing-position, pending, freshness, and reconciliation semantics ([response](../../src/investment_reports.rs#L8-L149), [report](../../src/investment_reports.rs#L978-L1031)) | Make its aggregation accept account/currency filters and return structured alert keys. It already receives a caller-owned connection, and the CLI opens it read-only ([CLI](../../src/cli.rs#L2980-L2996)). |
| Reconciliation | Derived/observed values, differences, original baseline, age, and status ([types](../../src/reconciliation.rs#L69-L105), [comparison](../../src/reconciliation.rs#L279-L422)) | Keep this as the only authority for reconciliation and freshness. Do not recreate its status ladder in JavaScript. |
| Positions/instruments | Exact quantities and historical costs ([position type](../../src/investments.rs#L106-L114), [aggregation](../../src/investments.rs#L322-L404), [instrument list](../../src/investments.rs#L467-L479)) | Use instruments for display metadata. For dashboard closing positions prefer the consolidated investment report: `list_positions` covers allocation-derived cost only and has no as-of or valuation semantics. |
| Export/integrity | Read-only opening, broad canonical entity coverage, redacted provenance, and an existing `user_version` check ([open](../../src/operations.rs#L99-L105), [integrity check](../../src/operations.rs#L174-L237), [export queries](../../src/operations.rs#L431-L489)) | Reuse the opening/error vocabulary, not the generic entity-map export as a browser API. Export is too broad, leaks implementation tables, and would force finance logic into the browser. |

Candidate rows are never metric input: all finance queries above read canonical
transactions or current investment heads. Pending investment allocations and
provider events are operational alerts, not hidden additions to totals.

## `tracky.dashboard.v1` read model

The exact Rust names may vary, but the JSON shape and semantic ownership should
be equivalent to:

```text
DashboardResponse {
  schema_version, ok, read_at, filters,
  dimensions { currencies, accounts, categories, instruments },
  monthly: [MonthlyMeasures],
  categories: [CategoryBreakdown],
  accounts: [AccountBreakdown],
  investments { flows, closing_positions },
  alerts: [DashboardAlert],
  errors
}

MonthlyMeasures {
  month, currency,
  income_minor, expense_minor, net_cash_flow_minor,
  investment_contribution_minor
}

ClosingPosition {
  account_id, instrument_id?, instrument_type?, quantity?,
  historical_cost_minor?, cost_currency?,
  observed_value_minor?, valuation_currency?,
  effective_date?, observed_at?, age_days?, freshness,
  reconciliation_status, quantity_difference?, value_difference_minor?
}
```

All `*_minor` fields are signed base-10 **JSON strings**, not JSON numbers.
SQLite and current Rust responses use `i64`, but JavaScript cannot exactly
represent every `i64`. Quantities, prices, rates, and quantity differences remain
canonical decimal strings; floating point is never part of the transport. Rust
performs checked arithmetic and the browser formats/scales exact values. Currency
is an uppercase code adjacent to every monetary value; historical cost and
observed valuation retain their separate currencies.

IDs and enum values are stable machine keys. Names are display metadata. Empty
months are emitted as zero-valued rows for every selected currency (or every
currency present in the filtered result), so the browser does not invent missing
period semantics.

## Exact metric and filter semantics

- `start` and `end` are required valid ISO dates and inclusive. Monthly buckets
  are calendar `YYYY-MM` intersections with that range; partial first/last months
  remain partial.
- **Income** is the checked sum of positive `amount_minor` for canonical
  `transaction_kind='income'`.
- **Expense** is the positive magnitude `-amount_minor` for canonical
  `transaction_kind='expense'`. Without a category filter, each transaction is
  counted once. With categories selected, sum only matching expense lines, so a
  split contributes its matching portion rather than its whole transaction.
- **Savings** is presented from `net_cash_flow_minor = income - expense`. It does
  not subtract investment contributions and does not include transfers. The API
  keeps the unambiguous `net_cash_flow_minor` name even if the UI label is
  “Savings.”
- **Investment contribution** is the positive magnitude of canonical
  `investment_contribution` outflows. It is separate from consumption expense
  and from later acquisitions/reinvestment, matching the existing finance and
  consolidated-investment contracts.
- Own-account/card-payment transfer pairs do not enter income, expense, savings,
  or contribution. They may be returned only as excluded-transfer context; a
  pair is counted once, as the existing query does
  ([source](../../src/storage.rs#L6090-L6118)).
- Currency filtering matches the currency of each measure. It never converts,
  sums, sorts by converted value, or treats denomination, historical-cost, and
  valuation currencies as interchangeable.
- Account filtering matches the row's owning `account_id`: ledger measures use
  the canonical transaction account; investment operation/position measures use
  their investment account. External contributions therefore remain attributed
  to their source canonical account rather than being guessed onto a broker
  position.
- Category filtering applies only to expense measures, category/account expense
  breakdowns, and expense drill-down. Income, contributions, investment flows,
  positions, and alerts are unchanged; the response echoes this applicability so
  the browser can explain it rather than silently hiding unrelated data.
- Account activity contains per account/currency income, expense, net cash flow,
  investment contributions, and canonical row counts under the same rules.
- Chart drill-down supplies a typed `metric`; Rust maps it to the same query
  predicate/aggregation rule. The browser cannot send arbitrary SQL or infer rows
  by downloading the export.

## Investment freshness and alerts

Closing state is **as of `end`**. Use the most recent applicable snapshot for
each `(account_id, instrument_id-or-cash, currency)` whose provider-effective
date (or observation date fallback) is not after `end`. Existing consolidated
report logic already chooses applicable snapshots and falls back to derived
historical-cost positions when none exists
([source](../../src/investment_reports.rs#L429-L544)).

Preserve the current deterministic policy: age is calendar days from
`observed_at` to `end`; age greater than seven days is `stale`, otherwise `fresh`
([source](../../src/reconciliation.rs#L10-L12), [calculation](../../src/reconciliation.rs#L297-L305)).
`unavailable` means no applicable snapshot. A stale position retains both its
freshness and its underlying/original reconciliation condition; do not collapse
all useful detail into the word `stale`.

Replace the report's opaque `"account:instrument:currency"` alert strings with
typed `DashboardAlert` values containing `kind`, `severity`, account/instrument/
currency keys, applicable dates, age, and available exact differences. MVP kinds
are: `pending_allocation`, `pending_provider_event`,
`missing_snapshot_position`, `missing_valuation`, `stale_valuation`, and
`reconciliation_difference`. The existing report already detects these classes
([pending types](../../src/investment_reports.rs#L104-L133), [classification](../../src/investment_reports.rs#L461-L500)).

## Strict read-only and schema behavior

1. Open an existing file with `SQLITE_OPEN_READ_ONLY` (optionally URI
   `mode=ro`); never fall back to `Connection::open`, create a missing database,
   attach another database, or call `apply_migrations`. SQLite documents that a
   read-only open fails if the database does not exist
   ([official API](https://www.sqlite.org/c3ref/open.html)).
2. Do **not** use `immutable=1`: Tracky's database can change while the server is
   running and manual refresh must observe it. SQLite disables locking and change
   detection for immutable databases and warns of incorrect/corrupt results if
   the assertion is false ([official URI documentation](https://www.sqlite.org/uri.html)).
3. Set `PRAGMA query_only=ON` as defense in depth, then verify the main database
   is read-only. `query_only` alone is not the security boundary; SQLite states
   that it does not make the connection truly read-only
   ([official PRAGMA documentation](https://www.sqlite.org/pragma.html#pragma_query_only)).
4. Before serving, validate an explicit Tracky application/schema generation and
   every table/column/index needed by `tracky.dashboard.v1`. Return a stable
   `unsupported_schema_version` error with expected/actual generations and
   guidance to run the normal operational command separately. Never reveal the
   database path or raw SQL error to the browser.
5. Execute each dashboard response in one read transaction, so all sections
   describe one database snapshot. A refresh starts a new transaction and may see
   newly committed CLI work.

The compatibility marker is currently missing. `user_version` is fixed at `1`
([migration](../../migrations/0001_review_first_schema.sql#L519)), while
`apply_migrations` also performs later conditional schema changes in Rust
([source](../../src/storage.rs#L791-L815)). Consequently, `user_version == 1`
alone cannot prove that an older database has every dashboard column. Before
implementation, introduce a deliberately bumped persistent schema generation
(and preferably an application identifier) through the normal writable migration
path; the dashboard only reads it. Until that migration exists, startup must use
an exact required-capability probe and fail closed.

## Missing read-only capabilities to implement

1. Shared `open_dashboard_database` plus fail-closed compatibility validation;
   finance summary cannot reuse its current writable CLI path.
2. A filterable/month-bucketed finance query layer. Current summary accepts only
   dates, while transaction list lacks currency and pagination.
3. Filter-aware consolidated investment aggregation. Post-filtering an already
   aggregated report would produce incorrect capital/net/reconciliation results.
4. Comprehensive as-of positions from the investment report exposed as stable
   read-model types; do not use allocation-only `list_positions` as the dashboard
   authority.
5. Structured alerts instead of concatenated keys, and explicit `age_days` on
   the dashboard closing-position projection.
6. Exact string serialization for minor units and checked browser formatting.
7. Focused tests proving metric parity with existing reports, split-category and
   transfer behavior, filter composition, snapshot selection/freshness, i64 and
   decimal exactness, deterministic pagination, incompatible-schema failure, and
   byte/mtime invariance of the database and sidecars across startup, reads, and
   refresh.

## Browser-only state

Keep selected chart, hover/focus, expanded alert, drill-down cursor, visual sort,
theme, and URL/history serialization in the browser. Keep dates and selected
filter IDs there only as request state echoed by Rust. All economic values,
zero-filling, filter applicability, metric-to-row drill mapping, freshness,
reconciliation, and error classification belong to Rust.
