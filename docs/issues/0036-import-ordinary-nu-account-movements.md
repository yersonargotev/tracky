# 0036 — Import ordinary Nu account movements

Labels: `ready-for-agent`

## Parent

- Review-first PDF import: `docs/issues/0005-add-import-pdf-persistence.md`
- Nu investment-document adapter: `docs/issues/0035-parse-layout-fragmented-nu-investment-rows.md`

## Problem

The Nu Cuenta PDF already used for CDT openings, CDT returns, and transfers to Plenti also contains
ordinary account movements. Tracky currently ignores those rows: the generic financial PDF parser
supports Nequi and Rappi, while the Nu investment-document adapter deliberately recognizes only
`Abriste un CDT`, `Recibiste dinero de un CDT`, and `Enviaste a Plenti`.

Importing the same PDF independently through the generic and investment-document paths is not a
safe workaround. Both paths share source-document identity and exact-source deduplication, so the
document must be inspected and persisted once while producing the appropriate review-first records
for both ordinary and investment-related rows.

## What to build

Extend Nu Cuenta ingestion so one content-detected inspection/import of the statement emits
ordinary account movements as candidate transactions and keeps the existing CDT/Plenti rows as
typed provider events. Derive the supported ordinary-row vocabulary, signs, and layout shapes only
from representative user-authorized Nu statements and privacy-safe fixtures.

The import must preserve a single source document and atomic review-first boundary. It must not
promote ordinary movements directly to canonical income or expenses, classify transfers as
consumption, or duplicate CDT returns as ordinary income. Existing provider-event reconciliation
and exact-source rejection remain authoritative.

## Acceptance criteria

- [x] Content detection recognizes the representative Nu Cuenta format without relying on its
  filename, account identifier, or a Nu-specific CLI hint.
- [x] One inspect response exposes both ordinary candidate transactions and the existing supported
  Nu provider events from the same source without requiring two imports of the PDF.
- [x] One import persists a single source document and one atomic review-first batch containing all
  supported outputs; extraction, validation, or persistence failure leaves no partial candidates,
  provider events, provenance, or canonical transactions.
- [x] Ordinary debit and credit rows preserve exact effective date, signed COP minor units,
  normalized description, page/row provenance, redacted evidence, and deterministic fingerprints.
- [x] Ordinary movements remain pending review with evidence-grounded direction/semantic hints;
  they become canonical income or expenses only through the existing typed review actions with the
  required income source or expense category.
- [x] Own-account transfers and card payments remain transfer-like candidates rather than income or
  expense, and ambiguous counterparties remain unresolved for review.
- [x] `Abriste un CDT`, `Recibiste dinero de un CDT`, and `Enviaste a Plenti` continue to produce
  only their existing provider events; they are not duplicated as ordinary income, expense, or
  generic transfer candidates.
- [x] Linear and layout extraction of the same row deduplicate deterministically while preferring
  coordinate-bearing page/row provenance.
- [x] Unsupported, incomplete, balance-only, summary, fee, tax, or ambiguous Nu rows fail safely or
  remain absent/pending; the parser does not invent categories, income sources, counterparties,
  instruments, CDT terms, or canonical transactions.
- [x] Exact document reimport is rejected before writes, and normalized movement fingerprints
  continue to surface cross-document possible duplicates without silently dropping reviewable rows.
- [x] Existing March–June Nu CDT/Plenti event counts and values remain unchanged, and existing
  Nequi, Rappi, Wenia, and Plenti imports remain compatible.
- [x] Privacy-safe unit and public CLI/JSON tests cover representative Nu ordinary debit, credit,
  transfer-like, fragmented-layout, mixed ordinary/investment, atomic failure, and exact-reimport
  cases without committing real PDFs, credentials, account identifiers, or personal data.

## Blocked by

- `0035-parse-layout-fragmented-nu-investment-rows.md`

## Source-artifact constraints

- Use the same authorized Nu Cuenta March–June PDFs already validated for issue 0035 only during
  local development with isolated HOME, XDG, and SQLite paths.
- Before implementation, record a privacy-safe inventory of the ordinary row shapes actually
  present. Do not claim support for labels or semantics not demonstrated by those artifacts.
- Real statements, passwords, names, account identifiers, raw extracted text, probes, and generated
  databases must remain uncommitted.


## Reconciliation evidence

- `src/investment_document_parsers.rs` reuses the issue-0035 logical-row/layout seam to emit
  deterministic Nu ordinary candidates while excluding the three provider-event forms.
- `src/investment_documents.rs` exposes the compatible `tracky.investment-documents.v2` mixed
  response and persists one source, one batch, candidates, provider events, fingerprints, and both
  provenance kinds in one SQLite transaction.
- `tests/investment_provider_documents_cli.rs` covers mixed inspect/import, exact reimport rejection,
  layout preference, transfer-like card payment semantics, secret safety, and rollback after an
  injected provider-event persistence failure.
- Authorized March–June validation was deterministic and read-only for inspect. Privacy-safe
  ordinary counts were 1/2/3/3 (inflow/outflow/card-payment: 1/0/0, 1/1/0, 0/3/1, 3/0/0); provider
  events remained 6/9/4/3. Each isolated import produced one source and one batch, complete
  candidate/event provenance, zero canonical transactions, no CDT/Plenti ordinary duplicates, and
  rejected exact reimport before writes.
- Verified with `cargo fmt --all -- --check`, `cargo test --locked --all-targets`, `cargo clippy
  --locked --all-targets --all-features -- -D warnings`, `cargo build --locked --release`, and
  `git diff --check`.
