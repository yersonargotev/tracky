# 0046 — Compare embedded local-web architectures

Labels: `wayfinder:research`

Status: closed

Assignee: yersonargotev

## Parent map

- [Wayfind the local analytics dashboard](0045-wayfind-local-analytics-dashboard.md)

## Question

Which Rust server, embedded-asset, frontend-build, browser-launch, and process
lifecycle architecture best satisfies Tracky's single-binary, loopback-only,
offline dashboard constraints, and what trade-offs should govern the final
choice?

The research must compare viable alternatives using primary documentation and
repository evidence, including release compatibility, dependency cost, binary
size, cross-platform behavior, loopback hardening, and whether a static snapshot
could meet any requirement more simply than a live local server.

## Blocked by

- None.

## Resolution

Research is captured in
[Embedded local-web architecture for Tracky](../research/embedded-local-web-architecture.md).

The recommended baseline for the later product-contract decision is a live,
foreground Axum 0.8 server on a minimal current-thread Tokio runtime. It should
bind the literal `127.0.0.1:0`, serve versioned read-only JSON endpoints and a
small framework-free HTML/CSS/JavaScript frontend embedded with fixed
`include_bytes!` entries, isolate synchronous SQLite work with
`spawn_blocking`, and shut down gracefully on terminal signals with a bounded
drain period.

The command should print its exact URL and use the hardened `webbrowser` crate
to open it unless `--no-open` is set; launch failure remains non-fatal. The
server contract must combine strict Host validation, a random per-process
capability in application/API paths, Fetch Metadata checks, restrictive CSP and
related privacy headers, no CORS opt-in, `no-store`, and loopback-only binding.

Reject `tiny_http` as the default because its smaller directional footprint
would leave Tracky owning more routing and HTTP-security behavior. Also reject
`file://` snapshots as the dashboard, runtime asset directories, Rust/Wasm,
shell-based browser launching, and a frontend framework/build pipeline for the
MVP. A static snapshot may later be a separate explicit export; Vite and
TypeScript may be reconsidered only if the information-architecture prototype
shows that framework-free code is no longer the simplest maintainable option.

The disposable size probe establishes only direction: the Axum/Tokio probe was
larger than the synchronous alternative. Actual incremental size, dependency
licenses, packaged assets, and startup behavior had to be measured on all three
then-current Cargo Dist targets before release; no probe number was an accepted
product budget. The active matrix no longer includes macOS Intel as of
2026-07-22.
