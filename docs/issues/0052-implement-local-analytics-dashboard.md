# 0052 — Implement the local analytics dashboard

Labels: `ready-for-agent`

## Problem Statement

Tracky stores canonical personal-finance and investment data locally, but users
must currently assemble analytical answers from separate CLI reports. They lack
one supported place to inspect monthly income, consumption expense, savings,
investment contributions, account and category activity, historical-cost
positions, available valuations, freshness, and reconciliation alerts together.

The missing surface must not weaken Tracky's local-first and review-first model.
It must remain read-only, must never combine unlike currencies or invent exchange
rates, must not disclose sensitive financial or provenance data beyond the local
user, and must ship inside the existing binary without runtime web assets or a
frontend toolchain. Confidence also cannot rest on a development server: the real
packaged artifacts must meet explicit correctness, security, accessibility,
performance, licensing, size, installation, and lifecycle gates.

## Solution

Add `tracky dashboard` as a foreground, terminal-bound local analytical
dashboard. The command opens a strictly validated read-only SQLite connection,
binds an ephemeral listener only on `127.0.0.1`, prints a capability-bearing URL,
and opens the default browser unless `--no-open` is requested. It serves one
responsive Monthly ledger whose exact initial semantic content is rendered by
Rust and progressively enhanced by fixed embedded CSS and JavaScript.

The dashboard presents one explicit currency at a time across an inclusive date
range. It shows monthly income, consumption expense, savings/net cash flow,
investment contributions, category and account breakdowns, canonical transaction
drill-downs, historical-cost positions, available valuations, freshness, pending
allocation, and reconciliation alerts. Rust owns all financial definitions,
filter composition, checked arithmetic, as-of selection, pagination, exact JSON
serialization, and sanitized failure states. The browser owns only presentation
and ephemeral interaction.

Deliver the solution through seven cumulative, dependency-ordered changes: an
immutable evidence baseline; a secure runnable monthly tracer; complete finance,
filter, and drill-down semantics; investments and alerts; the complete Monthly
ledger interaction; refresh/adverse-state/accessibility behavior; and packaged
verification plus the supported Tracky 0.2.0 rollout.

## User Stories

1. As a Tracky user, I want to launch the dashboard from the same binary, so that I do not install or maintain another application.
2. As a Tracky user, I want the exact local URL printed after readiness, so that I can open it manually when browser launch fails or is disabled.
3. As a terminal user, I want `--no-open`, so that I can control which browser or profile accesses my financial data.
4. As a Tracky user, I want startup failures to exit clearly before serving data, so that an incompatible or unreadable database is never mistaken for an empty ledger.
5. As an existing Tracky user, I want an explicit database-upgrade command, so that I can prepare a legacy database without allowing the dashboard to migrate it.
6. As a privacy-conscious user, I want the dashboard bound only to literal loopback, so that it is not reachable from another machine or network interface.
7. As a privacy-conscious user, I want every process to use a fresh unguessable capability, so that another local web page cannot casually access my financial data.
8. As a privacy-conscious user, I want restrictive request validation and response headers, so that Host manipulation, cross-site requests, framing, caching, and MIME confusion fail closed.
9. As a privacy-conscious user, I want no CDN, telemetry, remote font, or other external request, so that browsing the dashboard does not disclose activity to third parties.
10. As a Tracky user, I want the dashboard to leave my database unchanged, so that analytics cannot modify canonical transactions, provenance, schema, or SQLite sidecars.
11. As a Tracky user, I want the current twelve-month window by default, so that the dashboard is immediately useful without configuration.
12. As a Tracky user, I want startup date overrides, so that I can begin with a known historical reporting period.
13. As a Tracky user, I want one visible currency scope at a time, so that unlike currencies are never implied to be additive or comparable.
14. As a Tracky user, I want a deterministic initial currency and an explicit override, so that startup behavior is predictable.
15. As a Tracky user, I want to switch currencies, so that I can inspect each native-currency ledger independently.
16. As a Tracky user, I want to select multiple compatible accounts, so that I can compare a chosen subset without losing currency context.
17. As a Tracky user, I want to select multiple expense categories, so that I can focus consumption analysis without altering income or investment semantics.
18. As a Tracky user, I want an empty filter selection to produce an explained empty result, so that “none” is never silently interpreted as “all.”
19. As a Tracky user, I want the inclusive date range and active scope always visible, so that every number has clear context.
20. As a Tracky user, I want monthly income, consumption expense, savings, and investment contributions together, so that I can understand cash flow without treating investment principal as consumption.
21. As a Tracky user, I want months with no activity represented explicitly, so that trends do not collapse or skip time.
22. As a Tracky user, I want exact monetary values, so that charts and formatting never introduce binary floating-point changes.
23. As a Tracky user, I want a monthly trend with an exact table equivalent, so that I can understand patterns visually or semantically.
24. As a keyboard or assistive-technology user, I want the exact initial ledger available without JavaScript enhancement, so that core financial content does not depend on a graphical runtime.
25. As a Tracky user, I want account and category breakdowns, so that I can explain where activity occurred.
26. As a Tracky user, I want chart and breakdown selections to open canonical rows, so that every aggregate can be inspected rather than merely trusted.
27. As a Tracky user, I want stable cursor pagination in drill-downs, so that large ledgers remain inspectable without duplicates or gaps.
28. As a Tracky user, I want corrections to direct me to the CLI, so that the dashboard preserves the review-first mutation boundary.
29. As an investor, I want exact historical-cost positions, so that I can inspect confirmed capital allocation without inventing market value.
30. As an investor, I want cost and observed-valuation currencies shown separately, so that unlike units are not silently combined.
31. As an investor, I want valuation dates and freshness visible, so that old observations are not presented as current.
32. As an investor, I want pending allocation and reconciliation alerts visible near the affected position, so that incomplete financial history is not hidden.
33. As a Tracky user, I want missing or unavailable metrics distinguished from zero, so that absence of evidence is not reported as a financial fact.
34. As a Tracky user, I want to refresh explicitly, so that I control when the dashboard rereads local data.
35. As a Tracky user, I want a failed refresh to retain the last good snapshot, so that a transient problem does not erase useful context.
36. As a Tracky user, I want applicable filters preserved across an internal refresh, so that updating data does not discard my current analysis.
37. As a Tracky user, I want an obsolete drill-down closed after refresh, so that stale canonical rows are not left on screen.
38. As a privacy-conscious user, I want filter and drill-down state kept out of URLs and browser storage, so that financial context is not persisted accidentally.
39. As a keyboard user, I want complete navigation, visible focus, and restored focus, so that every dashboard action remains operable.
40. As a screen-reader user, I want meaningful structure, announcements, labels, and exact tables, so that the dashboard communicates the same facts without visual inference.
41. As a low-vision user, I want usable zoom, reflow, contrast, and target sizes, so that the ledger remains readable and operable.
42. As a motion-sensitive user, I want reduced-motion preferences respected, so that enhancement does not create avoidable discomfort.
43. As a Tracky user, I want graceful Ctrl-C and termination handling, so that closing the terminal reliably stops financial-data access.
44. As a Tracky user, I want no daemon, child process, temporary asset, or listening socket after exit, so that the dashboard has a bounded lifecycle.
45. As a Tracky maintainer, I want deterministic conformance fixtures with manual expected answers, so that production code cannot validate itself.
46. As a Tracky maintainer, I want packaged artifacts tested on every supported target, so that source-checkout success is not mistaken for release readiness.
47. As a Tracky maintainer, I want pinned dependency, license, advisory, and source policy, so that the embedded server remains distributable and reviewable.
48. As a Tracky maintainer, I want fixed asset, dependency, binary, and archive budgets, so that the dashboard cannot grow the CLI without an explicit contract change.
49. As a Tracky maintainer, I want retained machine-readable and human-readable release evidence, so that publication is based on reproducible proof.
50. As a Tracky maintainer, I want the command hidden until every supported gate passes, so that users are not promised an unsupported preview.

## Implementation Decisions

- `tracky dashboard` is a foreground command tied to its terminal. It binds
  `127.0.0.1:0`, prints the exact capability URL only after readiness, opens the
  browser best-effort unless `--no-open` is present, and shuts down gracefully
  within the accepted bound.
- The server uses Axum on a minimal current-thread Tokio runtime. Synchronous
  SQLite work runs outside the async executor's blocking path.
- Rust renders the initial HTML with exact semantic financial content. Fixed
  embedded CSS and framework-free JavaScript progressively enhance charts,
  filters, drill-down, and refresh. There is no Node or frontend build pipeline.
- Rust owns the presentation-neutral `tracky.dashboard.v1` aggregate and
  canonical transaction drill-down contracts. Minor-unit amounts and all exact
  decimals cross JSON as canonical base-10 strings.
- The aggregate route is not exposed in a partial state. Finance and transport
  behavior may be built and tested before the investment projection, but the
  reachable versioned resource is enabled only when every mandatory section is
  truthful and complete.
- A shared read boundary opens existing databases immutably/read-only, applies
  defensive query-only behavior, validates identity and capabilities, creates
  one caller-owned read transaction per response, and returns typed sanitized
  incompatibility or operational failures. It never calls migrations.
- SQLite `application_id` identifies Tracky. Monotonic `user_version` identifies
  schema generation, with generation 2 as the first dashboard-compatible schema.
  Writable migration code sets both only after all migrations succeed.
- `tracky database upgrade` is the explicit legacy recovery path. It recognizes
  legacy Tracky capabilities before writing and refuses unrelated SQLite files.
- The initial inclusive range begins on the first day eleven months before the
  current month and ends today. Tests inject the clock. Startup date options may
  override the range before a listener is created.
- Exactly one currency is active. The initial currency is the first available ISO
  code in lexical order unless explicitly overridden. An unknown override fails
  before listener startup; no currencies is a valid empty state.
- Accounts and categories are multi-select and initially include every compatible
  value. Changing currency removes incompatible selections. Empty selections
  produce an explicit filter-empty state. Category filters affect only expense
  measures and expense drill-downs.
- Savings means income minus consumption expense. Investment contributions remain
  separate from both consumption expense and savings. Unlike currencies are
  never combined, ranked by amount, or converted.
- Rust performs inclusive monthly zero-fill, checked arithmetic, filter
  composition, stable currency-aware cursor pagination, investment as-of
  selection, freshness, reconciliation, and structured alert construction.
- The Monthly ledger order is fixed: scope/header, currency selector, summary,
  monthly trend and exact table, category and account activity, alerts, then
  dense investment positions. Activations use one canonical read-only drawer.
- Browser state is ephemeral. Dates, filters, identifiers, drawer state, and
  pagination are absent from query strings, fragments, cookies, and persistent
  browser storage. Internal refresh preserves applicable state; full page reload
  restores startup scope; browser Back is not repurposed.
- Initial read happens once and refresh happens only on explicit request. Refresh
  failure retains the last successful snapshot with a sanitized stale/error
  state. There is no polling or file watcher.
- Only `GET` is accepted on exact capability-bearing application, asset, and API
  paths. Other methods, unknown paths, and traversal reveal no financial data and
  retain the defensive header policy. No public health endpoint is added.
- Host and Fetch Metadata checks, per-process capability entropy, restrictive
  CSP/frame/referrer/MIME/cache headers, no CORS opt-in, redacted errors/logs, and
  zero external requests are mandatory at the first listener-bearing change.
- Official support matches the current Cargo Dist targets and the fixed minimum
  Safari, Firefox ESR, and Chromium/Chrome contracts. Windows and mobile browsers
  are not supported even if the responsive layout happens to work.
- Implementation proceeds through seven serial cumulative pull requests. Each
  leaves the binary runnable, adds permanent gates, and avoids unrelated refactors.
  The command remains hidden until the final rollout change.
- Tracky 0.2.0 is the first public supported dashboard release. It is documented
  as supported, not beta or preview, only after complete release-candidate proof.

## Testing Decisions

- The primary acceptance seam is the real `tracky` process running against
  deterministic SQLite fixtures. Tests observe only public output and exit codes,
  listener/socket behavior, HTTP/HTML/JSON, browser behavior, filesystem state,
  descendants, and cleanup.
- The same fixtures drive public canonical CLI reports, and exact expected values
  must agree across CLI, aggregate, drill-down, semantic HTML, and tables.
- A narrow secondary Rust read-model seam covers the exhaustive manually authored
  corpus for checked arithmetic, exact decimal transport, zero-fill, filter
  composition, as-of selection, alerts, and cursor pagination. Private helpers
  and Axum implementation details are not test contracts.
- Prior art is the existing CLI integration-test style: synthetic databases,
  public command invocation, exact JSON assertions, schema/row verification, and
  no dependence on private implementation functions.
- Pull-request gates include existing formatting, all-target tests, strict Clippy,
  and release build plus semantic/inmutability conformance, adversarial HTTP
  security, fast lifecycle, browser flows, axe, dependency policy, and static
  asset/dependency budgets.
- Security tests spawn the real process and cover capability isolation, Host and
  Fetch Metadata validation, methods, paths, traversal, headers, no CORS, decoy
  secret non-disclosure, sanitized logs/errors, concurrent instances, and zero
  external network requests.
- Read-only tests compare database files, rows, schema, and relevant sidecars
  before and after startup, browsing, filtering, drill-down, refresh, failure,
  and shutdown. No dashboard path may create a missing database.
- Lifecycle tests cover readiness ordering, non-fatal browser-open failure,
  fatal pre-listener failures, Ctrl-C, SIGTERM, bounded drain, independent ports
  and capabilities, 100 start/stop cycles, descriptors, descendants, sockets,
  temporary files, and browser ownership.
- Browser tests exercise every supported minimum and current stable version for
  load, filters, drill-down, refresh, empty/error/stale states, enhancement
  failure, keyboard navigation, focus, responsive layout, and exact semantic
  fallback.
- Accessibility requires zero applicable axe violations plus retained manual
  evidence for VoiceOver/Safari, Orca/Firefox, keyboard-only operation, zoom,
  320-CSS-pixel reflow, contrast, target size, reduced motion, announcements,
  and non-color meaning.
- Release-candidate tests use real native Cargo Dist artifacts and the fixed
  stress fixture. They enforce every accepted latency, RSS, CPU, thread,
  descriptor, leak, dependency, asset, binary, archive, installer, and browser
  budget without silently changing the baseline or threshold.
- Release proof is a schema-validated `dashboard-verification.json` plus a
  human-readable Markdown rendering. Publication is technically blocked unless
  the evidence is complete, passing, and maintainer-approved.

## Out of Scope

- Hosted, remote, multi-user, cloud-synchronized, or non-loopback dashboards.
- Editing, importing, reviewing, correcting, or otherwise mutating canonical
  financial data from the dashboard.
- Currency conversion, consolidated unlike-currency totals, or inferred exchange
  rates.
- A dashboard daemon, background service, PID file, or browser lifecycle manager.
- Static-file snapshot export or `file://` launch fallback.
- Frontend frameworks, Vite, TypeScript, Rust/Wasm, runtime asset directories,
  remote assets, telemetry, or a Node toolchain.
- Generic export, arbitrary SQL/query APIs, provenance browsing, or raw database
  errors in browser responses.
- Automatic refresh, polling, or filesystem watching.
- Windows support, mobile-browser support, or mobile-native applications.
- Implementing, replacing, or cancelling the pending TUI review work.
- Investment advice, market-data acquisition, or invented current valuations.

## Further Notes

- The accepted product, architecture, read model, information architecture,
  verification contract, and slice order are preserved in the completed local
  analytics dashboard Wayfinder and its closed child decisions.
- Work begins with the evidence baseline before adding dashboard dependencies or
  packaged code. The implementation frontier then advances through the seven
  ordered tickets derived from this specification.
- A failed or missing release gate blocks publication. Any threshold change,
  accepted deviation, browser-baseline change, or architecture replacement
  requires an explicit contract revision rather than an implementation shortcut.
