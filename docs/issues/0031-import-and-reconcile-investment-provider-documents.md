# 0031 — Import and reconcile investment provider documents

Labels: `ready-for-agent`

## Parent

- Research: `docs/research/investment-tracking-model.md`

## What to build

Extend the review-first import workflow to investment-provider documents so Tracky can reduce manual entry for Wenia and CDT records. Use a common investment-document boundary with provider-specific adapters rather than embedding provider assumptions in the canonical investment model. Trii is outside this scope because it does not provide an accessible statement from which to build and validate a responsible adapter.

Implementation must be driven by representative, user-authorized source artifacts or privacy-safe fixtures derived from them. A bank description alone is not sufficient evidence of a specific security, digital asset, quantity, rate, or position.

## Acceptance criteria

- [x] Investment documents are inspected and imported into reviewable provider events or snapshots before they can affect canonical positions.
- [x] Provider-specific adapters cover the representative Wenia and CDT certificate/statement formats selected during implementation; Trii remains explicitly unsupported until a representative statement becomes accessible.
- [x] Each supported adapter has privacy-safe generated or redacted fixtures grounded in a representative artifact; no parser is claimed complete from filename or marketing-page assumptions alone.
- [x] Imported deposits and snapshots map to the shared investment vocabulary without losing provider evidence; unsupported purchases, sales, dividends, fees, conversions, and incomplete CDT terms remain pending rather than being invented.
- [x] Reconciliation links provider movements to bank-side contributions or withdrawals when evidence matches and surfaces unmatched or ambiguous rows for review.
- [x] Exact document reimports and repeated provider movements cannot duplicate canonical operations or positions.
- [x] Unsupported or ambiguous documents fail safely or remain pending without guessing instruments, quantities, or currencies.
- [x] Focused tests cover each supported provider adapter, duplicate handling, cross-source reconciliation, provenance, atomic review, and safe unsupported-document behavior.

## Blocked by

- `0028-track-complete-cdt-lifecycle.md`
- `0029-track-complete-brokerage-investment-lifecycle.md`
- `0030-reconcile-investment-positions-and-dated-valuations.md`

## Implementation evidence (completed applicable scope, 2026-07-11)

- Authorized runtime artifacts inspected locally: NU Cuenta March-June 2026, Wenia June
  portfolio summary, and Plenti April-June transactional statement. They remain outside the
  commit; synthetic redacted rows derived from their structure are used in tests.
- `src/investment_documents.rs` and
  `docs/contracts/investment-provider-documents-json.md` define the common content-detected
  boundary, pending events, exact values, provenance, review, and durable deduplication.
- Trii is explicitly outside the supported adapter scope because it provides no accessible
  statement from which to derive and validate a responsible parser.
- Wenia evidence supports a portfolio snapshot, not movements. Plenti and NU show aggregate
  CDT balance/open/return wording but no contract identifier, maturity, or agreed rate; no CDT
  terms are invented.
- Provider events extend canonical provenance and expose their source document, batch,
  parser/version, page/row, redacted evidence, decision, selected link, and accepted snapshot.
  Existing provenance stores migrate compatibly.
- Complete Wenia positions have typed acceptance into the immutable dated snapshot, position,
  baseline, and reconciliation model from 0030. Incomplete positions remain pending.
- Read-only reconciliation candidates are direction- and semantics-aware and distinguish unique,
  ambiguous, unmatched, already-reconciled, and incompatible results. Explicit selection is
  atomic and durable uniqueness prevents either side from being consumed twice.
- Unsupported purchases, sales, dividends, fees, conversions, or CDT terms remain pending because
  no representative evidence supports promoting them into canonical operations.
- Typed movement actions require provider and counterpart accounts and compare direction, shared
  semantics, date, amount, currency, and durable external references. Inspection expands the
  accepted canonical target or immutable snapshot chain.
- Deterministically generated encrypted/redacted NU tests cover missing, wrong, and correct runtime
  credentials plus exact reimport without depending on real statements or secrets.
