# Embedded local-web architecture for Tracky

Research date: 2026-07-21

## Question

Which server, asset, frontend, browser-launch, and lifecycle design best fits a
single-binary, offline, loopback-only, read-only Tracky dashboard?

## Recommendation

Use a **foreground live server in the existing `tracky` process**:

- Axum 0.8 on a Tokio current-thread runtime, with only HTTP/1, JSON, query,
  runtime, networking, signal, and macro features enabled;
- a listener created explicitly on `127.0.0.1:0`, followed by discovery of the
  assigned port;
- a small, framework-free HTML/CSS/JavaScript frontend whose fixed files are
  compiled into the Rust binary with `include_bytes!`;
- JSON `GET` endpoints over Tracky's strict read-only query layer, with blocking
  SQLite work isolated via `tokio::task::spawn_blocking`;
- `webbrowser` with its `hardened` feature to open the HTTP URL, plus an always
  printed URL and `--no-open` fallback;
- foreground ownership and graceful Ctrl-C/SIGTERM shutdown. Do not tie server
  lifetime to a browser tab and do not expose a browser-callable shutdown route.

This is more machinery than `tiny_http`, but the cost is bounded and buys typed
routing/query/JSON seams, middleware composition, and a documented graceful
shutdown path. Axum is designed for Tokio/Hyper and its optional features can be
disabled; `axum::serve(...).with_graceful_shutdown(...)` is provided by the
crate itself. [Axum overview and features](https://docs.rs/axum/0.8.9/axum/),
[Axum graceful shutdown](https://docs.rs/axum/0.8.9/axum/serve/struct.WithGracefulShutdown.html)

The live server is the product surface. A self-contained static HTML snapshot
may be valuable later as an **explicit export**, but not as `tracky dashboard`.

## Why this shape fits the repository

Tracky currently releases one Rust binary for Apple Silicon macOS, Intel macOS,
and x86-64 GNU/Linux, using Cargo Dist and a Cargo-only release build. It does
not declare a `rust-version`, and CI follows stable Rust. See
[`Cargo.toml`](../../Cargo.toml) and
[`release.yml`](../../.github/workflows/release.yml).

A no-build frontend keeps those release jobs compatible: the release compiler
sees ordinary files through Rust's `include_bytes!`, which produces a static byte
array at compile time. [Rust `include_bytes!`](https://doc.rust-lang.org/stable/std/macro.include_bytes.html)
There is no Node, package-manager, CDN, font, telemetry, or sidecar requirement
for users or release runners.

Use browser-native ES modules, `fetch`, semantic HTML, CSS, and SVG for the MVP.
Filters, manual refresh, drill-down, and charts do not by themselves justify a
framework or bundler. If the information-architecture prototype later proves
that this is too costly to maintain, Vite plus TypeScript is the next choice,
but its output must be produced and verified before `cargo build`; Vite's build
turns an HTML entry point into a static bundle and its default target has an
explicit modern-browser floor. [Vite production build](https://vite.dev/guide/build),
[Vite static assets](https://vite.dev/guide/assets)

## Alternatives considered

| Concern | Alternative | Evidence and trade-off | Decision |
| --- | --- | --- | --- |
| HTTP server | `tiny_http` plus `ctrlc` | Synchronous and substantially smaller in a minimal probe. It supports ephemeral binds, timed receive, `unblock`, and closes listening sockets when dropped. Routing, query validation, JSON/error responses, security headers, and concurrency policy would be Tracky-owned. [tiny_http `Server`](https://docs.rs/tiny_http/0.12.0/tiny_http/struct.Server.html) | Reject for the dashboard: saved framework weight is not worth growing a bespoke HTTP/security layer. Reconsider only if release-size budgets fail. |
| HTTP server | Axum plus Tokio | Typed extractors deserialize query strings and reject malformed values; JSON responses set their content type; Tower-compatible middleware and first-party graceful shutdown reduce bespoke surface. [Axum `Query`](https://docs.rs/axum/0.8.9/axum/extract/struct.Query.html), [Axum `Json`](https://docs.rs/axum/0.8.9/axum/struct.Json.html) | **Choose**, with minimal features and a current-thread runtime. |
| Embedded assets | fixed `include_bytes!` entries | Standard-library only and embeds in every profile, but every asset and MIME type must be listed. | **Choose** while the asset set is small and stable. |
| Embedded assets | `include_dir!` | Convenient directory lookup, but its own documentation warns that many/large files increase compiler memory, compile time, and binary size. [include_dir compile-time considerations](https://docs.rs/include_dir/0.7.4/include_dir/#compile-time-considerations) | Reserve for a larger generated asset tree. |
| Embedded assets | `rust-embed` | Convenient lookup and optional compression/MIME support, but by default debug builds read from disk while release builds embed; `debug-embed` is needed to remove that profile difference. [rust-embed behavior and features](https://docs.rs/crate/rust-embed/8.12.0) | Reject for the fixed MVP files; the extra abstraction and profile gotcha buy little. |
| Frontend | Vite/TypeScript SPA | Productive once modules and third-party visualizations grow, but adds Node/package-lock/toolchain work to CI and release. No Node runtime is needed after building. | Defer until prototype evidence justifies it. Never invoke npm from `build.rs`. |
| Frontend | Rust/Wasm UI | Preserves Rust as the source language but adds a Wasm target, bindings, loader assets, and another build graph while the server still needs normal HTML/JS integration. | Reject: no dashboard requirement needs Wasm. |
| Browser launch | shelling out to `open`, `xdg-open`, or `start` | Small but Tracky would own platform detection, escaping, and failure behavior. | Reject. |
| Browser launch | `webbrowser` | Documents tested support for macOS, Windows, and Linux/WSL, consistent non-blocking GUI behavior, and a `hardened` feature that rejects non-HTTP(S) URLs. [webbrowser platform and feature documentation](https://docs.rs/webbrowser/1.2.1/webbrowser/) | **Choose**, while treating launch failure as non-fatal. |
| Product surface | generated `file://` snapshot | Can provide offline charts and local filtering with all data embedded, but has no live query/manual-refresh channel. The URL standard deliberately leaves `file:` origins implementation-dependent and recommends an opaque origin when in doubt. [WHATWG URL origin algorithm](https://url.spec.whatwg.org/#origin) | Reject as the dashboard; consider a separately named, explicit export later. |

### Qualified dependency and size probe

A disposable probe on `aarch64-apple-darwin` with Rust 1.93.0, release
optimization, symbol stripping, and **no LTO** produced:

| Probe | Direct dependencies/features | Resolved transitive packages* | Binary |
| --- | --- | ---: | ---: |
| Axum | `axum 0.8` (`http1,json,tokio`, no defaults), `tokio 1` (`macros,net,rt,signal`) | 48 | 1,060,432 bytes |
| synchronous | `tiny_http 0.12`, `ctrlc 3.5` | 17 | 663,056 bytes |

\* Cargo metadata counts the root-excluded resolved package set, including
target-conditioned packages. These numbers demonstrate direction, not Tracky's
incremental cost: Tracky may already share dependencies, its release uses thin
LTO, real handlers add code, and embedded assets contribute approximately their
linked representation. No binary-size claim should be accepted until the real
release targets are measured in CI.

Axum 0.8.5 raised its minimum Rust version to 1.78. Tracky's stable-only CI is
currently compatible but does not promise an MSRV. The implementation spec
should require an explicit `rust-version` at least as high as the complete
dependency graph actually needs and a CI job that verifies it; Cargo defines
`rust-version` as the package's supported compiler version.
[Axum 0.8.5 release note](https://github.com/tokio-rs/axum/releases/tag/axum-v0.8.5),
[Cargo Rust-version reference](https://doc.rust-lang.org/stable/cargo/reference/rust-version.html)

## Runtime contract

1. Parse options and resolve the database path without writing it.
2. Open/validate the database through the separately specified strict read-only
   seam; never migrate on this path.
3. Bind `TcpListener` to the literal IPv4 loopback address and port zero. A
   literal loopback address avoids hostname-resolution ambiguity, and port zero
   avoids races inherent in probing then rebinding a port. RFC 8252 independently
   recommends loopback IP literals and ephemeral ports for native-app listeners.
   [RFC 8252 section 7.3](https://www.rfc-editor.org/rfc/rfc8252.html#section-7.3)
4. Generate a cryptographically random per-process capability (at least 128 bits)
   and place it in the initial URL path and every API path. Do not log it beyond
   the single URL presented to the user.
5. Print the exact `http://127.0.0.1:<port>/<capability>/` URL, then ask
   `webbrowser` to open it unless `--no-open` was supplied. Browser-open failure
   is a warning, not server failure.
6. Serve embedded assets and versioned JSON `GET` endpoints. Run synchronous
   rusqlite/query work in `spawn_blocking`; Tokio documents that ordinary
   blocking work in an async task blocks its executor thread and provides a
   dedicated blocking pool for this purpose.
   [Tokio blocking guidance](https://docs.rs/tokio/1.53.1/tokio/task/#blocking-and-yielding)
7. Remain attached to the terminal. First Ctrl-C (and SIGTERM on Unix) stops new
   work and drains in-flight requests; a short deadline or second Ctrl-C forces
   exit so a stuck browser connection/query cannot hang shutdown. Tokio's
   cross-platform Ctrl-C future replaces the Unix default handler once polled,
   so the explicit second-signal/timeout behavior matters.
   [Tokio `ctrl_c` caveat](https://docs.rs/tokio/1.53.1/tokio/signal/fn.ctrl_c.html)

The browser is an independently managed desktop application, not a child Tracky
can reliably supervise. Tab-close heartbeat shutdown would be vulnerable to
sleeping laptops, throttled background tabs, crashes, and multiple tabs; idle
timeouts also conflict with an on-demand dashboard left open for reference.

## Loopback hardening contract

Loopback binding limits network reach; it is not authorization. Apply all of the
following defense-in-depth rules:

- accept only an exact runtime `Host: 127.0.0.1:<port>` authority and a valid
  per-run capability path; return 404 without either;
- expose only `GET`, `HEAD`, and `OPTIONS` as actually needed; never add a write
  or shutdown endpoint to this server;
- do not emit `Access-Control-Allow-Origin`; browsers only share a cross-origin
  response when the CORS protocol permits it. [WHATWG Fetch CORS model](https://fetch.spec.whatwg.org/#http-cors-protocol)
- reject `Sec-Fetch-Site: cross-site`; permit `none` only for the capability-bearing
  top-level navigation and `same-origin` for application requests. The W3C
  header is specifically designed to let a server reject requests based on
  initiator context. [Fetch Metadata](https://www.w3.org/TR/fetch-metadata/)
- send `Content-Security-Policy: default-src 'none'; script-src 'self';
  style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none';
  form-action 'none'; frame-ancestors 'none'`. CSP defines both fetch controls
  and the independent `frame-ancestors` navigation control.
  [CSP Level 3](https://www.w3.org/TR/CSP3/)
- send `Referrer-Policy: no-referrer`, `X-Content-Type-Options: nosniff`, and
  `Cross-Origin-Resource-Policy: same-origin`;
- send `Cache-Control: no-store` on HTML and API responses. RFC 9111 says a cache
  must not store a response carrying `no-store`, while warning that this is not
  by itself a complete privacy mechanism. [RFC 9111](https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2.2.5)
- never embed provenance details in HTML, asset names, URLs, logs, error pages, or
  browser titles; return the already-redacted contract fields only.

The capability is additional protection against unrelated local pages/processes
guessing an ephemeral port. It is not protection from a process running as the
same OS user that can inspect process arguments, memory, or terminal output; that
threat is outside a loopback web server's security boundary.

## Static snapshot boundary

A single generated HTML file can satisfy a narrower requirement: a point-in-time,
portable, read-only report whose data and scripts are inline. It cannot satisfy
the chosen manual refresh behavior without regenerating/reopening the file, and
it creates a durable copy of sensitive finance data outside SQLite. It also loses
the predictable HTTP origin used by fetch/CSP response headers.

Therefore:

- do not silently write a snapshot as a side effect of `tracky dashboard`;
- if later requested, make it an explicit export command with an output path,
  overwrite confirmation, redaction contract, and a visible “generated at” time;
- do not treat snapshot export as a fallback when browser launch fails—the live
  server URL printed to the terminal is the fallback.

## Verification obligations surfaced for the final specification

- package test: release binary runs with the frontend source directory absent;
- network test: only `127.0.0.1` is listening and an OS-assigned port is used;
- HTTP tests: wrong Host/token, cross-site Fetch Metadata, disallowed methods,
  path traversal, unknown assets, and CORS probes fail closed;
- response tests: CSP, referrer, MIME/nosniff, no-store, and provenance-redaction
  headers/content are exact;
- lifecycle tests: open failure, `--no-open`, first/second Ctrl-C, SIGTERM on Unix,
  in-flight query drain, and database-open failure leave no listener behind;
- release checks on all three Cargo Dist targets: dependency licenses, archive
  contents, startup, asset availability, and real incremental binary size;
- browser checks should name a minimum supported Safari, Firefox, and Chromium
  version after the prototype fixes the JavaScript/CSS feature set; the HTML
  should retain a useful tabular fallback if SVG enhancement fails.

## Remaining risks

- The random capability and strict Host/Fetch-Metadata policy need focused tests;
  small local servers are otherwise easy to under-harden.
- `spawn_blocking` work cannot be cancelled once running; queries must be bounded
  and shutdown must have a deadline.
- Axum/Tokio add more supply-chain and compile surface than a synchronous server;
  only real Tracky target builds can decide whether that violates a release
  budget.
- Framework-free JavaScript minimizes release machinery but may stop being the
  simplest option if the prototype grows into many coordinated interactive
  views. That is the explicit trigger for reconsidering Vite/TypeScript.
