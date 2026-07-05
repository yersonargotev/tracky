# PDF import agent workflow

This guide describes the current CLI/JSON path for Tracky's review-first PDF import milestone. It is intended for agents and humans who need to inspect a protected statement, import reviewable candidates, and explicitly accept or reject them without bypassing provenance or auditability.

## Guardrails

- `tracky pdf inspect` is read-only. It returns transient candidate-shaped JSON and does not write SQLite data.
- `tracky import pdf` writes a `SourceDocument`, `ImportBatch`, `Provenance`, duplicate markers, and **candidate transactions** only. It must not create canonical transactions.
- Canonical transactions appear only after an explicit `tracky candidates accept` action.
- Do not drop or hide provenance when reviewing candidates. Accepted and rejected decisions must remain auditable through the source document, import batch, parser/extractor evidence, and candidate id.
- Do not use real PDFs, unredacted account data, passwords, emails, addresses, counterparties, long identifiers, or full amounts as committed fixtures or examples.

## Runtime password handling

PDF passwords are document credentials supplied only at runtime. Tracky can read a password from an environment variable named by `--password-env`; the command records only the credential source, not the credential value.

```bash
export TRACKY_SAMPLE_PDF_PASSWORD='runtime-only-secret'
tracky pdf inspect ~/statements/redacted-sample.pdf \
  --password-env TRACKY_SAMPLE_PDF_PASSWORD \
  --json
```

The JSON should report a credential source such as `env` under `extractor_status.credential_source`. It must not print or persist the password. If the environment variable is missing, Tracky returns stable error JSON instead of requiring agents to parse prose.

## Happy path

### 1. Inspect the PDF without writing storage

Use `pdf inspect` first to check extraction/parser status and redacted candidate previews.

```bash
tracky pdf inspect ~/statements/redacted-sample.pdf \
  --password-env TRACKY_SAMPLE_PDF_PASSWORD \
  --json > /tmp/tracky-inspect.json
```

Agent-oriented checks:

```bash
jq '.ok, .schema_version, .extractor_status.status, .parser_status.status' /tmp/tracky-inspect.json
jq '.candidates[] | {id, status, duplicate_status, posted_date, amount_minor, currency, provenance}' /tmp/tracky-inspect.json
```

Treat `candidates[]` from inspect as previews only. They are useful for validation and review planning, but they are not stored records and do not affect canonical finance data.

### 2. Import candidates into SQLite

After inspect output looks usable, run `import pdf` against a local SQLite database.

```bash
tracky import pdf ~/statements/redacted-sample.pdf \
  --db /tmp/tracky-review.sqlite \
  --password-env TRACKY_SAMPLE_PDF_PASSWORD \
  --json > /tmp/tracky-import.json
```

Expected successful output includes:

- `source_document` with document hash and `document_duplicate_status`.
- `import_batch` with candidate/error/duplicate counts.
- `candidates[]` with stored candidate ids.
- `provenance` on each candidate, including source document, parser/extractor metadata, page/row evidence, confidence, and redacted evidence text.

Import remains review-first: candidates are stored for review, but no canonical transactions are created by this command.

### 3. List candidates for a batch or status

Use the import batch id from the import response to drive review.

```bash
BATCH_ID=$(jq -r '.import_batch.id' /tmp/tracky-import.json)
tracky candidates list \
  --db /tmp/tracky-review.sqlite \
  --import-batch-id "$BATCH_ID" \
  --json > /tmp/tracky-candidates.json
```

Useful filters:

```bash
tracky candidates list --db /tmp/tracky-review.sqlite --status pending_review --json
tracky candidates list --db /tmp/tracky-review.sqlite --status possible_duplicate --json
```

### 4. Accept or reject explicitly

Accept only candidates that have been reviewed and whose provenance/evidence still supports the transaction.

```bash
tracky candidates accept --db /tmp/tracky-review.sqlite cand_REDACTED --json
```

Reject candidates that should not become canonical, while preserving audit history.

```bash
tracky candidates reject --db /tmp/tracky-review.sqlite cand_REDACTED --json
```

Accepting a candidate creates or links a canonical transaction and keeps the trace back to the candidate, source document, import batch, and provenance. Rejecting updates candidate state without deleting the evidence trail.

## Status and duplicate interpretation

Candidate statuses:

| Status | Meaning | Agent action |
| --- | --- | --- |
| `pending_review` | Stored candidate with no known duplicate warning. | Review evidence before accepting or rejecting. |
| `possible_duplicate` | Candidate resembles another candidate or canonical transaction. | Do not auto-accept; compare provenance, fingerprint, date, amount, account, and description. |
| `accepted` | Candidate was explicitly accepted. | Treat as reviewed; do not accept again. |
| `rejected` | Candidate was explicitly rejected. | Preserve for audit; do not delete as cleanup. |

Duplicate statuses:

| Status | Meaning |
| --- | --- |
| `not_checked` | Duplicate detection did not run. |
| `unique` | No match was found. |
| `possible_duplicate` | A near or similar match needs human/agent review. |
| `exact_duplicate` | Normalized fingerprint matched an existing candidate or canonical transaction. |

Source document duplicate status is separate from transaction-level duplicate status. A `duplicate_source_document` response means the same file hash was already imported; Tracky should avoid creating another import batch or another copy of all candidates. Transaction-level `possible_duplicate` means the document may be new, but one or more movements resemble existing records and require review.

## Current limits and non-goals

The current milestone is CLI/JSON-first. Do not document or assume these as available in this slice:

- Full TUI review workflow.
- MCP server or MCP-specific tool wrapper.
- AI fallback for unsupported PDFs.
- Password storage, credential vault integration, or persisted document credentials.
- Import-side canonical transaction creation.
- Sensitive PDF fixtures or committed real financial data.

## Next likely slices

After the CLI path is documented and usable, likely follow-up slices are:

1. Keep the user-facing command reference in [`README.md`](../../README.md) aligned with this workflow and the JSON contract.
2. Improve candidate review ergonomics without changing the review-first contract, such as safer batch summaries or clearer duplicate comparison JSON.
3. Add broader parsers or fixture coverage using redacted row shapes only, not real PDFs.
4. Expand institution/account resolution conservatively at review time, keeping unresolved hints explicit.
5. Consider TUI or MCP wrappers only after the stable CLI JSON workflow remains the source of truth.

## Reference documents

- Domain glossary: `CONTEXT.md`
- Review-first ADR: `docs/adr/0001-review-first-import.md`
- SQLite ADR: `docs/adr/0002-sqlite-canonical-store.md`
- JSON contract: `docs/contracts/review-first-pdf-import-json.md`
- Milestone PRD: `docs/prd/review-first-pdf-import.md`
