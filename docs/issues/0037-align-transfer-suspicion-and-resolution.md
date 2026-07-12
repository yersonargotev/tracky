# 0037 — Align transfer suspicion and resolution

Labels: `ready-for-agent`

## Parent

- Own-account transfer review: `docs/issues/0012-review-own-account-transfers.md`
- Cross-batch suggestions: `docs/issues/0023-suggest-cross-batch-transfer-actions.md`

## Problem

Real April–June review produced a Nequi outflow and Nu inflow that cannot receive any supported
decision. Typed income and expense review refuse them as a possible own-account transfer, while
pair discovery omits them and direct pair acceptance reports that they do not match. The safety
predicate can therefore strand candidates without a valid next action.

## What to build

Make transfer suspicion, suggestion, and acceptance use one coherent evidence model. Whenever a
candidate is blocked from individual typed review solely because it may be an own-account
transfer, Tracky must either expose an admissible pair or return a stable explanation of the exact
unresolved condition without falsely claiming that a valid pair exists.

## Acceptance criteria

- [ ] One shared, direction-aware rule set governs transfer suspicion, pair suggestions, and pair
  acceptance across bank movements and card payments.
- [ ] A candidate is never blocked from every supported review action solely because the transfer
  predicates disagree.
- [ ] The confirmed privacy-safe Nequi/Nu stranded shape has a deterministic actionable outcome.
- [ ] Existing accepted Nequi-to-Rappi card-payment pairs remain compatible and non-transfer
  income/expenses do not become permissive.
- [ ] Public tests cover suggestion, direct acceptance, individual-review refusal, and the
  previously stranded boundary without real descriptions, amounts, or account identifiers.

## Blocked by

- None — can start immediately.
