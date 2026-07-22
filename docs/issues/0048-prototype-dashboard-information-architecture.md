# 0048 — Prototype the dashboard information architecture

Labels: `wayfinder:prototype`

Status: closed

Assignee: yersonargotev

## Parent map

- [Wayfind the local analytics dashboard](0045-wayfind-local-analytics-dashboard.md)

## Question

Which page hierarchy, visual emphasis, charts, filters, drill-downs, alerts, and
empty/stale states let the user understand the agreed finance and investment MVP
quickly without implying unsupported cross-currency totals or write actions?

Create a cheap interactive prototype with synthetic, privacy-safe data and work
through it live with the user. The prototype is a decision aid, not production
frontend code; its resolution should record the accepted information architecture
and the rejected alternatives.

## Blocked by

- None.

## Resolution

The live comparison is preserved as disposable decision evidence in
[Dashboard information-architecture prototype](../prototypes/dashboard-information-architecture/README.md).
The user selected **A — Monthly ledger** as the base information architecture.

Adopt one responsive, scrollable overview in this order:

1. A persistent header states the inclusive date range, account scope, explicit
   read-only status, filter entry point, last-read time, and manual refresh.
2. A top-level currency selector opens exactly one native-currency ledger at a
   time. Every summary, chart, breakdown, and drill-down remains visibly scoped
   to that currency; there is no all-currency balance, conversion, or ranking.
3. The opening summary shows income, consumption expense, savings/net cash flow,
   and investment contributions together, while labelling contributions as
   separate from expenses and savings as income minus expense.
4. A monthly income/expense trend is the primary chart. Selecting a month opens
   the canonical rows behind that mark in a read-only side drawer.
5. Expense categories and account activity follow the trend. Selecting a row
   opens the same canonical drill-down; category filters explicitly state that
   they affect expense measures only.
6. Reconciliation and freshness alerts remain visible beside the analytical
   breakdown rather than becoming the page's organizing principle. Alerts link
   to the affected position and retain stale, unavailable, and underlying
   reconciliation conditions separately.
7. Investment positions close the overview in a dense as-of table that keeps
   exact quantity, historical-cost currency, observed valuation currency,
   freshness, and reconciliation status distinct.

Date, currency, account, and category filters use one compact global panel and
are echoed in the page scope. Empty results replace misleading zero summaries
with an explanation and a direct path to review filters. Stale data preserves
available historical cost and visibly marks affected observations. Drill-downs
offer pagination and inspection only; corrections direct the user to the Tracky
CLI. Mobile retains the same order and collapses sections vertically rather than
introducing different navigation.

Reject **B — Question paths** as the default because separate question pages hide
the cross-section context that makes the personal ledger useful and require
navigation to discover spending or investment warnings. Its plain-language
questions may inform headings or help text, but not the primary hierarchy.

Reject **C — Signal desk** as the default because leading with exceptional states
overweights reconciliation during normal use, while simultaneous currency lanes
invite visual comparison even without a calculated total. Keep its principle of
clear freshness signals, but express those signals inside the Monthly ledger's
alert and position sections.
