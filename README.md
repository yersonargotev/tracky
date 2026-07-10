# Tracky

Tracky is a local-first finance tracker. The current implemented path is a CLI/JSON-first, review-first workflow for supported statement PDFs: inspect a document, import reviewable candidate transactions, then explicitly accept or reject candidates.

## Review-first PDF workflow

Tracky's PDF commands are designed so extraction can be aggressive without corrupting canonical finance data. Imported movements become **candidate transactions** first; they do not affect canonical reports, balances, categories, transfers, or income until an explicit review action accepts them.

### 1. Inspect a PDF without writing storage

```bash
tracky pdf inspect ~/statements/redacted-sample.pdf \
  --password-env TRACKY_SAMPLE_PDF_PASSWORD \
  --json
```

`pdf inspect` is read-only. It returns document, extractor, parser, candidate preview, duplicate, and provenance-shaped JSON, but writes no SQLite rows.

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

tracky candidates accept --db /tmp/tracky-review.sqlite cand_REDACTED --json
tracky candidates reject --db /tmp/tracky-review.sqlite cand_REDACTED --json
```

Only `candidates accept` creates or links a canonical transaction, and it preserves the audit path back to the candidate, provenance, source document, and import batch. `candidates reject` updates review state without deleting provenance or evidence.

### Review large batches safely

```bash
tracky candidates batch-summary --db /tmp/tracky-review.sqlite \
  --import-batch-id batch_REDACTED --largest-limit 20 --json
tracky candidates compare-duplicate cand_REDACTED \
  --db /tmp/tracky-review.sqlite --json
tracky candidates suggest-actions --db /tmp/tracky-review.sqlite \
  --import-batch-id batch_REDACTED --json
```

These three commands are strictly read-only. Suggestions are deterministic and explain their fingerprint or structured transfer evidence, but they never apply themselves. Apply only explicit candidate ids, preferably after a dry run:

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

- Agent/human PDF workflow: [`docs/agents/pdf-import-workflow.md`](docs/agents/pdf-import-workflow.md)
- JSON contract: [`docs/contracts/review-first-pdf-import-json.md`](docs/contracts/review-first-pdf-import-json.md)
- Domain glossary: [`CONTEXT.md`](CONTEXT.md)
- Review-first ADR: [`docs/adr/0001-review-first-import.md`](docs/adr/0001-review-first-import.md)
- Local issue tracker: [`docs/issues/README.md`](docs/issues/README.md)
