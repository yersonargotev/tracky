# 0049 — Choose the dashboard product and architecture contract

Labels: `wayfinder:grilling`

Status: closed

Assignee: yersonargotev

## Parent map

- [Wayfind the local analytics dashboard](0045-wayfind-local-analytics-dashboard.md)

## Question

Given the architecture comparison, canonical read-model definition, and validated
information architecture, which complete product and technical contract should
Tracky adopt for `tracky dashboard`, and which alternatives should it explicitly
reject?

Resolve the server and frontend shape, command lifecycle, read-model boundary,
refresh behavior, error and empty states, platform expectations, and privacy
posture together so later verification and implementation tickets inherit one
coherent decision rather than separate local optima.

## Blocked by

- [Compare embedded local-web architectures](0046-compare-embedded-local-web-architectures.md)
- [Define the read-only dashboard data contract](0047-define-read-only-dashboard-data-contract.md)
- [Prototype the dashboard information architecture](0048-prototype-dashboard-information-architecture.md)

## Resolution

Adopt `tracky dashboard` as a foreground, terminal-bound command. It binds only
the literal `127.0.0.1` on an OS-assigned port, prints its exact capability-bearing
URL, opens the default browser unless `--no-open` is set, and remains active until
Ctrl-C or a termination signal. Browser-launch failure is non-fatal; startup,
database-open, or compatibility-validation failure is fatal before financial data
is served. Shutdown is graceful and bounded. The command does not install or
manage a daemon, background service, or PID file.

Use Axum 0.8 on a minimal current-thread Tokio runtime, with synchronous SQLite
work isolated through `spawn_blocking`. Compile the selected single-page,
responsive Monthly ledger's fixed HTML, CSS, and framework-free JavaScript into
the Tracky binary with `include_bytes!`; require neither runtime assets nor a
Node/frontend build toolchain. The release contract must measure the real size,
startup, archive, and license impact on every supported target rather than adopt
the disposable research probe as a budget.

Rust is the sole SQLite and finance-domain authority. Expose the versioned
`tracky.dashboard.v1` aggregate resource and canonical paginated transaction
drill-down defined by the read-model research. Rust owns metric and filter
semantics, zero-filled periods, checked arithmetic, investment as-of selection,
freshness, reconciliation, and typed alerts; the browser owns only rendering and
ephemeral interaction state. Exact minor-unit and decimal values cross JSON as
strings. Do not expose generic export, arbitrary query, or browser-owned finance
logic.

Perform one initial read and refresh only on explicit user request. Each response
comes from one strict read-only SQLite transaction. Refresh preserves applicable
filters and navigation state but closes or invalidates a drill-down that no
longer matches the new snapshot. A refresh failure retains the last successful
snapshot with a visible sanitized stale/error indication and retry; there is no
polling or file watcher.

Fail closed without creating a database or running migrations. Distinguish
startup and initial-read failures, refresh failures, valid empty ledgers,
filter-empty results, unavailable metrics, and stale observations. Never present
missing, incompatible, or failed data as a financial zero. Operational failures
must explain the relevant CLI recovery path without exposing raw SQLite details.

Treat loopback as necessary but insufficient. Combine the literal
`127.0.0.1:0` bind with a random per-process capability in application and API
paths, strict Host and Fetch Metadata validation, no CORS opt-in, restrictive
CSP/frame/referrer/MIME/cache headers, sanitized logs and errors, provenance
redaction, and no CDN, telemetry, remote fonts, or other network dependency. The
capability URL is sensitive and appears only where intentionally needed for
access, principally the terminal and browser launch.

Official dashboard support inherits Tracky's current Cargo Dist matrix: macOS
Apple Silicon, macOS Intel, and Linux x86-64 GNU. Browser opening is best-effort
and the printed URL is always the fallback. Verification must set tested minimum
Safari, Firefox, and Chromium versions and retain useful semantic/table content
when graphical enhancement fails. Windows and mobile browsers are not supported
MVP platforms even if the responsive page happens to work there.

Superseded on 2026-07-22: the active Cargo Dist and dashboard support matrix no
longer includes macOS Intel. The paragraph above records the decision as adopted
at the time.

Explicitly reject Ratatui as the analytics surface, `file://` snapshots as the
dashboard or launch fallback, daemons, `tiny_http`, a bespoke HTTP/security
layer, frontend frameworks, Vite/TypeScript, Rust/Wasm, external runtime assets,
automatic refresh, JavaScript-owned financial aggregation, generic query/export
APIs, dashboard mutations, invented cross-currency conversion, non-loopback
binding, and external network dependencies. Reconsider Vite/TypeScript only if
demonstrated frontend complexity makes framework-free JavaScript less
maintainable. Any future static export must be a separate explicit command with
its own privacy and overwrite contract.
