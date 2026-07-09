# 0014 — Accept expense candidates with categories

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`
- Domain glossary: category analysis should use transaction lines/splits over canonical transactions.

## What to build

Let the user accept a reviewed purchase candidate as a categorized expense. The first version should support a single category line whose amount equals the canonical transaction amount. It should work for card purchases after RappiCard semantics are safe.

This is the minimum path from imported PDF candidate to categorized real expense.

## Acceptance criteria

- [x] A user can create/list categories as stable JSON.
- [x] A user can accept a purchase candidate with exactly one expense category.
- [x] Acceptance creates a canonical transaction plus one transaction line tied to the selected category.
- [x] The transaction line amount reconciles exactly with the canonical transaction amount.
- [x] Rejected, transfer-like, or already accepted candidates cannot be accepted as categorized expenses.
- [x] Tests cover a RappiCard purchase candidate, category persistence, and provenance preservation.

## Blocked by

- `0010-normalize-rappi-card-statement-semantics.md`
