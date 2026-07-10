# 0022 — Close the generic candidate accept review bypass

Labels: `ready-for-agent`

## Parent

- Pre-TUI real-asset audit on 2026-07-10.
- Review-first guardrails from issues 0012, 0013, and 0014.

## What to build

Prevent the legacy `candidates accept` command from promoting typed finance candidates without the metadata and validation required by the specialized review commands.

The audit reproduced two unsafe successes: a Nequi inflow became canonical without an income source/kind, and a Rappi `card_payment` became canonical without an owned-account transfer pair. Both records had a null `transaction_kind`. Keep stable JSON compatibility, but either remove the generic success path for current parser semantics or make it refuse these candidates and direct the caller to `accept-income`, `accept-expense`, or `accept-transfer-pair`.

## Acceptance criteria

- [ ] Generic `candidates accept` cannot promote a `bank_movement` inflow without explicit income source and kind metadata.
- [ ] Generic `candidates accept` cannot promote `card_payment` or transfer-like candidates without an explicit validated transfer pair.
- [ ] Existing expense/category guardrails remain enforced.
- [ ] Refused generic accepts return stable JSON errors naming the required specialized command and leave candidates/canonical storage unchanged.
- [ ] Reports cannot receive a new unclassified canonical income, expense, or card-payment record through this bypass.
- [ ] README and agent workflow examples no longer recommend generic accept where a typed review action is required.
- [ ] CLI integration tests cover the inflow and card-payment regressions plus storage non-mutation.

## Blocked by

- `0012-review-own-account-transfers.md`
- `0013-accept-income-with-source.md`
- `0014-accept-expenses-with-categories.md`
