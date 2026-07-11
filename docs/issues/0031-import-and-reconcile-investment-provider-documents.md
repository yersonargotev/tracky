# 0031 — Import and reconcile investment provider documents

Labels: `ready-for-agent`

## Parent

- Research: `docs/research/investment-tracking-model.md`

## What to build

Extend the review-first import workflow to investment-provider documents so Tracky can reduce manual entry for trii, Wenia, and CDT records. Use a common investment-document boundary with provider-specific adapters rather than embedding provider assumptions in the canonical investment model.

Implementation must be driven by representative, user-authorized source artifacts or privacy-safe fixtures derived from them. A bank description alone is not sufficient evidence of a specific security, digital asset, quantity, rate, or position.

## Acceptance criteria

- [x] Investment documents are inspected and imported into reviewable provider events or snapshots before they can affect canonical positions.
- [ ] Provider-specific adapters cover the representative trii movement/portfolio, Wenia movement, and CDT certificate/statement formats selected during implementation.
- [ ] Each supported adapter has privacy-safe generated or redacted fixtures grounded in a representative artifact; no parser is claimed complete from filename or marketing-page assumptions alone.
- [ ] Imported deposits, purchases, sales, dividends, fees, conversions, CDT terms, and snapshots map to the shared investment vocabulary without losing provider evidence.
- [ ] Reconciliation links provider movements to bank-side contributions or withdrawals when evidence matches and surfaces unmatched or ambiguous rows for review.
- [x] Exact document reimports and repeated provider movements cannot duplicate canonical operations or positions.
- [x] Unsupported or ambiguous documents fail safely or remain pending without guessing instruments, quantities, or currencies.
- [ ] Focused tests cover each supported provider adapter, duplicate handling, cross-source reconciliation, provenance, atomic review, and safe unsupported-document behavior.

## Blocked by

- `0028-track-complete-cdt-lifecycle.md`
- `0029-track-complete-brokerage-investment-lifecycle.md`
- `0030-reconcile-investment-positions-and-dated-valuations.md`

## Implementation evidence (partial, 2026-07-11)

- Authorized runtime artifacts inspected locally: NU Cuenta March-June 2026, Wenia June
  portfolio summary, and Plenti April-June transactional statement. They remain outside the
  commit; synthetic redacted rows derived from their structure are used in tests.
- `src/investment_documents.rs` and
  `docs/contracts/investment-provider-documents-json.md` define the common content-detected
  boundary, pending events, exact values, provenance, review, and durable deduplication.
- Trii is explicitly unsupported because it provides no downloadable statement. Its adapter
  criterion remains unchecked.
- Wenia evidence supports a portfolio snapshot, not movements. Plenti and NU show aggregate
  CDT balance/open/return wording but no contract identifier, maturity, or agreed rate; no CDT
  terms are invented.
- Remaining before completion: typed canonical acceptance for supported events/snapshots,
  compatibility-checked automatic reconciliation candidates (including ambiguity), fuller
  provenance inspection, and focused public CLI tests for every supported adapter and atomic
  failure path.
