# 0011 — Add an owned account registry

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`

## What to build

Add a minimal way for Tracky to know which accounts belong to the user. This enables later transfer matching, card-payment review, and reports that do not confuse movements between owned accounts with real income or expenses.

The first vertical slice should be CLI/JSON-first: create/list owned accounts in SQLite and connect imported account hints to registered accounts conservatively. It should not require a TUI.

## Acceptance criteria

- [ ] A user can create or register an owned account with institution, account label, account type, currency, and optional masked identifier.
- [ ] A user can list registered accounts as stable JSON.
- [ ] Imported candidate account hints can be resolved to a registered account when institution, label/type, and currency are unambiguous.
- [ ] Ambiguous or unresolved account hints remain explicit instead of guessing.
- [ ] Tests prove Nequi wallet and RappiCard can be registered as separate owned accounts.
- [ ] Tests prove unresolved account hints do not block PDF import.

## Blocked by

- `0010-normalize-rappi-card-statement-semantics.md`
