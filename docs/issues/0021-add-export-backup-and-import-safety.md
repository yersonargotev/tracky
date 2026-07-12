# 0021 — Add export, backup, and import safety commands

Labels: `ready-for-agent`

## Parent

- ADR: `docs/adr/0002-sqlite-canonical-store.md`

## What to build

Add operational commands that make the local-first SQLite database safe to use as the user's real finance tracker. The user should be able to back up the database, export canonical data for analysis, and verify database integrity before/after imports.

## Acceptance criteria

- [x] A user can create a timestamped SQLite backup from the CLI.
- [x] A user can run an integrity/check command that reports migration version, table counts, and obvious broken links.
- [x] A user can export canonical transactions, transaction lines, accounts, categories, income sources, transfers, and provenance links to CSV or JSON.
- [x] Export excludes pending/rejected candidates by default but can include review/audit data with an explicit flag.
- [x] Commands avoid writing to real home/config paths unless explicitly requested.
- [x] Tests cover backup creation, export shape, and integrity failure reporting.

## Blocked by

- `0018-add-monthly-finance-reports.md`
