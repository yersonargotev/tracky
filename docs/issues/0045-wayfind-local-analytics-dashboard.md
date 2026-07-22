# 0045 — Wayfind the local analytics dashboard

Labels: `wayfinder:map`

## Destination

Produce an implementation-ready specification for Tracky's local web dashboard,
covering experience, embedded architecture, read-only data contracts, security,
verification, and distribution, with no material decision left for an
implementation agent.

## Notes

- Use `grilling`, `domain-modeling`, `research`, and `prototype` as appropriate.
- The dashboard is a personal, local, on-demand, read-only analytics surface;
  canonical review and mutation remain in the CLI and a possible future TUI.
- The MVP covers per-currency monthly income, expenses, savings, and investment
  contributions; trends; category and account breakdowns; historical-cost
  positions; available valuation freshness; and reconciliation alerts.
- Support date, currency, account, and category filters, chart drill-down, and
  manual refresh without writing to SQLite.
- Ship as one Tracky binary, operate offline on loopback only, preserve provenance
  redaction, and never combine currencies or invent exchange rates.
- Dashboard startup must use strict read-only database access and must never run
  migrations.
- This map plans decisions only; it does not implement the dashboard.

## Decisions so far

<!-- Closed child tickets are indexed here by linked title and one-line gist. -->

- [Compare embedded local-web architectures](0046-compare-embedded-local-web-architectures.md)
  — Research recommends a hardened foreground Axum/Tokio loopback server with
  fixed embedded assets and a framework-free MVP frontend, subject to final
  product-contract selection and real release-target measurement.
- [Define the read-only dashboard data contract](0047-define-read-only-dashboard-data-contract.md)
  — Research specifies a versioned Rust-owned exact read model, canonical
  drill-downs, typed alerts, and fail-closed schema validation over strict
  read-only SQLite transactions.
- [Prototype the dashboard information architecture](0048-prototype-dashboard-information-architecture.md)
  — Live comparison selects a single-scroll Monthly ledger with one explicit
  currency scope, contextual alerts, canonical drill-downs, and read-only
  empty/stale behavior.
- [Choose the dashboard product and architecture contract](0049-choose-dashboard-product-and-architecture-contract.md)
  — Live synthesis adopts a foreground hardened Axum dashboard, embedded
  Monthly ledger, Rust-owned exact snapshots, explicit refresh, and the current
  release-platform boundary.
- [Define the dashboard verification and release contract](0050-define-dashboard-verification-and-release-contract.md)
  — Live synthesis fixes release-blocking semantic, security, lifecycle,
  browser, accessibility, resource, dependency, size, packaging, and durable
  evidence gates for every supported target.
- [Plan dashboard implementation slices and rollout](0051-plan-dashboard-implementation-slices-and-rollout.md)
  — Live synthesis orders seven cumulative PR slices from an immutable evidence
  baseline through a secure tracer bullet and complete Tracky 0.2.0 rollout.

## Not yet specified

- None.

## Out of scope

- Hosted, remote, multi-user, mobile-native, or cloud-synchronized dashboards.
- Editing, importing, reviewing, or otherwise mutating canonical data from the
  dashboard.
- Currency conversion or consolidated totals across unlike currencies.
- Implementing or cancelling the pending TUI review issues.
- Dashboard implementation during this Wayfinder effort.
