# 0002 — Extract PDF inspection core from the spike

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`

## What to build

Move the proven `pdf_oxide` extraction and deterministic Nequi/Rappi parser behavior out of the spike binary into product-facing modules that can power a real CLI command. Preserve the review-first diagnostic behavior, redaction, parser status, page-level extraction errors, and structured row evidence.

The spike binary may remain as a wrapper or development tool, but product code should not require copying logic from it.

## Acceptance criteria

- [x] Product modules expose a read-only PDF inspection function that returns the contract-ready document/parser/candidate data.
- [x] Nequi and Rappi deterministic parser behavior from the spike is preserved.
- [x] Page-level `pdf_oxide` text/line extraction errors are surfaced as file-level extractor/parser errors, not silently skipped.
- [x] Rappi amount selection continues to handle multiple money cells by using visual order and non-zero preference.
- [x] Redaction still covers emails, cardholder/header names, long numbers, counterparties, addresses, card suffixes, and amounts in samples/evidence intended for agent-visible JSON.
- [x] The spike binary still compiles or is intentionally replaced by an equivalent product command.
- [x] Focused tests cover parser behavior using redacted fixtures rather than sensitive PDFs.

## Blocked by

- `0001-define-review-first-import-contract.md`

## Reconciliation evidence

Product core and focused redacted-fixture parser tests: `src/pdf.rs`, `tests/pdf_inspect_cli.rs`; introduced by `dc6a077` and validated in the pre-TUI audit.
