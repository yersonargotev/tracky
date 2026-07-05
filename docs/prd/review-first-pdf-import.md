# PRD: Review-first PDF import contract and storage

## Problem Statement

Tracky has proven that protected Nequi and Rappi PDFs can be opened and parsed with `pdf_oxide`, but the parser still lives inside a spike binary. The next product problem is to turn that evidence into a stable review-first import flow: PDFs should produce candidate transactions with provenance, confidence, and duplicate evidence without immediately mutating canonical financial records.

The user needs a local-first Rust CLI/TUI finance tracker that can ingest bank and card statement PDFs, expose machine-readable JSON for Codex/Claude/OpenCode, store reviewable candidates in SQLite, and only promote reviewed items into canonical transactions.

## Solution

Build the first product-grade import path around a stable `CandidateTransaction` contract and SQLite storage. The implementation should start with a machine-readable `tracky pdf inspect` command that reuses the proven PDF extraction/parser behavior without writing to the database, then add a `tracky import pdf` path that persists source documents, import batches, provenance, and candidate transactions for later review.

The first iteration should remain CLI/JSON-first. It should not build the full TUI, MCP server, AI fallback, analytics dashboards, asset versioning, or password storage. Passwords remain runtime-only via env, prompt, or CLI-provided password source.

## User Stories

1. As a Tracky user, I want to inspect a protected PDF from the CLI, so that I can see whether Tracky can extract useful movement candidates before importing anything.
2. As a Tracky user, I want inspect output as stable JSON, so that Codex, Claude Code, OpenCode, or scripts can reason about the extracted data.
3. As a Tracky user, I want PDF passwords supplied only at runtime, so that Tracky does not store document secrets.
4. As a Tracky user, I want each PDF import to record a source document hash, so that exact reimports can be detected.
5. As a Tracky user, I want imported movements to become candidate transactions first, so that inaccurate PDF extraction cannot corrupt my canonical records.
6. As a Tracky user, I want every candidate transaction to include provenance, so that I can trace it back to file, page, extractor, parser, evidence text, and confidence.
7. As a Tracky user, I want candidates to include redacted evidence in JSON output, so that agent workflows can inspect data without leaking sensitive details unnecessarily.
8. As a Tracky user, I want raw/provenance evidence stored locally where appropriate, so that I can audit why a candidate was created.
9. As a Tracky user, I want Nequi and Rappi PDFs handled by deterministic parsers when possible, so that imports are repeatable.
10. As a Tracky user, I want parser status reported per file, so that partial or failed extraction is visible instead of silent.
11. As a Tracky user, I want Rappi rows with multiple money cells handled consistently, so that selected amounts match the transaction movement rather than an ancillary column.
12. As a Tracky user, I want Nequi rows with amount and balance cells handled consistently, so that candidate amount and optional balance are separated.
13. As a Tracky user, I want candidate statuses like pending review, accepted, rejected, and possible duplicate, so that the review workflow can be explicit.
14. As a Tracky user, I want duplicate candidates flagged rather than auto-merged, so that I can decide whether similar transactions are true duplicates.
15. As a Tracky user, I want import batches, so that I can review candidates from a single PDF/import run together.
16. As a Tracky user, I want institutions and accounts represented explicitly, so that Nequi wallet movements and Rappi card movements do not get mixed together.
17. As a Tracky user, I want canonical transactions separate from candidate transactions, so that only reviewed financial records affect reports.
18. As a Tracky user, I want accepted candidates to preserve provenance on the canonical transaction, so that future audits remain possible.
19. As a Tracky user, I want transfers between my own accounts to be representable, so that credit-card payments from Nequi to Rappi are not counted as expenses.
20. As a Tracky user, I want income sources modeled independently from accounts, so that payroll can move accounts over time without losing semantic meaning.
21. As an agent, I want a documented JSON contract, so that I can generate, validate, and review import outputs without reading the Rust implementation.
22. As an agent, I want stable command behavior and exit/error shape, so that automation can distinguish extractor failure, parser failure, duplicate document, and validation errors.
23. As a developer, I want the spike code moved behind product seams, so that future parser tests do not depend on the spike binary.
24. As a developer, I want high-level CLI tests around JSON behavior, so that refactors preserve external behavior.
25. As a developer, I want SQLite migrations to encode the review-first model, so that storage and domain vocabulary stay aligned.
26. As a developer, I want parser fixtures based on redacted row shapes, so that parser behavior can be tested without committing sensitive PDFs.
27. As a developer, I want source PDFs ignored by git, so that sensitive assets never become repo artifacts.
28. As a developer, I want the first implementation to be narrow and vertical, so that each step is demoable without waiting for the entire finance app.

## Implementation Decisions

- Use a CLI JSON contract before MCP. MCP can later wrap the same core command behavior.
- Keep import review-first: PDF extraction creates candidate transactions, never canonical transactions directly.
- Use SQLite as the initial canonical store and migration target.
- Use `pdf_oxide` as the first extractor for product code, with `pdfium-render` retained as fallback/comparator only when needed.
- Preserve runtime-only PDF passwords via environment variables, prompt, or future CLI password flags; do not store passwords.
- Define a product-level `CandidateTransaction` concept with at least: candidate id, import batch id, source document id, institution/account hints, date, description, amount in minor units, currency, optional balance in minor units, confidence, status, duplicate status, and provenance reference.
- Define provenance as first-class data with extractor name, parser id/version, source file identity, page, row bbox when available, redacted evidence, and optional raw evidence policy.
- Define candidate statuses as a small explicit state machine: `pending_review`, `possible_duplicate`, `accepted`, `rejected`.
- Define document deduplication by source document hash and transaction deduplication by normalized transaction fingerprint.
- Keep institution/account resolution conservative in the first slice: infer institution from parser/PDF source, but allow unresolved account hints until review.
- Move parser/extractor logic out of the spike binary into product modules before adding storage writes.
- Keep product commands narrow: first `pdf inspect` for read-only JSON, then `import pdf` for persistence, then review/list/accept behavior.
- Keep sensitive sample PDFs outside git. Tests should use redacted text/row fixtures or generated fixture data, not the real `assets/*.pdf` files.

## Testing Decisions

- Prefer testing at the CLI JSON seam because this is the contract agents and users will rely on.
- Test the product parser using redacted row fixtures extracted from the spike, not sensitive PDFs.
- Test SQLite migrations and repository behavior with temporary database files.
- Test deduplication through behavior: importing the same source document twice should report an exact duplicate; importing similar candidate rows should mark possible duplicates for review.
- Test review-first behavior by proving import creates candidates and does not create canonical transactions until an explicit accept action exists.
- Test errors through stable JSON output and non-zero exit behavior where appropriate.
- Avoid tests that assert private helper structure; external CLI behavior, storage effects, and domain state transitions matter most.

## Out of Scope

- Full TUI review workflow.
- MCP server.
- AI-assisted extraction fallback for unknown PDF formats.
- Full analytics/reporting dashboard.
- Password storage or credential vault integration.
- Advanced categorization rules.
- Full transfer matching automation.
- Multi-currency investment/crypto modeling beyond storing currency on candidate transactions.
- Importing non-PDF sources.
- Shipping/distribution packaging.

## Further Notes

The PDF extraction spike already established `pdf_oxide` viability and produced deterministic Nequi/Rappi parser diagnostics. This PRD is about productizing that path without broadening scope. The architectural guardrail is still: aggressive extraction is acceptable only because results remain reviewable candidates with provenance.
