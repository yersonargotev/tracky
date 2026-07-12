# Investment provider documents JSON contract

Schema: `tracky.investment-documents.v1`.

Nu Cuenta mixed inspection/import uses the compatible extension
`tracky.investment-documents.v2`. It retains every v1 field and adds
`ordinary_candidates[]`, whose entries use the existing review-first PDF candidate shape. Other
providers and event-review commands remain on v1.

Supported commands:

```text
tracky investment-documents inspect PDF [--password-env NAME] --json
tracky investment-documents import PDF --db PATH [--password-env NAME] --json
tracky investment-documents list --db PATH --json
tracky investment-documents inspect-event EVENT_ID --db PATH --json
tracky investment-documents candidates EVENT_ID --db PATH \
  --event-account-id PROVIDER_ACCOUNT_ID --counterpart-account-id ACCOUNT_ID --json
tracky investment-documents accept-snapshot EVENT_ID --db PATH \
  --account-id ACCOUNT_ID --instrument-id INSTRUMENT_ID --json
tracky investment-documents reconcile-deposit EVENT_ID --db PATH \
  --event-account-id PROVIDER_ACCOUNT_ID --counterpart-account-id ACCOUNT_ID \
  (--canonical-transaction-id ID | --provider-event-id ID) --json
tracky investment-documents reconcile-withdrawal EVENT_ID --db PATH \
  --event-account-id PROVIDER_ACCOUNT_ID --counterpart-account-id ACCOUNT_ID \
  (--canonical-transaction-id ID | --provider-event-id ID) --json
tracky investment-documents reject EVENT_ID --db PATH --json
tracky investment-documents cdt-actions EVENT_ID --db PATH --json
tracky investment-documents enrich-cdt EVENT_ID --db PATH --request-json JSON \
  (--dry-run | --apply) --json
```

`inspect` is read-only. `import` writes one source document, one import batch, and pending
provider events in one SQLite transaction. For a Nu Cuenta statement, that same transaction also
writes supported `ordinary_candidates`, their fingerprints, and provenance. Neither command creates canonical investment
operations. Typed movement reconciliation, rejection, and snapshot acceptance are single-use. Reconciliation requires the
uniquely selected compatible candidate. Candidate generation is read-only and checks direction,
event semantics, owned counterpart account, external reference when present, exact date, amount,
currency, and supported target kind. It reports
`unique_match`, `ambiguous_match`, `unmatched`, `already_reconciled`, or `incompatible`; zero or
multiple matches remain pending. Provider-event pairs are consumed atomically at both ends.

CDT enrichment uses `tracky.cdt-provider-enrichment.v1`. `cdt-actions` opens the database
read-only and reports imported date, currency, and amount separately from the exact reviewer
fields required for compatible constitution, renewal, redemption, or exact existing-operation
link actions. The tagged
`request-json` supplies one explicit action. `--dry-run` validates against an in-memory SQLite
backup; `--apply` atomically creates or exactly links the lifecycle operation, accepts the single-use event, and
records both immutable imported evidence and reviewer-supplied terms. A return's net cash comes
from the imported amount; principal, interest, deductions, maturity, rate, payment mode,
renewal terms, contract identifier, and allocation/position identities are never inferred.

Every response contains `schema_version`, `command`, `ok`, `events`, and `errors`. Events
preserve source-document and batch ids, canonical provenance id, provider/parser versions,
effective date, exact minor units or canonical decimal quantity, page/row, redacted evidence,
fingerprint, decision, and optional reconciliation or accepted-snapshot link. `inspect-event` is
a read-only expanded view of this chain, including canonical target details or accepted snapshot
position/baseline counts.

`accept-snapshot` is limited to complete `observed_position` rows. It requires a compatible owned
account and provider instrument, then creates the immutable issue-0030 snapshot, position,
original reconciliation baseline, provenance link, and review decision in one transaction.

Supported artifact-derived formats are deliberately narrow:

- NU Cuenta 2026 statement: signed, dated ordinary credit/debit movements plus `Enviaste a
  Plenti`, `Abriste un CDT`, and `Recibiste dinero de un CDT`. The three named provider forms emit
  only provider events. Ordinary rows remain pending candidates; demonstrated card-payment rows
  use `card_payment`, while other supported rows use `bank_movement`. The statement does not prove
  categories, income sources, counterparties, contractual CDT terms, or canonical transactions.
- Wenia monthly portfolio summary: positions that the extractor can deterministically read.
  It does not claim movement support.
- Plenti transactional statement: `Recarga Bre-B` and `Depósito amigo Plenti` rows.
  Its aggregate CDT balance is not promoted to contractual CDT terms.

Detection uses document content, never only the filename. Unsupported or recognized-but-
insufficient documents return `unsupported_document` or `partially_recognized_document`.
Exact document hashes and normalized event fingerprints are durable unique keys.
Ordinary Nu candidates use the existing normalized transaction fingerprints and possible-duplicate
workflow. Layout evidence is preferred when linear and coordinate-bearing extraction contain the
same row.

Real statements and credentials are never fixtures or committed artifacts. Tests use synthetic
redacted rows derived from the authorized document structures.
