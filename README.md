# Tracky

Tracky 0.2.0 is a local-first finance tracker with a review-first CLI and a supported, read-only local analytics dashboard.

## Install

Install the latest release with Homebrew:

```bash
brew install yersonargotev/tap/tracky
```

Prebuilt archives and checksums for supported platforms are also available from
[GitHub Releases](https://github.com/yersonargotev/tracky/releases). Supported packages target
Apple Silicon macOS and x86-64 glibc Linux. The TUI remains intentionally deferred.

## Local analytics dashboard

Start the dashboard against an existing Tracky database:

```bash
tracky dashboard --db ~/.local/share/tracky/tracky.sqlite
```

Tracky prints a capability-bearing `http://127.0.0.1` URL after the listener is
ready and opens the default browser. Use `--no-open` on a headless machine or
when browser launch fails, then open the printed URL yourself. Optional
`--start-date`, `--end-date`, and `--currency` flags choose the initial inclusive
period and one explicit ISO currency; the dashboard never converts or combines
currencies.

The dashboard is local and read-only: it binds only to loopback, serves embedded
assets from the Tracky process, makes no external network request, writes no
runtime assets, and never creates, migrates, or mutates the selected database.
If an older Tracky database is rejected, back it up and run `tracky database
upgrade --db PATH` before trying again. Stop the foreground server with Ctrl-C.

Tracky 0.2.0 supports Safari 26.0+, Firefox 153 ESR+, and Chromium/Chrome 150+
on the packaged desktop targets above. See the [dashboard guide](docs/dashboard.md)
for filters, privacy boundaries, database recovery, and troubleshooting.

## Review-first PDF workflow

Tracky's PDF commands are designed so extraction can be aggressive without corrupting canonical finance data. Imported movements become **candidate transactions** first; they do not affect canonical reports, balances, categories, transfers, or income until an explicit review action accepts them.

### 1. Inspect a PDF without writing storage

```bash
tracky pdf inspect ~/statements/redacted-sample.pdf \
  --password-env TRACKY_SAMPLE_PDF_PASSWORD \
  --json
```

`pdf inspect` is read-only. It returns document, extractor, parser, candidate preview, duplicate, and provenance-shaped JSON, but writes no SQLite rows.

The generic PDF workflow supports Nequi wallet and RappiCard statements and
content-detects Nu credit-card statements without trusting their filename. Nu
card purchases, fees, and interest remain reviewable `card_charge` candidates;
payments are `card_payment`; credits, reversals, and refunds keep their own
explicit semantic hints so they are not silently treated as income. Nu Cuenta
mixed investment statements continue to use `investment-documents`, not this
generic path.

### 2. Import reviewable candidates

```bash
tracky import pdf ~/statements/redacted-sample.pdf \
  --db /tmp/tracky-review.sqlite \
  --password-env TRACKY_SAMPLE_PDF_PASSWORD \
  --json
```

`import pdf` persists the source document, import batch, provenance, duplicate markers, and candidate transactions. It must never create canonical transactions directly.

### 3. Review candidates explicitly

```bash
tracky candidates list \
  --db /tmp/tracky-review.sqlite \
  --import-batch-id batch_REDACTED \
  --json

tracky candidates reject --db /tmp/tracky-review.sqlite cand_REDACTED --json
```

Use the typed accept command for every finance candidate: `accept-income` requires an income source and kind, `accept-expense` requires category lines, `accept-investment` records an outflow as capital pending allocation, and `accept-transfer-pair` requires an explicit validated pair. The legacy `candidates accept` command refuses typed shapes; `candidates reject` updates review state without deleting provenance or evidence.

### Review large batches safely

```bash
tracky candidates batch-summary --db /tmp/tracky-review.sqlite \
  --import-batch-id batch_REDACTED --largest-limit 20 --json
tracky candidates compare-duplicate cand_REDACTED \
  --db /tmp/tracky-review.sqlite --json
tracky candidates suggest-actions --db /tmp/tracky-review.sqlite \
  --import-batch-id batch_REDACTED --json
```

These three commands are strictly read-only. Suggestions are deterministic and explain their fingerprint or structured transfer evidence; transfer suggestions include valid pairs when either side belongs to the selected batch, even when the candidates came from different import batches. They never apply themselves. Apply only explicit candidate ids, preferably after a dry run:

```bash
tracky candidates apply-actions --db /tmp/tracky-review.sqlite \
  --action reject-duplicate:cand_DUPLICATE_REDACTED \
  --action accept-transfer-pair:cand_FROM_REDACTED:cand_TO_REDACTED \
  --dry-run --json
```

Without `--dry-run`, Tracky validates the complete action set using the individual reject/transfer rules and commits all actions atomically. Any failed action leaves every candidate unchanged.

## Canonical finance reports

Summarize an inclusive date range after review:

```bash
tracky reports summary --db /tmp/tracky-review.sqlite \
  --start-date 2026-06-01 --end-date 2026-06-30 --json
```

The stable JSON report groups totals by currency and includes income, positive expense magnitudes, net cash flow, categories, income sources, and excluded transfer/card-payment totals. Candidate transactions never affect the report until accepted; rejected and still-pending candidates remain audit data only.

## Safety guardrails

- Supply PDF passwords only at runtime, such as with `--password-env`; Tracky records the credential source, not the credential value.
- Do not commit real PDFs, document credentials, account numbers, emails, addresses, counterparties, long identifiers, or unredacted financial data as fixtures or examples.
- Treat `possible_duplicate` and `exact_duplicate` signals as review prompts. Tracky flags possible duplicates; it does not auto-merge, auto-accept, or auto-delete them.
- Batch suggestions are not persisted approvals. `apply-actions` requires explicit candidate ids and never accepts a suggestion silently.
- Reports count each accepted transfer pair once and never classify its balancing canonical legs as income or expense.
- Use redacted examples and synthetic identifiers in documentation and tests.

## Reference docs

- Dashboard guide and support matrix: [`docs/dashboard.md`](docs/dashboard.md)
- Dashboard release evidence contract: [`docs/dashboard-evidence.md`](docs/dashboard-evidence.md)
- Agent/human PDF workflow: [`docs/agents/pdf-import-workflow.md`](docs/agents/pdf-import-workflow.md)
- JSON contract: [`docs/contracts/review-first-pdf-import-json.md`](docs/contracts/review-first-pdf-import-json.md)
- Domain glossary: [`CONTEXT.md`](CONTEXT.md)
- Review-first ADR: [`docs/adr/0001-review-first-import.md`](docs/adr/0001-review-first-import.md)
- Local issue tracker: [`docs/issues/README.md`](docs/issues/README.md)
