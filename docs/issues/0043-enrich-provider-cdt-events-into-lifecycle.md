# 0043 — Enrich provider CDT events into canonical lifecycle operations

Labels: `ready-for-agent`

## Parent

- CDT lifecycle: `docs/issues/0028-track-complete-cdt-lifecycle.md`
- Provider-document reconciliation: `docs/issues/0031-import-and-reconcile-investment-provider-documents.md`

## Problem

Nu statements provide dated CDT opening and return evidence, but do not prove every contractual
term required by the canonical CDT lifecycle. The events remain reviewable indefinitely because
Tracky has no guided path to combine imported evidence with explicit reviewer-supplied terms.

## What to build

Add a review-first enrichment flow that previews which canonical CDT constitution, renewal, or
redemption operation an imported provider event can support, collects only the missing required
terms from the reviewer, and atomically links the accepted lifecycle operation back to the event
and its provenance.

## Acceptance criteria

- [x] A pending CDT provider event exposes the evidence-backed fields and the exact additional
  terms required for each compatible lifecycle action.
- [x] Dry-run validates the resulting lifecycle operation without mutating provider review state or
  canonical investments.
- [x] Explicit apply creates or links the canonical operation and accepts the provider event in one
  transaction with complete provenance.
- [x] Tracky never infers maturity, rate, payment mode, contract identifier, deductions, or
  principal components absent from evidence or reviewer input.
- [x] Ambiguous openings/returns, incompatible allocations/positions, reused events, and incomplete
  terms remain pending without partial writes.
- [x] Reports, export, backup, and integrity distinguish imported evidence from reviewer-supplied
  contractual enrichment.

## Completion evidence

- `investment-documents cdt-actions` previews evidence-backed fields and typed compatible actions;
  `enrich-cdt --dry-run|--apply` accepts a tagged explicit request through
  `tracky.cdt-provider-enrichment.v1`.
- `src/cdt_provider_enrichment.rs` reuses canonical CDT validators. Apply writes the operation,
  event decision, `cdt_provider_enrichments` audit split, and canonical provenance link in one
  transaction, or validates and links an exact existing operation; dry-run executes against an
  in-memory SQLite backup.
- `tests/cdt_provider_enrichment_cli.rs` uses synthetic redacted evidence for read-only preview and
  dry-run, explicit apply, ambiguous return actions, incomplete input, and pending-state
  preservation. Existing CDT, report, export, backup, integrity, and migration suites cover the
  reused lifecycle and operational surfaces. Investment reports expose the imported-evidence and
  reviewer-enrichment split for canonical CDT operations.

## Blocked by

- `0028-track-complete-cdt-lifecycle.md`
- `0031-import-and-reconcile-investment-provider-documents.md`
