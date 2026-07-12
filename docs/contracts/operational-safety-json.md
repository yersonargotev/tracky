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
batches, safe source-document metadata, and candidate provenance. Candidate status is the review
decision record exposed by this schema. Source input names, content hashes, raw evidence refs,
bounding boxes, credentials, passwords, environment secrets, and binary/raw evidence are never
exported. Only `evidence_text_redacted` and its redaction/storage policy may appear.

Issue 0021 deliberately excludes all investment tables and investment-specific integrity rules.
Those extend this seam only in issue 0033. Backup naturally preserves the complete SQLite file,
but 0021 makes no investment restore/export/integrity guarantee.
