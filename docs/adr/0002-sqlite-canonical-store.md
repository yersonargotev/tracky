# SQLite as the canonical store

Tracky will start with SQLite as the only canonical data store for transactions, accounts, categories, import batches, provenance, and app state. DuckDB remains a possible future analytics layer, but the initial product optimizes for reliable local CRUD, migrations, portability, and a simple CLI/TUI architecture.

## Considered Options

- **SQLite-only**: simplest local source of truth and best fit for the first version.
- **SQLite + DuckDB from day one**: better analytical headroom, but more moving parts before real data exists.
- **DuckDB as primary store**: attractive for analytics, but less natural as the transactional app database.

## Consequences

- Analysis initially runs through SQLite queries, views, exports, and indexes.
- The schema should stay analysis-friendly and exportable to CSV, Parquet, or DuckDB later.
- DuckDB integration should not appear in the first implementation slice unless the PDF spike proves it is necessary.
