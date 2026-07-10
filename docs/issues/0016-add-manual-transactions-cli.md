# 0016 — Add manual transactions CLI

Labels: `ready-for-agent`

## Parent

- Product goal: Tracky should be usable even when a transaction is missing from a supported PDF.

## What to build

Add CLI/JSON commands to create manual canonical transactions for expenses, income, and transfers. Manual entries should use the same accounts, categories, income sources, transfer semantics, and transaction lines as reviewed imports, but provenance should clearly indicate that the source was manual user entry.

## Acceptance criteria

- [x] A user can add a manual expense with account, date, amount, currency, description, and category.
- [x] A user can add a manual income with account, date, amount, currency, description, and income source/kind.
- [x] A user can add a manual transfer between owned accounts without counting it as income or expense.
- [x] Manual transactions receive audit/provenance metadata distinct from PDF provenance.
- [x] Manual entries appear in the same transaction listing/reporting queries as accepted imported transactions.
- [x] Tests cover manual expense, manual income, manual transfer, and invalid/unbalanced input.

## Blocked by

- `0012-review-own-account-transfers.md`
- `0013-accept-income-with-source.md`
- `0014-accept-expenses-with-categories.md`
