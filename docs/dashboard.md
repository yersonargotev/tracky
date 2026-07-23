# Tracky dashboard

Tracky 0.2.0 includes a supported, read-only dashboard for local analytical
inspection. It is not a hosted service, beta, or preview.

## Start and stop

```sh
tracky dashboard --db /path/to/tracky.sqlite
tracky dashboard --db /path/to/tracky.sqlite --no-open
tracky dashboard --db /path/to/tracky.sqlite \
  --start-date 2026-01-01 --end-date 2026-12-31 --currency COP
```

The command stays in the foreground and prints its exact URL only after its
random loopback port is accepting connections. The URL contains a per-process
capability; treat it as private financial data. `--no-open` suppresses only the
browser launch. Ctrl-C or SIGTERM stops the server.

Date flags define an inclusive initial range. `--currency` selects one currency;
Tracky neither converts currencies nor presents cross-currency totals. Browser
filters, drill-downs, and refreshes remain within the same canonical read model.

## Supported environments

| Package target | Operating system | Supported browsers |
| --- | --- | --- |
| `aarch64-apple-darwin` | Apple Silicon macOS | Safari 26.0+, Firefox 153 ESR+, Chromium/Chrome 150+ |
| `x86_64-unknown-linux-gnu` | x86-64 glibc Linux | Firefox 153 ESR+, Chromium/Chrome 150+ |

Each release also verifies the latest stable versions available at release cut.
Windows, mobile browsers, remote access, and the TUI are outside the 0.2.0
support contract.

## Privacy and read-only boundary

- The server binds only to literal `127.0.0.1` on an operating-system-assigned
  port and requires the unguessable URL capability.
- HTML, CSS, and JavaScript are embedded in the executable. No Node process,
  frontend checkout, CDN, telemetry, or other external request is used.
- Dashboard startup, reads, refreshes, and drill-downs do not create, migrate,
  or mutate SQLite and do not write runtime assets.
- The full startup URL is intentionally printed for the local user. Other logs
  and browser responses omit the capability, database path, SQL, and private
  provenance.

Do not share the startup URL, screenshots, exported browser data, or a database
containing real financial information.

## Database upgrade and recovery

The dashboard accepts only a recognized Tracky database at the schema generation
supported by the binary. For an older Tracky database:

1. Stop every Tracky process and make a backup copy.
2. Run `tracky database upgrade --db /path/to/tracky.sqlite`.
3. Start `tracky dashboard` again.

An unrelated or incompatible SQLite file is refused. If refresh later fails,
the page keeps its last known-good snapshot and offers Retry. Fix the database or
filesystem problem before retrying; do not replace the file while another writer
is active. Restart the foreground command if recovery is not possible.

## Troubleshooting

- **Browser did not open:** copy the printed URL, or restart with `--no-open`.
- **No URL was printed:** startup validation failed before a listener was left
  behind. Read the sanitized terminal error and verify the path and schema.
- **Currency unavailable:** choose a currency present in the selected period.
- **Empty dashboard:** broaden the dates or filters; valid-empty views do not
  invent zero metrics.
- **Stale or failed refresh:** use Retry after resolving the underlying local
  database problem. The last good snapshot remains visible.
- **Port or process concern:** stop with Ctrl-C and restart. Each instance has an
  independent port and capability and does not own the browser process.

For release verification and permanently attached evidence, see
[`dashboard-evidence.md`](dashboard-evidence.md).
