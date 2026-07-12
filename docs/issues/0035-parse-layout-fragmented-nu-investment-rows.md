# 0035 — Parse layout-fragmented Nu investment rows

Labels: `ready-for-agent`

## Parent

- Provider-document baseline: `docs/issues/0031-import-and-reconcile-investment-provider-documents.md`

## Problem

`tracky investment-documents inspect` safely recognizes but rejects representative April and
May 2026 Nu Cuenta statements with `partially_recognized_document`, even though they contain the
already-supported `Abriste un CDT` and `Recibiste dinero de un CDT` movement shapes.

The Nu parser currently searches only the joined `pdf_oxide::extract_text` lines. In the affected
documents, extraction separates the amount from the description in April and separates additional
date/description cells in May, so the complete-row regular expression finds no events. The existing
coordinate-bearing `extract_text_lines` output can reconstruct the affected visual rows without
inventing document semantics: release validation found nine complete supported rows in April and
four in May.

## What to build

Make the Nu investment-document adapter parse supported movement rows from the existing logical
layout-row representation when linear extracted text fragments a visual row. Preserve content-based
provider detection, exact money/date handling, redacted evidence, deterministic fingerprints, and
the review-first boundary. Do not broaden the supported Nu vocabulary or infer contractual CDT terms.

## Acceptance criteria

- [x] Layout-fragmented Nu rows for `Abriste un CDT`, `Recibiste dinero de un CDT`, and
  `Enviaste a Plenti` are reconstructed from page and bounding-box evidence before parsing.
- [x] Privacy-safe fixtures derived from both affected structures cover an amount split from its
  date/description and a date, description, and amount split across layout cells.
- [x] The affected April and May statement structures produce the supported events with exact date,
  signed COP minor units, event type, redacted evidence, and stable fingerprints.
- [x] Existing March and June Nu structures keep their current event counts and values without
  duplicate events when both linear and layout extraction contain the same movement.
- [x] Incomplete, ambiguous, or unsupported Nu rows still fail safely or remain absent; the parser
  does not invent instruments, quantities, rates, maturity dates, or CDT contract identifiers.
- [x] `investment-documents inspect` remains deterministic and read-only, while import remains
  atomic and exact-source reimports remain rejected.
- [x] Focused unit and public CLI/JSON regression tests cover the fragmented layouts without
  committing real PDFs, account identifiers, customer data, or credentials.

## Blocked by

- `0031-import-and-reconcile-investment-provider-documents.md`

## Reproduction evidence (2026-07-12)

- Release CLI reproduction against authorized local assets is deterministic: both affected files
  return exit code 2, zero events, and `partially_recognized_document`; import performs no writes.
- A redacted `pdf_oxide::extract_text` probe shows supported phrases and the document year are
  present, but the parser's complete-row regex has zero matches because cells are fragmented.
- Applying the module's existing page/y-coordinate logical-row grouping in a read-only release
  validation yields nine complete supported rows for April and four for May. This corrects the
  earlier probe's 8/3 undercount; every additional row uses supported vocabulary and has a distinct
  date, exact amount, and fingerprint.
- March and June remain successful controls, proving that credentials, provider detection, year
  extraction, and the supported movement vocabulary are not the failing boundaries.

## Implementation evidence (2026-07-12)

- Privacy-safe unit fixtures reproduce both observed fragmentation shapes and assert exact dates,
  signed COP minor units, event types, redacted evidence, page provenance, stable output, and
  linear/layout deduplication.
- The encrypted public CLI fixture emits the amount before the date/description in linear PDF
  content while retaining the same visual Y row; inspect/import succeeds through layout recovery,
  runtime secrets stay absent, and exact-source reimport returns `duplicate_source_document`.
- Isolated release validation is byte-deterministic across repeated inspect calls. March preserves
  6 events, April recovers 9, May recovers 4, and June preserves 3. Each first import writes exactly
  one source, one batch, and the expected pending events with zero canonical transactions; every
  exact reimport is rejected.
- `cargo fmt --all -- --check`, `cargo test --locked --all-targets`,
  `cargo clippy --locked --all-targets --all-features -- -D warnings`,
  `cargo build --locked --release`, and `git diff --check` pass.
