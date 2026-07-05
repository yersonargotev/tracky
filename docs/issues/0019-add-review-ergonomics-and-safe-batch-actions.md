# 0019 — Add review ergonomics and safe batch actions

Labels: `ready-for-agent`

## Parent

- Follow-up from real asset review on 2026-07-05.

## What to build

Make candidate review practical for hundreds of rows without bypassing review-first safety. Add batch summaries, duplicate comparison output, and safe suggested actions that agents can inspect before applying.

This should not auto-accept or auto-reject without an explicit apply command and clear candidate ids.

## Acceptance criteria

- [ ] A user can get a batch summary grouped by status, duplicate status, institution, account, direction/semantic hint, and largest amounts.
- [ ] A user can compare a possible duplicate against its matched candidates/canonical transactions as stable JSON.
- [ ] A user can generate suggested review actions for obvious duplicates and likely transfers without applying them automatically.
- [ ] Applying any batch action requires explicit candidate ids or a saved suggestion id.
- [ ] Batch actions preserve the same validation rules as single-candidate review commands.
- [ ] Tests cover duplicate comparison, suggestion generation, dry-run output, and explicit apply.

## Blocked by

- `0012-review-own-account-transfers.md`
- `0014-accept-expenses-with-categories.md`
