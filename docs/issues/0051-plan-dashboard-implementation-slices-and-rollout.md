# 0051 — Plan dashboard implementation slices and rollout

Labels: `wayfinder:grilling`

Status: closed

Assignee: yersonargotev

## Parent map

- [Wayfind the local analytics dashboard](0045-wayfind-local-analytics-dashboard.md)

## Question

How should the accepted product, architecture, read model, information
architecture, security posture, and verification contract be divided into
dependency-ordered implementation slices that preserve a runnable tracer bullet,
keep read-only and security invariants testable at every step, and define the
final rollout and documentation boundary?

Produce the implementation-ready specification and slice order that completes
this map; do not implement the dashboard or create implementation tickets during
this planning decision.

## Blocked by

- [Define the dashboard verification and release contract](0050-define-dashboard-verification-and-release-contract.md)

## Resolution

Implement the dashboard through seven serial, dependency-ordered slices numbered
0 through 6. Each slice is one separately reviewable and revertible pull request.
It may touch only its declared boundary, must leave `main` green and the Tracky
binary runnable, and permanently adds its tests and evidence to all later gates.
Do not begin the next slice until its blocker is integrated. Do not combine
opportunistic refactors, weaken a threshold, or move the dashboard-free baseline
in the same change that would benefit from doing so.

Keep `tracky dashboard` directly invocable for development and CI after it first
exists, but hidden from general Clap help until the final rollout. No intermediate
slice is a public preview or a releasable subset of the accepted dashboard.

### Slice 0 — Freeze the evidence foundation

Before changing dashboard code, dependencies, or packaged output, record the
immutable dashboard-free Cargo Dist baseline required by
[Define the dashboard verification and release contract](0050-define-dashboard-verification-and-release-contract.md):
commit, Rust toolchain, lockfile hash, targets, executable and archive byte counts,
archive contents, and reproducible commands. Preserve the artifacts and hashes as
durable CI evidence.

Add the manually authored deterministic conformance corpus and expected oracles,
the schema and validator for `dashboard-verification.json`, pinned `cargo-deny`
policy, dependency/license inventory generation, asset/dependency size checks,
and fast PR-gate entry points. Expected financial results must not be generated
by production calculations. This slice changes no product behavior and passes the
existing formatting, tests, strict Clippy, and release build before it closes.

### Slice 1 — Deliver the secure monthly tracer bullet

First make schema compatibility explicit through SQLite's native markers.
`application_id` identifies a Tracky database and monotonic `user_version = 2`
identifies the first complete dashboard-compatible generation. The normal
writable migration path sets both only after every migration succeeds. Add
`tracky database upgrade --db PATH` as the named recovery path: it validates a
recognizable legacy Tracky schema before upgrading and refuses unrelated SQLite
files. The dashboard accepts only the expected application identity and a schema
generation supported by that binary; it never creates or migrates a database.

Introduce one shared strict read-only open-and-validate seam and caller-owned
read transaction. It must use immutable/read-only SQLite access plus defensive
query-only behavior, validate required capabilities, sanitize failures, and leave
the database, journal sidecars, bytes, rows, schema, and timestamps unchanged.
Existing canonical CLI reports must retain their semantics through the shared
query seams.

Then add the hidden `tracky dashboard` command and the first real vertical
financial tracer: default inclusive dates from the first day eleven months before
the current month through today, optional `--from` and `--to` startup overrides,
the lexicographically first available ISO currency by default, and an optional
`--currency CODE` override. Tests inject the clock. An unknown requested currency
fails before a listener exists; a database with no currencies renders the valid
empty state. The tracer calculates exact monthly income, consumption expense,
savings/net cash flow, and investment contributions for that scope and renders
the values and a semantic table into the initial HTML in Rust. It does not expose
a provisional `tracky.dashboard.v1` JSON route.

The first listener-bearing code must already implement the complete security and
lifecycle boundary: current-thread Tokio/Axum, blocking SQLite isolation, literal
`127.0.0.1:0`, an independent capability with at least 128 bits of entropy,
strict Host and Fetch Metadata checks, sanitized errors/logs, all required
headers, no CORS, no external requests or runtime assets, bounded shutdown, and
non-fatal browser-open failure. Permit only `GET` on exact application, embedded
asset, and later API routes. Reject `HEAD`, `OPTIONS`, mutating methods, unknown
paths, and traversal without financial data and with the defensive headers;
advertise `Allow: GET` where applicable. There is no public health route.
Coordinate readiness internally and print the capability URL only after the
listener accepts connections.

Keep HTML rendering in Rust so exact content survives absent JavaScript. Fixed
embedded CSS and JavaScript provide progressive enhancement only; Rust remains
the sole financial authority. This slice closes only after the minimal exact
tracer, adversarial HTTP matrix, decoy leak checks, database immutability,
startup failures, concurrent isolation, browser fallback, and signal cleanup all
pass.

### Slice 2 — Complete finance, filters, and canonical drill-down

Build the final presentation-neutral resource envelope and complete its finance
and transaction projections: inclusive monthly zero-filling, checked arithmetic,
exact string transport, income/expense/savings/contribution semantics, category
and account breakdowns, and currency-aware stable cursor pagination over
canonical rows and expense lines. Reuse the canonical finance, transaction,
registry, and reporting seams so dashboard values and corresponding CLI reports
agree exactly.

Currency always has exactly one selection. Account and category filters are
multi-select and initially contain every compatible value. Changing currency
clears incompatible selections. Category filters affect expense measures and
expense drill-downs only. An empty account or category selection is an explicit
filter-empty result, never an alias for all. Compose all filters in Rust and never
aggregate unlike currencies.

Define and conformance-test the final `tracky.dashboard.v1` envelope here, but do
not enable an externally reachable partial aggregate route. The versioned route
becomes available only when Slice 3 completes every mandatory investment and
alert section. Slice 2 closes on the manual corpus, CLI parity, numeric boundary,
filter-composition, zero-fill, deterministic-pagination, and immutability gates.

### Slice 3 — Complete investments and enable the v1 resources

Make consolidated investment reporting filter-aware before aggregation and add
the accepted as-of projections: historical-cost positions, exact quantities,
separate cost and observed-valuation currencies, dated snapshot selection,
freshness, pending allocation, underlying reconciliation state, and structured
alerts. Allocation-only positions are not the as-of authority, and missing or
stale valuation is never converted into zero.

After the full aggregate and canonical drill-down responses pass the exact corpus
and canonical report parity, enable their capability-bearing
`tracky.dashboard.v1` routes. This slice closes on fresh, stale, unavailable,
pending, reconciled, incompatible, multi-currency, and checked-decimal cases with
no provenance or raw storage details exposed.

### Slice 4 — Complete the Monthly ledger experience

Implement the accepted single-scroll Monthly ledger in its fixed order: persistent
scope/header; explicit single-currency selector; income, consumption expense,
savings, and contribution summary; monthly trend plus exact table; category and
account activity; contextual reconciliation/freshness alerts; and the dense
as-of investment table. Filters live in one compact panel. Chart, breakdown, and
position activation open the same read-only paginated canonical drawer.

Browser code owns only rendering and ephemeral interaction. Dates, currency,
accounts, categories, drawer state, and pagination never enter query strings,
fragments, cookies, `localStorage`, or `sessionStorage`. Internal refresh may
preserve applicable state, but a full page reload restores the startup scope and
browser Back is not repurposed for internal navigation. No JavaScript calculation
may define a financial value. The embedded HTML/CSS/JavaScript budget remains at
most 250 KiB uncompressed, and the prototype remains decision evidence rather
than production source.

Close this slice on exact table/API agreement, supported automated browser flows,
keyboard and focus behavior, responsive order, zero external requests, and the
local interaction budget. The server-rendered initial table remains useful when
JavaScript or graphical enhancement fails.

### Slice 5 — Complete refresh, state, and accessibility

Add the one initial snapshot plus explicit manual refresh contract without
polling or file watching. A successful refresh preserves compatible filters and
navigation, but closes a drawer whose canonical cursor no longer belongs to the
new snapshot. A failed refresh retains the last good snapshot and adds a
sanitized stale/error indication and retry path.

Complete and distinguish valid empty ledger, filter-empty, unavailable metric,
stale observation, incompatible schema, startup/initial-read failure, and refresh
failure behavior. Implement live announcements, restored visible focus, reduced
motion, no color-only meaning, exact chart descriptions and tables, 200-percent
zoom, 320-CSS-pixel reflow, contrast, and pointer targets. Close only after the
full browser-state matrix, zero applicable axe violations, refresh immutability,
and the automated portions of WCAG 2.2 AA pass; retain the named manual assistive
technology checks for the release candidate.

### Slice 6 — Prove distribution and perform the supported rollout

Tune and verify the real native Cargo Dist artifacts without revising the frozen
baseline or accepted budgets. Run every target's packaged security and lifecycle
matrix, 100-cycle leak checks, latency/resource measurements, dependency/license
policy, exact archive allowlist and notices, checksum/architecture/permission
checks, sandboxed extracted and shell-installer tests, ephemeral Homebrew test,
paths containing spaces and Unicode, minimum and latest supported browsers, and
the manual accessibility matrix.

Generate the schema-valid `dashboard-verification.json` and human-readable
Markdown rendering with all required identities, hashes, measurements, commands,
results, and maintainer approval. Publication must depend technically on that
complete passing proof.

Only after the implementation and PR gates pass does the rollout pull request
unhide `tracky dashboard` and update the README, user-facing command reference,
supported platform/browser matrix, privacy boundary, `database upgrade` recovery,
`--no-open` fallback, troubleshooting, and release notes. The first public
supported dashboard ships in Tracky `0.2.0`; it is not labelled beta or preview.
Attach both evidence formats permanently to the release.

No implementation ticket is created by this planning resolution. Hosted or
remote service, mutations, currency conversion, static export, frontend build
tooling, runtime assets, Windows/mobile support, and the pending TUI work remain
outside this map.
