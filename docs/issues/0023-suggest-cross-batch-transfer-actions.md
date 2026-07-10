# 0023 — Suggest transfer actions across import batches

Labels: `ready-for-agent`

## Parent

- Pre-TUI real-asset audit on 2026-07-10.
- Follow-up to issue 0019 batch review ergonomics.

## What to build

Make `candidates suggest-actions --import-batch-id` surface valid owned-account transfer pairs when the two candidate sides came from different PDF import batches.

Real Nequi and Rappi statements are imported as separate batches. In the audit, global `candidates list-transfer-pairs` found two valid pending pairs, while `suggest-actions` returned zero suggestions for all six batches because it only paired candidates loaded from the selected batch.

Suggestions must remain deterministic and strictly read-only. Applying one must continue to require both explicit candidate ids through the existing atomic `apply-actions` path.

## Acceptance criteria

- [ ] A selected batch receives a transfer suggestion when either side belongs to that batch and the valid counterpart belongs to another batch.
- [ ] Cross-batch suggestions include both candidate ids, both import batch ids, and the existing structured transfer evidence.
- [ ] A pair is emitted at most once per `suggest-actions` response with deterministic ordering and suggestion id.
- [ ] Reviewed, unresolved, non-owned, currency-mismatched, amount-mismatched, and date-mismatched candidates remain excluded by the existing transfer rules.
- [ ] The command remains read-only and does not persist approvals or suggestions.
- [ ] `apply-actions` still requires explicit candidate ids and preserves atomic preflight/apply behavior.
- [ ] Synthetic integration tests use two separate import batches and cover the real cross-document shape without committing sensitive PDFs.

## Blocked by

- `0012-review-own-account-transfers.md`
- `0019-add-review-ergonomics-and-safe-batch-actions.md`
