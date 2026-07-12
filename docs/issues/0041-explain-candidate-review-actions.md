# 0041 — Explain candidate review actions

Labels: `ready-for-agent`

## Parent

- Candidate review: `docs/issues/0007-add-candidate-review-cli.md`
- Review suggestions: `docs/issues/0019-add-review-ergonomics-and-safe-batch-actions.md`

## Problem

Operators can receive refusal codes such as possible own-account transfer, unresolved account, or
unmatched provider target without one read-only view explaining which review actions remain valid
and which evidence dimension blocks each alternative.

## What to build

Add a deterministic read-only explanation for a candidate that evaluates every supported review
action and reports whether it is available, blocked, or requires explicit data, using the same
validation rules that the mutating commands enforce.

## Acceptance criteria

- [x] One CLI/JSON response explains income, expense, investment, transfer, duplicate rejection,
  and ordinary rejection availability for a candidate.
- [x] Blocked actions identify stable reason codes and relevant non-sensitive dimensions rather
  than relying on prose-only errors.
- [x] Explanations are read-only, deterministic, redacted, and do not expose credentials or raw
  document evidence.
- [x] Explanation outcomes remain consistent with the corresponding mutating commands.
- [x] Tests cover accepted, ambiguous, unresolved-account, possible-duplicate, transfer-like, and
  no-valid-action states.

## Blocked by

- `0037-align-transfer-suspicion-and-resolution.md`
- `0038-make-owned-account-resolution-tolerant-and-explainable.md`

## Reconciliation evidence

- `candidates explain-actions <candidate-id> --db <path> --json` opens SQLite read-only and emits
  six actions in stable order under `tracky.candidate-action-explanation.v1`, with only typed
  statuses, stable reason codes, and non-sensitive candidate dimensions.
- `src/storage.rs` derives the explanation from the same candidate-state, typed-shape, transfer-pair,
  and rejection validators used by mutating review commands; it never returns descriptions,
  provenance, raw evidence, account identifiers, or credentials.
- `tests/candidate_review_cli.rs` covers deterministic/read-only output, accepted/no-action,
  ambiguous and unresolved accounts, possible duplicates, transfer-like candidates, and parity
  with the existing typed mutation refusal cases using synthetic fixtures only.
