# Tickets: Local analytics dashboard

These tickets build Tracky's supported local, read-only Monthly ledger from the
[local analytics dashboard specification](docs/issues/0052-implement-local-analytics-dashboard.md).
Every ticket is `ready-for-agent` and leaves the Tracky binary runnable.

Work the **frontier**: any ticket whose blockers are all done. This plan is a
linear chain, so work it from top to bottom and integrate each ticket before
starting the next.

## Freeze dashboard release evidence baseline

**What to build:** Establish reproducible dashboard-free release evidence and
the automated policy scaffolding that will judge every later dashboard change,
without changing Tracky's product behavior or packaged output.

**Blocked by:** None — can start immediately.

- [x] Record and retain the dashboard-free commit, Rust toolchain, lockfile hash,
  Cargo Dist targets, exact executable/archive sizes, archive contents, artifact
  hashes, and reproducible comparison commands for every supported target.
- [x] Create a deterministic SQLite conformance corpus covering the accepted
  finance, investment, filtering, pagination, empty, stale, unavailable,
  incompatible, overflow, and error scenarios.
- [x] Hand-author the expected financial and transport results independently of
  production calculations and make fixture provenance reproducible.
- [x] Define and validate the machine-readable dashboard evidence manifest and
  its human-readable rendering inputs.
- [x] Pin and enforce dependency license, advisory, source, yanked-package, and
  prohibited-duplicate policy, including inventory and notice generation.
- [x] Add static asset, dependency-count, binary, and archive comparison entry
  points without moving the accepted baseline or thresholds.
- [x] Keep existing formatting, tests, strict Clippy, and release build green and
  demonstrate that this ticket changes no user-visible behavior.

## Deliver the secure read-only monthly dashboard tracer

**What to build:** Let a user run the hidden dashboard command against an
existing Tracky database and receive a real exact monthly ledger in semantic
HTML through a fully secured, terminal-bound loopback process, while preserving
the database byte-for-byte and providing an explicit legacy upgrade path.

**Blocked by:** Freeze dashboard release evidence baseline.

- [x] Identify Tracky databases with SQLite `application_id` and monotonic
  `user_version`, treating generation 2 as the first dashboard-compatible
  generation and committing markers only after successful writable migrations.
- [x] Provide an explicit database-upgrade command that recognizes legacy Tracky
  capabilities before writing, upgrades safely, and refuses unrelated SQLite
  files with actionable sanitized errors.
- [x] Establish one shared immutable/read-only, query-only, capability-validating
  database boundary and one caller-owned read transaction per response without
  changing canonical CLI report semantics.
- [x] Launch the hidden foreground dashboard with the confirmed default dates,
  date overrides, deterministic currency selection, currency override, and
  `--no-open`; validate every startup argument before listener creation.
- [x] Render exact monthly income, consumption expense, savings/net cash flow,
  and investment contributions plus a semantic table in Rust, including the
  valid no-currency empty state and no provisional public `v1` API.
- [x] Bind only literal `127.0.0.1:0`, generate an independent capability with at
  least 128 bits of entropy, print its URL only after readiness, and treat
  browser-open failure as non-fatal.
- [x] Accept only `GET` on exact capability-bearing routes and enforce Host,
  Fetch Metadata, method, path, traversal, CSP, frame, referrer, MIME, cache,
  same-origin, no-CORS, sanitized logging, and zero-external-request policies on
  success and error responses.
- [x] Embed all assets in the binary, render useful exact content without
  JavaScript, and use fixed CSS/JavaScript only for progressive enhancement.
- [x] Pass the real-process security matrix, secret-decoy non-disclosure,
  immutable-database checks, fatal pre-listener failures, concurrent-instance
  isolation, and bounded Ctrl-C/SIGTERM cleanup with no descendants, sockets,
  runtime assets, or temporary files left behind.
- [x] Add all new evidence to the permanent pull-request gates while keeping the
  command hidden from general help.

## Complete dashboard finance filters and drill-down

**What to build:** Let a user analyze exact monthly finance with composable
date, currency, account, and expense-category scopes and inspect the canonical
rows behind every aggregate through stable pagination, without exposing an
incomplete public dashboard resource.

**Blocked by:** Deliver the secure read-only monthly dashboard tracer.

- [x] Complete inclusive monthly zero-fill and checked income, consumption
  expense, savings, and investment-contribution calculations for every valid
  filter composition.
- [x] Preserve exact minor-unit and decimal values as canonical base-10 strings
  and keep every value adjacent to its actual currency.
- [x] Support exactly one currency, multi-select accounts and categories,
  compatible defaults, clearing incompatible selections on currency change,
  expense-only category effects, and explicit filter-empty results.
- [x] Add exact category and account breakdowns that agree with their aggregate
  totals and corresponding canonical CLI reports.
- [x] Add a currency-aware canonical transaction and expense-line drill-down
  with deterministic stable cursors, filtering, ordering, and pagination.
- [x] Define and conformance-test the final `tracky.dashboard.v1` envelope while
  keeping the externally reachable aggregate route disabled until all mandatory
  investment and alert sections are complete.
- [x] Pass the manual corpus for split expenses, transfers, empty periods,
  unlike currencies, numeric limits, composed filters, and pagination without
  writes or provenance disclosure.
- [x] Keep every prior security, lifecycle, dependency, size, and product gate
  enabled and passing.

## Add dashboard investments and alerts

**What to build:** Let a user inspect truthful investment positions, available
valuations, freshness, pending allocation, and reconciliation alerts in the same
currency-scoped snapshot, and expose the complete versioned dashboard resources
only after every mandatory section is implemented.

**Blocked by:** Complete dashboard finance filters and drill-down.

- [x] Make consolidated investment reporting apply date, currency, and account
  filters before aggregation and preserve exact checked quantities and values.
- [x] Project historical-cost positions from canonical active operations rather
  than treating allocation-only state as the as-of authority.
- [x] Keep historical-cost currency, observed-valuation currency, valuation date,
  freshness, pending allocation, and underlying reconciliation state distinct.
- [x] Select dated investment snapshots deterministically as of the requested
  range and represent missing, unavailable, and stale observations without
  inventing zero or current market value.
- [x] Produce stable structured alert identifiers and connect alerts to the
  affected position without exposing raw provenance or storage errors.
- [x] Complete exact parity with canonical investment and reconciliation reports
  across fresh, stale, unavailable, pending, reconciled, incompatible,
  multi-currency, and checked-decimal cases.
- [x] Enable the capability-bearing `tracky.dashboard.v1` aggregate and
  drill-down routes only after their complete schemas and conformance corpus pass.
- [x] Keep every prior gate enabled and passing with the real process remaining
  read-only and fail-closed.

## Complete the Monthly ledger experience

**What to build:** Let a user explore the accepted responsive single-scroll
Monthly ledger using filters, exact chart/table views, breakdowns, alerts,
positions, and one canonical drill-down while retaining useful semantic content
without JavaScript and persisting no financial interaction state in the browser.

**Blocked by:** Add dashboard investments and alerts.

- [x] Present the fixed information order: persistent scope/header, explicit
  currency selector, four-part summary, monthly trend and exact table, category
  and account activity, contextual alerts, and dense investment positions.
- [x] Implement the compact global filter panel with the confirmed date,
  currency, account, category, default-selection, and empty-selection semantics.
- [x] Make chart points, breakdown rows, alerts, and positions open the same
  read-only canonical drawer with exact values and stable pagination.
- [x] Keep every financial calculation and filter decision in Rust; browser code
  performs presentation and ephemeral interaction only.
- [x] Keep dates, filters, account/category identifiers, drawers, and cursors out
  of query strings, fragments, cookies, local storage, and session storage.
- [x] Preserve applicable state during internal dashboard actions, reset to
  startup scope on full page reload, and do not repurpose browser Back.
- [x] Retain exact server-rendered tables, labels, periods, currencies, freshness,
  and alerts when JavaScript or graphical enhancement fails.
- [x] Meet responsive order, keyboard/focus basics, exact API/table agreement,
  zero external requests, local interaction latency, and the 250 KiB embedded
  uncompressed asset budget.
- [x] Treat the information-architecture prototype only as decision evidence and
  keep all prior gates enabled and passing.

## Add dashboard refresh, failure states, and accessibility

**What to build:** Let a user explicitly refresh the current analysis without
losing applicable context, recover safely from refresh failures, distinguish all
empty/unavailable/stale/error states, and operate every supported dashboard state
to WCAG 2.2 AA expectations.

**Blocked by:** Complete the Monthly ledger experience.

- [x] Read one initial snapshot and refresh only on explicit request, with no
  polling, timer, filesystem watcher, or automatic background reread.
- [x] Preserve compatible filters and navigation after success and close any
  canonical drawer whose cursor no longer belongs to the refreshed snapshot.
- [x] Retain the last good snapshot after failure and present a sanitized stale
  or error indication, announcement, and retry path without raw SQLite details.
- [x] Distinguish valid empty ledger, filter-empty result, unavailable metric,
  stale observation, incompatible schema, fatal startup/initial-read failure,
  and recoverable refresh failure without presenting absence as zero.
- [x] Provide complete keyboard operation, visible and restored focus, live
  announcements, descriptive charts and exact tables, reduced motion, and no
  color-only meaning across every view and state.
- [x] Pass zero applicable axe violations plus automated zoom, 320-CSS-pixel
  reflow, contrast, pointer-target, responsive, enhancement-failure, and
  supported-browser flow checks.
- [x] Preserve database and filesystem immutability through successful and failed
  refreshes and retain every prior product, security, lifecycle, dependency,
  size, and performance gate.
- [x] Produce the release-candidate checklist inputs for manual keyboard,
  VoiceOver/Safari, Orca/Firefox, zoom, reflow, contrast, targets, and motion
  verification without claiming those manual checks before they run.

## Verify packaged dashboard and roll out Tracky 0.2.0

**What to build:** Prove that the real packaged dashboard satisfies every
accepted release contract on every supported target, then make the command and
documentation public as a supported Tracky 0.2.0 feature with publication
technically blocked on complete approved evidence.

**Blocked by:** Add dashboard refresh, failure states, and accessibility.

- [ ] Build the real Cargo Dist artifacts for every supported target from the
  accepted commit, toolchain, lockfile, and profile and compare them against the
  frozen dashboard-free baseline.
- [ ] Enforce the accepted process readiness, snapshot, refresh, navigation,
  drill-down, local-interaction, RSS, CPU, thread, descriptor, memory-growth,
  dependency-count, binary-size, archive-size, and asset-size budgets using the
  deterministic stress fixture.
- [ ] Run the complete packaged security, no-network, immutable-database,
  lifecycle, concurrent-instance, 100-cycle leak, and sanitized-output matrices.
- [ ] Verify published checksums and the exact archive allowlist, notices,
  architecture, permissions, embedded bytes, MIME types, defensive headers, and
  absence of runtime asset writes.
- [ ] Run extracted binaries and shell installers with sandboxed HOME/config and
  temporary prefixes from empty directories and paths containing spaces and
  Unicode, and verify the Homebrew formula on an ephemeral macOS runner.
- [ ] Pass the common dashboard flow on every minimum and current supported
  Safari, Firefox ESR, and Chromium/Chrome version.
- [ ] Complete and sign the manual keyboard, VoiceOver/Safari, Orca/Firefox,
  zoom, reflow, contrast, targets, reduced-motion, and non-color evidence.
- [ ] Generate schema-valid machine-readable and human-readable evidence with
  commit, lockfile, tool, target, browser, hash, command, measurement, result,
  and responsible-maintainer identities.
- [x] Make release publication depend technically on complete passing approved
  evidence and permanently attach both evidence formats to the release.
- [x] Unhide `tracky dashboard` only in the rollout change and document startup,
  `--no-open`, date/currency overrides, supported platforms and browsers,
  privacy boundaries, database upgrade, recovery, and troubleshooting.
- [ ] Publish the first dashboard as supported Tracky 0.2.0 rather than a beta or
  preview, with no threshold waiver or baseline movement hidden in the rollout.

Implementation evidence (2026-07-22): Tracky is versioned as 0.2.0 and the
dashboard is public in CLI help; README, `docs/dashboard.md`, and CHANGELOG cover
the supported non-preview contract. Cargo Dist now emits the exact four-file
archive, and `dashboard_evidence.py` binds release evidence to the accepted
commit, lockfile, checksums, archive bytes, source-identical notices/docs,
executable architecture, permissions, and allowlist. `dashboard-release-proof`
requires protected maintainer approval; the tag workflow cannot reach host,
Homebrew, or announcement without the matching complete proof and permanently
adds its JSON/Markdown renderings to the release artifacts.

Local Apple Silicon Cargo Dist verification passes both frozen size limits
(archive 4,214,744 bytes; executable 15,065,296 bytes), plus the complete Rust,
Clippy, release-build, Chromium, Firefox, WebKit, and axe gates under sandboxed
HOME/config. The remaining unchecked criteria deliberately require retained
real-runner evidence for Linux, installers/Homebrew, stress/resource
matrices, minimum/current browsers, and signed manual accessibility. Publication
remains technically blocked until those external gates populate and approve the
manifest; no evidence or threshold was fabricated or waived in this change.
