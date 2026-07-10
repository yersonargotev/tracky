# PDF import agent workflow

This guide describes the current CLI/JSON path for Tracky's review-first PDF import milestone. It is intended for agents and humans who need to inspect a protected statement, import reviewable candidates, and explicitly accept or reject them without bypassing provenance or auditability.

## Guardrails

- `tracky pdf inspect` is read-only. It returns transient candidate-shaped JSON and does not write SQLite data.
- `tracky import pdf` writes a `SourceDocument`, `ImportBatch`, `Provenance`, duplicate markers, and **candidate transactions** only. It must not create canonical transactions.
- Canonical transactions appear only after an explicit `tracky candidates accept` action.
- Do not drop or hide provenance when reviewing candidates. Accepted and rejected decisions must remain auditable through the source document, import batch, parser/extractor evidence, and candidate id.
- Do not use real PDFs, unredacted account data, passwords, emails, addresses, counterparties, long identifiers, or full amounts as committed fixtures or examples.

## Runtime password handling

PDF passwords are document credentials supplied only at runtime. Tracky can read a password from an environment variable named by `--password-env`; the command records only the credential source, not the credential value.

```bash
export TRACKY_SAMPLE_PDF_PASSWORD='runtime-only-secret'
tracky pdf inspect ~/statements/redacted-sample.pdf \
  --password-env TRACKY_SAMPLE_PDF_PASSWORD \
  --json
```

The JSON should report a credential source such as `env` under `extractor_status.credential_source`. It must not print or persist the password. If the environment variable is missing, Tracky returns stable error JSON instead of requiring agents to parse prose.

## Happy path

### 1. Inspect the PDF without writing storage

Use `pdf inspect` first to check extraction/parser status and redacted candidate previews.

```bash
tracky pdf inspect ~/statements/redacted-sample.pdf \
  --password-env TRACKY_SAMPLE_PDF_PASSWORD \
  --json > /tmp/tracky-inspect.json
```

Agent-oriented checks:

```bash
jq '.ok, .schema_version, .extractor_status.status, .parser_status.status' /tmp/tracky-inspect.json
jq '.candidates[] | {id, status, duplicate_status, posted_date, amount_minor, currency, direction_hint, semantic_hint, provenance}' /tmp/tracky-inspect.json
```

Treat `candidates[]` from inspect as previews only. They are useful for validation and review planning, but they are not stored records and do not affect canonical finance data.

### 2. Import candidates into SQLite

After inspect output looks usable, run `import pdf` against a local SQLite database.

```bash
tracky import pdf ~/statements/redacted-sample.pdf \
  --db /tmp/tracky-review.sqlite \
  --password-env TRACKY_SAMPLE_PDF_PASSWORD \
  --json > /tmp/tracky-import.json
```

Expected successful output includes:

- `source_document` with document hash and `document_duplicate_status`.
- `import_batch` with candidate/error/duplicate counts.
- `candidates[]` with stored candidate ids.
- `provenance` on each candidate, including source document, parser/extractor metadata, page/row evidence, confidence, and redacted evidence text.

Import remains review-first: candidates are stored for review, but no canonical transactions are created by this command.

### 3. List candidates for a batch or status

Use the import batch id from the import response to drive review.

```bash
BATCH_ID=$(jq -r '.import_batch.id' /tmp/tracky-import.json)
tracky candidates list \
  --db /tmp/tracky-review.sqlite \
  --import-batch-id "$BATCH_ID" \
  --json > /tmp/tracky-candidates.json
```

Useful filters:

```bash
tracky candidates list --db /tmp/tracky-review.sqlite --status pending_review --json
tracky candidates list --db /tmp/tracky-review.sqlite --status possible_duplicate --json
```

### 4. Accept or reject explicitly

Accept only candidates that have been reviewed and whose provenance/evidence still supports the transaction.

```bash
tracky candidates accept --db /tmp/tracky-review.sqlite cand_REDACTED --json
```

Reject candidates that should not become canonical, while preserving audit history.

```bash
tracky candidates reject --db /tmp/tracky-review.sqlite cand_REDACTED --json
```

Accepting a candidate creates or links a canonical transaction and keeps the trace back to the candidate, source document, import batch, and provenance. Rejecting updates candidate state without deleting the evidence trail.

### 5. Accept explicit income inflows

Income review is explicit: first create or list stable income sources, then accept an eligible inflow with a chosen source and kind. Do not infer salary from a positive Nequi movement.

```bash
tracky income-sources create --db /tmp/tracky-review.sqlite \
  --name "REDACTED_EMPLOYER" \
  --json
tracky income-sources list --db /tmp/tracky-review.sqlite --json

tracky candidates accept-income cand_NEQUI_INFLOW_REDACTED \
  --db /tmp/tracky-review.sqlite \
  --income-source-id incsrc_REDACTED \
  --income-kind salary \
  --json
```

`accept-income` only accepts unreviewed `bank_movement` inflows with positive amounts. It refuses already accepted/rejected candidates, card-payment/card-charge rows, outflows, missing income sources, and inflows that match a resolved owned-account outflow pattern. Accepted canonical income keeps the candidate provenance link plus `transaction_kind: "income"`, `income_source_id`, and `income_kind`.

### 6. Accept explicit categorized expenses

Expense review is explicit: create or list categories, then accept an eligible purchase/outflow candidate with a chosen category. Do not infer categories during import.

```bash
tracky categories create --db /tmp/tracky-review.sqlite \
  --name "REDACTED_CATEGORY" \
  --json
tracky categories list --db /tmp/tracky-review.sqlite --json

tracky candidates accept-expense cand_PURCHASE_REDACTED \
  --db /tmp/tracky-review.sqlite \
  --category-id cat_REDACTED_CATEGORY \
  --json
```

`accept-expense` creates a canonical transaction marked `transaction_kind: "expense"` with one or more categorized transaction lines. The compatible `--category-id` form creates exactly one line; split lines must collectively reconcile with the canonical transaction amount. Nequi purchase outflows remain negative; RappiCard `card_charge` rows are accepted as card expenses and normalized to a negative outflow amount even if the source statement amount was positive. Income/inflows, `card_payment` rows, likely own-account transfers, missing categories, and already accepted/rejected candidates are refused.

For a mixed purchase, create a balanced split explicitly instead of using `--category-id`:

```bash
tracky candidates accept-expense cand_PURCHASE_REDACTED \
  --db /tmp/tracky-review.sqlite \
  --line cat_food:-1500000:COP \
  --line cat_delivery:-300000:COP \
  --json
```

To correct a previously accepted expense, replace all of its lines through its candidate id. The canonical transaction and provenance remain intact:

```bash
tracky candidates set-expense-lines cand_PURCHASE_REDACTED \
  --db /tmp/tracky-review.sqlite \
  --line cat_food:-1200000:COP \
  --line cat_delivery:-300000:COP \
  --line cat_household:-500000:COP \
  --json
```

Each line must name an existing distinct category, use the canonical currency, and collectively sum to the canonical amount in minor units.

### 7. Review likely own-account transfer/card-payment pairs

For card payments, first ask Tracky for likely pairs. For example, a Nequi PSE outflow can match a RappiCard `card_payment` row when both accounts are registered as owned, the date/currency match, and the absolute amount is the same.

```bash
tracky candidates list-transfer-pairs --db /tmp/tracky-review.sqlite --json
```

Accept a suggested pair explicitly:

```bash
tracky candidates accept-transfer-pair cand_NEQUI_REDACTED cand_RAPPI_REDACTED \
  --db /tmp/tracky-review.sqlite \
  --json
```

The accepted pair creates balancing canonical transfer legs marked `transaction_kind: "own_account_transfer"` and links both candidates through a transfer-pair record. Do not accept pairs with unresolved accounts, non-owned accounts, mismatched amounts/dates/currencies, or candidates that are already accepted/rejected.

## Manual canonical entries

Manual entries are an explicit non-PDF route for records missing from a supported statement. They do not inspect or import a PDF, and they do not create candidate transactions. Their audit metadata is `source: "manual_entry"`, deliberately distinct from PDF provenance.

Create a manual expense using a registered owned account and an explicit category:

```bash
tracky transactions add-expense --db /tmp/tracky-review.sqlite \
  --account-id acct_REDACTED --posted-date 2026-07-09 \
  --description "REDACTED_MANUAL_PURCHASE" --amount-minor -150000 --currency COP \
  --category-id cat_REDACTED --json
```

Create a manual income only with an explicit registered income source and kind:

```bash
tracky transactions add-income --db /tmp/tracky-review.sqlite \
  --account-id acct_REDACTED --posted-date 2026-07-09 \
  --description "REDACTED_MANUAL_INCOME" --amount-minor 500000 --currency COP \
  --income-source-id incsrc_REDACTED --income-kind salary --json
```

Create a manual own-account transfer with two distinct registered owned accounts. It creates equal and opposite canonical transfer legs, never income or expense:

```bash
tracky transactions add-transfer --db /tmp/tracky-review.sqlite \
  --from-account-id acct_REDACTED_FROM --to-account-id acct_REDACTED_TO \
  --posted-date 2026-07-09 --description "REDACTED_MANUAL_TRANSFER" \
  --amount-minor 200000 --currency COP --json
```

All manual commands require `--json`, a matching account currency, valid signs, and explicit categories/sources. Expenses may use balanced repeated `--line CATEGORY_ID:AMOUNT_MINOR:CURRENCY` values instead of `--category-id`.

## Status and duplicate interpretation

Candidate statuses:

| Status | Meaning | Agent action |
| --- | --- | --- |
| `pending_review` | Stored candidate with no known duplicate warning. | Review evidence before accepting or rejecting. |
| `possible_duplicate` | Candidate resembles another candidate or canonical transaction. | Do not auto-accept; compare provenance, fingerprint, date, amount, account, and description. |
| `accepted` | Candidate was explicitly accepted. | Treat as reviewed; do not accept again. |
| `rejected` | Candidate was explicitly rejected. | Preserve for audit; do not delete as cleanup. |

Semantic hints:

| Hint | Meaning | Agent action |
| --- | --- | --- |
| `bank_movement` | Regular bank/wallet movement. | Review with amount, direction, provenance, and future income/category rules. |
| `card_charge` | RappiCard purchase/subscription/fee/interest/installment; expense-like card activity even if the raw amount is positive. | Do not treat as income; accept as a categorized expense only with explicit `accept-expense`. |
| `card_payment` | RappiCard payment/liability reduction such as `PAGOS POR PSE`. | Keep distinct from purchases; future transfer/card-payment resolution should link it to the paying owned account. |

Duplicate statuses:

| Status | Meaning |
| --- | --- |
| `not_checked` | Duplicate detection did not run. |
| `unique` | No match was found. |
| `possible_duplicate` | A near or similar match needs human/agent review. |
| `exact_duplicate` | Normalized fingerprint matched an existing candidate or canonical transaction. |

Source document duplicate status is separate from transaction-level duplicate status. A `duplicate_source_document` response means the same file hash was already imported; Tracky should avoid creating another import batch or another copy of all candidates. Transaction-level `possible_duplicate` means the document may be new, but one or more movements resemble existing records and require review.

## Current limits and non-goals

The current milestone is CLI/JSON-first. Do not document or assume these as available in this slice:

- Full TUI review workflow.
- MCP server or MCP-specific tool wrapper.
- AI fallback for unsupported PDFs.
- Password storage, credential vault integration, or persisted document credentials.
- Import-side canonical transaction creation.
- Sensitive PDF fixtures or committed real financial data.

## Next likely slices

After the CLI path is documented and usable, likely follow-up slices are:

1. Keep the user-facing command reference in [`README.md`](../../README.md) aligned with this workflow and the JSON contract.
2. Improve candidate review ergonomics without changing the review-first contract, such as safer batch summaries or clearer duplicate comparison JSON.
3. Add broader parsers or fixture coverage using redacted row shapes only, not real PDFs.
4. Expand institution/account resolution conservatively at review time, keeping unresolved hints explicit.
5. Consider TUI or MCP wrappers only after the stable CLI JSON workflow remains the source of truth.

## Reference documents

- Domain glossary: `CONTEXT.md`
- Review-first ADR: `docs/adr/0001-review-first-import.md`
- SQLite ADR: `docs/adr/0002-sqlite-canonical-store.md`
- JSON contract: `docs/contracts/review-first-pdf-import-json.md`
- Milestone PRD: `docs/prd/review-first-pdf-import.md`

## Review canonical ledger entries

Use canonical ledger commands after acceptance or manual entry; they do not delete or rewrite PDF evidence:

```bash
tracky transactions list --db /tmp/tracky-review.sqlite --start-date 2026-07-01 --end-date 2026-07-31 --type expense --json
tracky transactions inspect txn_REDACTED --db /tmp/tracky-review.sqlite --json
tracky transactions update txn_REDACTED --db /tmp/tracky-review.sqlite --line cat_food:-120000:COP --line cat_home:-30000:COP --json
```

Updates retain the original candidate/source-document provenance or manual-entry provenance. Transfer legs may have their description corrected, but cannot be reclassified as income or expenses. Expense line updates must remain balanced in the canonical amount and currency.
