# 0001 — Define the review-first import JSON contract

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`

## What to build

Define Tracky's stable CLI JSON contract for read-only PDF inspection and review-first import outputs. The contract should document source documents, extractor/parser status, candidate transactions, provenance, duplicate signals, redaction expectations, and error shape from the user's/agent's perspective.

This issue is documentation-first: no parser or SQLite implementation is required yet. The output should be precise enough that later issues can implement and test against it.

## Acceptance criteria

- [ ] A contract document exists under `docs/` and describes the JSON shape for `pdf inspect` and `import pdf`.
- [ ] The contract includes `CandidateTransaction`, `SourceDocument`, `ImportBatch`, `Provenance`, extractor/parser status, and duplicate status concepts.
- [ ] Candidate statuses include `pending_review`, `possible_duplicate`, `accepted`, and `rejected`.
- [ ] The contract specifies that imports create candidates only and never canonical transactions directly.
- [ ] The contract specifies runtime-only password handling and no password persistence.
- [ ] The contract includes stable error categories for extractor failure, parser failure, validation failure, and duplicate source document.
- [ ] The contract uses domain vocabulary from `CONTEXT.md` and respects the review-first ADR.

## Blocked by

None - can start immediately.
