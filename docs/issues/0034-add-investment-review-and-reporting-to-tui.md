# 0034 — Add investment review and reporting to the TUI

Labels: `ready-for-agent`

## Parent

- Research: `docs/research/investment-tracking-model.md`
- TUI baseline: `docs/issues/0020-add-tui-review-mvp.md`

## What to build

Expose investment review and reporting through the terminal UI without creating a second investment model. The TUI should orchestrate the same storage behavior and stable actions already exercised through CLI/JSON.

The user should be able to recognize pending investment intent, allocate contributions, inspect positions and reconciliation warnings, and review monthly investment totals alongside the existing finance workflow.

## Acceptance criteria

- [ ] The TUI identifies confirmed investment contributions and distinguishes pending allocation from ordinary pending review.
- [ ] Users can invoke the same supported contribution and allocation actions as the CLI without bypassing typed validation.
- [ ] Instrument and position views show quantity, cost, last valuation date, freshness, and reconciliation differences when available.
- [ ] Monthly investment summaries expose capital contributed, reinvested, withdrawn, income, costs, and pending allocation without double counting.
- [ ] TUI actions reuse canonical storage and review seams; no TUI-only investment state or reporting logic is introduced.
- [ ] Provenance and provider evidence are summarized safely without exposing credentials or requiring raw documents to remain present.
- [ ] Focused tests or documented terminal verification cover the main investment review, allocation, position, and reporting paths.

## Blocked by

- `0020-add-tui-review-mvp.md`
- `0032-add-consolidated-monthly-investment-reports.md`
