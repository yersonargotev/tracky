# 0039 — Assign an owned account to an imported candidate

Labels: `ready-for-agent`

## Parent

- Owned-account registry: `docs/issues/0011-add-owned-account-registry.md`
- Candidate review: `docs/issues/0007-add-candidate-review-cli.md`

## Problem

When conservative import leaves a candidate without an account, Tracky has no supported command
to assign the reviewed owned account later. The operator must recreate the database and reimport
the source document before typed review and transfer matching can use the account.

## What to build

Add an explicit review action that assigns or corrects the owned account of an unreviewed imported
candidate while retaining its original source hints, provenance, fingerprint, and an auditable
record of the reviewer decision.

## Acceptance criteria

- [x] A pending imported candidate can be assigned to an existing compatible owned account through
  stable CLI/JSON.
- [x] The action validates currency and ownership and refuses missing or incompatible accounts
  without partial mutation.
- [x] Original import hints and provenance remain inspectable beside the reviewed assignment.
- [x] Already accepted/rejected candidates cannot be silently reassigned.
- [x] Transfer discovery and typed review immediately use the reviewed account assignment.
- [x] Export, backup, and integrity preserve and validate the assignment audit trail.

## Blocked by

- `0038-make-owned-account-resolution-tolerant-and-explainable.md`

## Reconciliation evidence

- `tracky candidates assign-account CANDIDATE_ID --account-id ID --json` updates only an
  unreviewed candidate after owned-account and currency validation, preserving import hints,
  provenance, and fingerprint while exposing the reviewed resolution through candidate JSON.
- `candidate_account_assignment_events` records append-only, revisioned assignment decisions;
  candidate inspection exposes the active assignment and its previous account.
- Transfer discovery and typed income review consume the updated candidate account immediately.
  Review-audit export includes every assignment revision, SQLite backup preserves the table, and
  integrity checks validate references, currency/ownership, revision continuity, and the active
  candidate head.
- Synthetic public coverage lives in `tests/candidate_review_cli.rs` and
  `tests/operations_cli.rs`.
