# 0044 — Add date-scoped review planning

Labels: `ready-for-agent`

## Parent

- Safe batch actions: `docs/issues/0019-add-review-ergonomics-and-safe-batch-actions.md`
- Monthly finance reports: `docs/issues/0018-add-monthly-finance-reports.md`

## Problem

A statement labelled for one month can include posted transactions from the previous month. An
operator processing April–June currently has to construct the review set manually and can
accidentally accept March activity merely because it appeared in an April statement.

## What to build

Add date-scoped review planning and batch execution based on candidate `posted_date`. Preserve and
audit every imported candidate, including those outside the selected window; the range controls
review selection, never source ingestion or evidence retention.

## Acceptance criteria

- [ ] Candidate listing, action planning, and typed batch dry-run accept inclusive posted-date
  boundaries with stable validation errors.
- [ ] The plan reports selected and excluded candidate counts grouped by status and month.
- [ ] Applying a date-scoped plan can mutate only the explicit candidate IDs returned and approved
  by its dry-run.
- [ ] Out-of-range candidates, provider events, source documents, batches, and provenance remain
  unchanged and inspectable.
- [ ] Statement filename/month never overrides canonical `posted_date` selection.
- [ ] Public tests cover cross-month statements, inclusive boundaries, stale plans, and atomic
  application.

## Blocked by

- `0040-add-typed-atomic-batch-review-actions.md`
