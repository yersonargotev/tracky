# 0015 — Support split transaction lines

Labels: `ready-for-agent`

## Parent

- Domain glossary: a split is a transaction represented by multiple categorizable lines.

## What to build

Extend categorized review so a single canonical transaction can be split across multiple categories. This covers purchases where one payment includes food, delivery fee, tip, household items, or other mixed purposes.

The slice should be verifiable through CLI/JSON and SQLite tests; no TUI is required.

## Acceptance criteria

- [ ] A user can create or update transaction lines so multiple lines belong to one canonical transaction.
- [ ] The sum of split lines must equal the canonical transaction amount in minor units.
- [ ] Tracky rejects splits with missing categories, wrong currency, or unbalanced totals.
- [ ] Existing single-line categorized expenses continue to work.
- [ ] JSON output exposes split lines in a stable shape.
- [ ] Tests cover balanced split, unbalanced rejection, and audit/provenance retention.

## Blocked by

- `0014-accept-expenses-with-categories.md`
