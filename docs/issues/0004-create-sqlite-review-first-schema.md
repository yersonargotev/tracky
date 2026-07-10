# 0004 — Create SQLite schema for review-first import

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`

## What to build

Add the initial SQLite schema and migration path for review-first PDF imports. The schema should store source documents, import batches, candidate transactions, provenance, canonical transactions, institutions/accounts, and duplicate/fingerprint data needed for the first import flow.

This issue creates storage only; it does not need to import PDFs yet.

## Acceptance criteria

- [x] SQLite migrations create tables for source documents, import batches, candidate transactions, provenance, canonical transactions, institutions, accounts, and transaction fingerprints or duplicate markers.
- [x] Candidate transactions are separate from canonical transactions.
- [x] Candidate status is constrained to the contract states.
- [x] Source documents store a file hash for exact document deduplication.
- [x] Candidate/provenance data can represent extractor, parser id/version, page, row bbox, redacted evidence, confidence, amount minor units, currency, optional balance, and description.
- [x] Tests can create a temporary database, apply migrations, and insert/read the core review-first records.

## Blocked by

- `0001-define-review-first-import-contract.md`

## Reconciliation evidence

Review-first migrations and temporary-database tests: `migrations/0001_review_first_schema.sql`, `tests/storage_migrations.rs`; introduced by `92455a0`.
