# Operational safety CLI/JSON contract

Issue 0021 defines three top-level, JSON-only commands. They open the source database with
SQLite read-only flags and never apply migrations or write user home/config paths.

## Backup

`tracky backup --db PATH [--destination PATH] --json` emits
`tracky.backup.v1`. When omitted, the destination is adjacent to the source and named
`<source-stem>-YYYYMMDDTHHMMSS.mmmZ.sqlite3`.

Tracky uses SQLite's online backup API into an exclusive temporary file, runs
`PRAGMA integrity_check` on the result, and publishes it with a same-filesystem hard link.
Publishing therefore fails rather than replacing an existing destination. Any failure removes
the temporary file and never leaves a final path that looks successful. A successful file is a
normal SQLite database that can be opened directly; it is not an archive or custom format.

## Integrity

`tracky integrity --db PATH --json` emits `tracky.integrity.v1`. It reports the SQLite
`integrity_check` result, `PRAGMA user_version`, sorted canonical table counts, sorted findings,
and a global `ok`. Finding categories are `sqlite_corruption`, `schema_incompatibility`, and
`broken_reference`; command/open/query failures use `operational` errors. Checks cover account
and income-source links from transactions, transaction/category links from lines, both transfer
legs/accounts and their amount/currency compatibility, and canonical/candidate provenance
targets. It reports only and never repairs.

## Export

`tracky export --db PATH [--include-review-audit] --json` emits `tracky.export.v1`.
Arrays and entity keys have deterministic ordering.

The default `entities` are accounts, categories, income sources, canonical transactions,
transaction lines, imported and manual transfer pairs, minimal redacted imported provenance,
and manual-entry provenance. IDs, nullable fields, dates, currencies, and integer minor-unit
amounts are preserved; relations are not replaced with display names.

`--include-review-audit` additionally includes candidate transactions of every status, import
batches (without arbitrary importer error payloads), safe source-document metadata, and candidate provenance. Candidate status is the review
decision record exposed by this schema. Source input names, content hashes, raw evidence refs,
bounding boxes, credentials, passwords, environment secrets, and binary/raw evidence are never
exported. Only `evidence_text_redacted` and its redaction/storage policy may appear.

Issue 0021 deliberately excludes all investment tables and investment-specific integrity rules.
Those extend this seam only in issue 0033. Backup naturally preserves the complete SQLite file,
but 0021 makes no investment restore/export/integrity guarantee.

## Investment extension (`0033`)

The v1 envelopes remain compatible. Backup still copies the whole canonical SQLite database with the Online Backup API; there is no investment-specific archive or duplicate store. Restore verification opens the published database normally, runs `tracky integrity`, and compares the public `reports investments` JSON for an identical date range. Publication and cleanup retain the atomic, no-overwrite behavior described above.

The default export adds these deterministically ordered collections:

- `investment_instruments`; allocation revisions, heads, and consumptions;
- CDT positions, operation revisions, and heads;
- brokerage accounts, operation revisions, heads, and buy funding attributions;
- investment snapshots, snapshot positions, and snapshot baselines;
- investment adjustment revisions and heads;
- accepted provider-document events and minimal redacted investment provenance.

IDs and foreign-key identities are exported directly, never replaced by display names. Revision history and active heads remain separate and navigable. Exact quantities, prices, rates, and quantity deltas remain canonical decimal strings; cash, costs, fees, withholding, deductions and values remain integer minor units with their explicit currencies. Nullable economic dates, observation dates, fee links, cash-transaction links, correction links and provenance links are preserved. No derived `current_positions` collection is exported: public position/report commands replay the canonical revisions, heads and relationships.

By default only accepted provider-document events needed to interpret canonical investment data are included. `--include-review-audit` adds safe accepted, pending and rejected decision metadata. Both modes exclude input names, content hashes, raw evidence references, credentials, passwords, environment values, filesystem paths, binary evidence and arbitrary import error payloads. Only explicitly redacted evidence fields and safe parser/provider metadata cross the boundary.

Integrity now counts every investment collection and adds deterministic `broken_reference` and `invariant_violation` findings. Checks cover contribution/allocation/instrument/head/fee and consumption links; exact quantity syntax and allocation limits; CDT position, instrument, head, funding and lifecycle consistency; brokerage account, instrument, head, funding attribution and canonical replay consistency; snapshot positions, baselines, adjustment revisions/heads; and provider-event/snapshot provenance targets. Canonical brokerage and CDT replay functions detect impossible active state rather than introducing a second replay. Each finding exposes a stable `id` and findings are ordered by category, entity, then that ID. Findings only report; they never repair or migrate.

Export and integrity open SQLite read-only and do not write HOME, config, freshness, baseline or application state. This extension adds no migration, TUI state, or issue-0034 surface.
