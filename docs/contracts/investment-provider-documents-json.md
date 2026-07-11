# Investment provider documents JSON contract

Schema: `tracky.investment-documents.v1`.

Supported commands:

```text
tracky investment-documents inspect PDF [--password-env NAME] --json
tracky investment-documents import PDF --db PATH [--password-env NAME] --json
tracky investment-documents list --db PATH --json
tracky investment-documents review EVENT_ID --db PATH --decision reconcile_transaction|reject \
  [--reconciled-kind KIND --reconciled-id ID] --json
```

`inspect` is read-only. `import` writes one source document, one import batch, and pending
provider events in one SQLite transaction. Neither command creates canonical investment
operations. `review` is the only state transition and is single-use. A transaction
reconciliation requires an existing canonical investment contribution/owned transfer with the
same date, absolute amount, and currency; arbitrary ids are rejected.

Every response contains `schema_version`, `command`, `ok`, `events`, and `errors`. Events
preserve provider/parser versions, effective date, exact minor units or canonical decimal
quantity, page/row, redacted evidence, fingerprint, decision, and optional reconciliation link.

Supported artifact-derived formats are deliberately narrow:

- NU Cuenta 2026 statement: `Enviaste a Plenti`, `Abriste un CDT`, and
  `Recibiste dinero de un CDT`. The statement does not prove contractual CDT terms.
- Wenia monthly portfolio summary: positions that the extractor can deterministically read.
  It does not claim movement support.
- Plenti transactional statement: `Recarga Bre-B` and `Depósito amigo Plenti` rows.
  Its aggregate CDT balance is not promoted to contractual CDT terms.

Detection uses document content, never only the filename. Unsupported or recognized-but-
insufficient documents return `unsupported_document` or `partially_recognized_document`.
Exact document hashes and normalized event fingerprints are durable unique keys.

Real statements and credentials are never fixtures or committed artifacts. Tests use synthetic
redacted rows derived from the authorized document structures.
