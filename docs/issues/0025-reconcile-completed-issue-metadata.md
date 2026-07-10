# 0025 — Reconcile completed issue metadata

Labels: `ready-for-agent`

## Parent

- Pre-TUI tracker audit on 2026-07-10.

## What to build

Make the local Markdown tracker accurately distinguish implemented slices from remaining work before agents start the TUI.

Issues 0001–0007 and 0011 still have every acceptance criterion unchecked even though their commands, storage, contracts, and tests are present and passed the pre-TUI audit. Other completed issues use checked criteria, while all files retain the workflow label `ready-for-agent`. This makes dependency and next-slice discovery unreliable.

Verify historical completion from code, tests, docs, and commits, then update the tracker metadata without changing product behavior.

## Acceptance criteria

- [ ] Acceptance criteria in issues 0001–0019 accurately reflect verified implementation state.
- [ ] Any genuinely incomplete criterion remains unchecked and receives a focused follow-up issue instead of being marked complete optimistically.
- [ ] The local tracker documents how completed issues are represented when status labels have no `done` value.
- [ ] `docs/issues/README.md` clearly separates completed slices from the remaining queue or otherwise exposes current state unambiguously.
- [ ] No product source, contract behavior, or sensitive asset data changes as part of this documentation-only cleanup.
