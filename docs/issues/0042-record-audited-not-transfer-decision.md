# 0042 — Record an audited not-transfer decision

Labels: `ready-for-agent`

## Parent

- Own-account transfer review: `docs/issues/0012-review-own-account-transfers.md`
- Typed review guardrails: `docs/issues/0022-close-generic-candidate-accept-review-bypass.md`

## Problem

Some candidates can be structurally similar to an own-account transfer without a valid matching
counter-leg. There is no explicit way for a human reviewer to record that the candidate is not a
transfer and then proceed through normal typed income or expense review.

## What to build

Allow a reviewer to dismiss the transfer hypothesis for a specific pending candidate with an
audited reason. The decision must not accept the financial transaction by itself; it only enables
the existing typed action whose other validations still pass.

## Acceptance criteria

- [x] A pending candidate blocked only by transfer suspicion can receive an explicit `not_transfer`
  decision with reviewer-supplied reason.
- [x] The decision preserves the original structured suspicion evidence and is exported and
  integrity-checked.
- [x] Income, expense, or investment acceptance still requires its existing source, category, sign,
  semantic, account, and duplicate validations.
- [x] A valid suggested/accepted transfer pair cannot be bypassed silently, and reviewed candidates
  cannot receive contradictory decisions.
- [x] Candidate action explanations show the effect of the decision before and after it is applied.
- [x] Public tests cover the override, invalid bypasses, audit persistence, and atomic failure.

## Blocked by

- `0037-align-transfer-suspicion-and-resolution.md`
- `0041-explain-candidate-review-actions.md`

## Reconciliation evidence

- `candidates decide-not-transfer <candidate-id> --reason <text> --db <path> --json` records one
  atomic `not_transfer` decision without promoting the candidate or creating a canonical
  transaction. Empty reasons, candidates without an admissible transfer hypothesis, repeated
  decisions, and already reviewed candidates are refused with stable JSON errors.
- `candidate_transfer_decisions` preserves the reviewer reason and the original structured pair
  evidence. Review-audit export includes the rows, and integrity checks validate their references,
  reason/evidence invariants, and absence of contradictory accepted transfer pairs.
- Typed income, expense, and investment commands retain their existing validation paths; the
  decision only removes the transfer-pair block. `candidates explain-actions` shows the explicit
  decision before application and the resulting typed/transfer availability afterward.
- `tests/candidate_review_cli.rs` covers the privacy-safe override, direct-pair contradiction,
  reviewed/no-suspicion/empty-reason bypasses, export/integrity persistence, and injected SQLite
  rollback through the public CLI/JSON seam.
