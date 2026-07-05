# 0005 — Add `tracky import pdf` persistence for candidates

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`

## What to build

Add the first write path: importing a supported PDF should create a source document record, an import batch, provenance records, and pending candidate transactions in SQLite. It must not create canonical transactions yet.

The command should return machine-readable JSON summarizing what was persisted and any duplicate source-document condition.

## Acceptance criteria

- [ ] `tracky import pdf <PDF>` uses the product PDF inspection core and persists the import into SQLite.
- [ ] A successful import creates one source document, one import batch, provenance records, and candidate transactions with `pending_review` status unless duplicate logic says otherwise.
- [ ] No canonical transactions are created by import.
- [ ] Reimporting the exact same source document hash reports a duplicate source document condition instead of silently duplicating all candidates.
- [ ] The command emits contract-compatible JSON with ids/counts/status.
- [ ] Integration tests verify database side effects using a temporary SQLite database.

## Blocked by

- `0003-add-pdf-inspect-cli-json.md`
- `0004-create-sqlite-review-first-schema.md`
