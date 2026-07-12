# 0038 — Make owned-account resolution tolerant and explainable

Labels: `ready-for-agent`

## Parent

- Owned-account registry: `docs/issues/0011-add-owned-account-registry.md`

## Problem

Imported candidates resolve only when the registered account label or type exactly matches an
extractor-owned label such as `Nequi wallet`, `Rappi card`, or `Nu account`. Reasonable user labels
can leave every candidate unresolved even when institution and currency identify one unique owned
account, and the CLI does not explain the failed match.

## What to build

Resolve an imported account conservatively from stable institution, currency, optional masked
identifier, and label/type evidence without making internal parser labels part of the user-facing
contract. When zero or multiple accounts remain possible, preserve the unresolved state and expose
the dimensions that prevented a unique match.

## Acceptance criteria

- [ ] A unique compatible owned account can resolve despite a user-defined label that differs from
  the parser's display label.
- [ ] Masked-identifier mismatches and multiple compatible accounts remain unresolved rather than
  guessed.
- [ ] Import and candidate inspection expose stable machine-readable reasons for unresolved or
  ambiguous account resolution.
- [ ] Existing exact-label resolution remains compatible for Nequi, Rappi, Nu, Plenti, and Wenia.
- [ ] Public tests cover unique, ambiguous, masked-mismatch, and no-match outcomes without personal
  account data.

## Blocked by

- None — can start immediately.
