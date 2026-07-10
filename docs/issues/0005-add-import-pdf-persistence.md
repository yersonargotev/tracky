# 0005 — Add `tracky import pdf` persistence for candidates

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`

## What to build

Add the first write path: importing a supported PDF should create a source document record, an import batch, provenance records, and pending candidate transactions in SQLite. It must not create canonical transactions yet.

The command should return machine-readable JSON summarizing what was persisted and any duplicate source-document condition.

## Acceptance criteria

- [x] `tracky import pdf <PDF>` uses the product PDF inspection core and persists the import into SQLite.
- [x] A successful import creates one source document, one import batch, provenance records, and candidate transactions with `pending_review` status unless duplicate logic says otherwise.
- [x] No canonical transactions are created by import.
- [x] Reimporting the exact same source document hash reports a duplicate source document condition instead of silently duplicating all candidates.
- [x] The command emits contract-compatible JSON with ids/counts/status.
- [x] Integration tests verify database side effects using a temporary SQLite database.

## Blocked by

- `0003-add-pdf-inspect-cli-json.md`
- `0004-create-sqlite-review-first-schema.md`

## Reconciliation evidence

Import persistence and integration tests: `src/storage.rs`, `tests/import_pdf_persistence.rs`; introduced by `03ad58c`, with duplicate-source error fixes through `413d4d3` and `b3ee944`.
