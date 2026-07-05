# 0018 — Add monthly finance reports

Labels: `ready-for-agent`

## Parent

- ADR: `docs/adr/0002-sqlite-canonical-store.md`
- Product goal: Tracky should answer income, expense, transfer, and category questions from the canonical ledger.

## What to build

Add CLI/JSON reports over canonical transactions. The first report should summarize a month or date range with income totals, expense totals, net cash flow, category totals, income-source totals, and excluded transfer totals.

Reports must ignore pending/rejected candidates and must not count own-account transfers as expenses or income.

## Acceptance criteria

- [ ] A user can request a date-range report as stable JSON.
- [ ] The report includes total income, total expenses, net, category totals, income-source totals, and transfer totals.
- [ ] Pending and rejected candidates do not affect reports.
- [ ] Own-account transfers/card payments are excluded from income and expense totals but visible as transfers.
- [ ] Split transaction lines contribute to category totals correctly.
- [ ] Tests cover accepted income, categorized expenses, split expenses, transfers, and rejected candidates.

## Blocked by

- `0017-add-transaction-list-and-edit-review-cli.md`
