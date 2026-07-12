# 0033 — Extend export, backup, and integrity for investments

Labels: `ready-for-agent`

## Parent

- Research: `docs/research/investment-tracking-model.md`
- Operational baseline: `docs/issues/0021-add-export-backup-and-import-safety.md`

## What to build

Extend Tracky's operational safety surface so investment data is included in backup, export, restore verification, and integrity checks. A user must not gain investment tracking at the cost of having positions or provenance omitted from their portable data.

## Acceptance criteria

- [x] Backup and restore verification preserve investment contributions, instruments, allocations, lifecycle events, positions, snapshots, reconciliations, adjustments, and provenance.
- [x] Structured export includes investment data and stable links back to canonical cash transactions without silently flattening quantities or currencies.
- [x] Default export preserves the same canonical-versus-pending boundary used for other Tracky data, with review/audit information available only through an explicit option.
- [x] Integrity checks detect broken contribution allocations, impossible position balances, duplicate lifecycle links, missing snapshot sources, and orphaned provenance.
- [x] Integrity and export commands remain safe for local-first use and avoid unrelated writes to user configuration.
- [x] Focused tests cover round-trip backup, export shape, restored investment reports, and representative integrity failures.

## Blocked by

- `0021-add-export-backup-and-import-safety.md`
- `0032-add-consolidated-monthly-investment-reports.md`
