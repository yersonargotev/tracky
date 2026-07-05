# 0007 — Add CLI review actions for candidate transactions

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`

## What to build

Add minimal CLI commands to list candidates from an import batch and explicitly accept or reject individual candidates. Accepting a candidate should create or link to a canonical transaction while preserving provenance. Rejecting should update candidate state without deleting audit history.

This is still CLI-first; no TUI is required.

## Acceptance criteria

- [ ] A command can list candidate transactions by import batch/status as machine-readable JSON.
- [ ] A command can accept a pending/possible-duplicate candidate and create a canonical transaction.
- [ ] A command can reject a candidate and preserve its provenance/audit trail.
- [ ] Accepted candidates retain a link from canonical transaction back to provenance/source document/import batch.
- [ ] The commands prevent accepting the same candidate twice.
- [ ] Tests prove import remains review-first: canonical transactions appear only after explicit accept.

## Blocked by

- `0005-add-import-pdf-persistence.md`
- `0006-flag-possible-duplicate-candidates.md`
