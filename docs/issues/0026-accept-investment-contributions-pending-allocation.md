# 0026 — Accept investment contributions pending allocation

Labels: `ready-for-agent`

## Parent

- Research: `docs/research/investment-tracking-model.md`

## What to build

Add the first complete investment-review path: a user can confirm that a candidate or manual outflow is capital destined for investment even when the exact instrument or acquired quantity is not known yet.

The confirmed principal must remain traceable, appear as an investment contribution, and stay out of consumption expenses and income. Unknown allocation is an explicit review state, not a reason to guess an instrument or degrade the movement to an expense.

## Acceptance criteria

- [ ] A typed CLI/JSON action can confirm a reviewable outflow as an investment contribution without using the generic candidate-accept path.
- [ ] A manual equivalent can record an investment contribution when no imported candidate exists.
- [ ] A contribution can remain pending allocation while retaining source account, date, amount, currency, description, and provenance.
- [ ] Confirmed investment principal is excluded from expense and income totals and is exposed separately in date-range reporting.
- [ ] Existing expense, income, and own-account-transfer behavior and JSON contracts remain compatible.
- [ ] Invalid or incompatible actions fail atomically without changing candidate or canonical state.
- [ ] Focused CLI/JSON and storage tests cover candidate confirmation, manual entry, pending allocation, reporting, provenance, and non-mutation failures.

## Blocked by

- None — can start immediately.
