# 0006 — Flag possible duplicate candidate transactions

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`

## What to build

Implement transaction-level duplicate detection for imported candidates. Tracky should compute a normalized transaction fingerprint from account/institution hint, date, amount, currency, description, and source/parser context where appropriate. Exact or near matches should become `possible_duplicate` rather than being accepted or discarded automatically.

## Acceptance criteria

- [ ] Candidate imports compute and store a transaction fingerprint or equivalent duplicate key.
- [ ] Importing a candidate that matches an existing candidate/canonical transaction marks it as `possible_duplicate` or records a duplicate marker for review.
- [ ] Duplicate detection does not auto-create, auto-accept, or auto-delete canonical records.
- [ ] The JSON response reports duplicate counts and candidate duplicate status.
- [ ] Tests cover exact document deduplication separately from transaction-level possible duplicates.

## Blocked by

- `0005-add-import-pdf-persistence.md`
