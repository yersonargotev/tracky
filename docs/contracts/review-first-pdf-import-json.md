# Review-first PDF import JSON contract

This document defines the stable JSON contract for the future `tracky pdf inspect` and `tracky import pdf` commands. Both commands expose PDF extraction as reviewable evidence: they may produce **transacciones candidatas**, but they must never create **transacciones canónicas** directly.

## Scope

| Command | Writes storage? | Creates candidates? | Creates canonical transactions? |
| --- | --- | --- | --- |
| `tracky pdf inspect` | No | Returns transient candidate-shaped results in JSON | Never |
| `tracky import pdf` | Yes, once implemented | Persists candidate transactions in an import batch | Never |
| `tracky accounts register/list` | Yes for register, no for list | No | Never |
| `tracky candidates list/reject` | Yes for reject, no for list | No | Never |
| `tracky candidates accept` | Yes | No new candidates | Legacy compatibility path only; refuses typed finance candidates and returns `required_command` |
| `tracky income-sources create/list` | Yes for create, no for list | No | Never |
| `tracky candidates accept-income` | Yes | No new candidates | Creates one canonical income transaction from one eligible candidate |
| `tracky categories create/list` | Yes for create, no for list | No | Never |
| `tracky candidates accept-expense` | Yes | No new candidates | Creates one canonical expense transaction and one or more balanced category lines from one eligible candidate |
| `tracky candidates accept-investment` | Yes | No new candidates | Creates one canonical investment contribution pending allocation from one eligible bank outflow |
| `tracky candidates set-expense-lines` | Yes | No new candidates | Replaces the category lines of an accepted canonical expense while preserving candidate provenance |
| `tracky candidates list-transfer-pairs` | No | No | Never |
| `tracky candidates accept-transfer-pair` | Yes | No new candidates | Creates two canonical own-account transfer legs from one eligible pair |
| `tracky candidates batch-summary/compare-duplicate/suggest-actions` | No | No | Never |
| `tracky candidates apply-actions --dry-run` | No | No | Never |
| `tracky candidates apply-actions` | Yes, atomically after full preflight | No new candidates | Rejects exact reviewed duplicates or creates validated transfer pairs only when explicit candidate ids are supplied |
| `tracky transactions add-expense/add-income/add-investment/add-transfer` | Yes | No | Creates direct manual canonical expense, income, or pending-allocation investment rows, or two balanced transfer legs, with distinct manual provenance |
| `tracky instruments create/list/inspect` | Create only | No | Never; manages stable investment-instrument identities |
| `tracky investments allocate/replace-allocation` | Yes | No | Links confirmed contribution principal to exact acquired quantities with append-only audit revisions |
| `tracky investments inspect-contribution/positions` | No | No | Never; derives remaining principal and historical-cost positions |

Out of scope for this contract: parser implementation, SQLite migration internals, TUI review, MCP wrappers, AI fallback, and password storage.

## Investment instruments, allocations, and positions JSON

All commands in this section require `--json` and return `schema_version: "tracky.investments.v1"`.

`instruments create` accepts `--name`, `--type`, `--denomination-currency`, `--provider`, and optional `--provider-identifier`. Supported types are `fiat_currency`, `dollar_referenced_digital_asset`, `security`, `fixed_income`, and `generic`. `instruments list` returns `instruments[]`; `instruments inspect --instrument-id ID` returns one `instrument`.

`investments allocate` accepts a confirmed `--contribution-id` plus either one leg through `--instrument-id`, positive `--cash-amount-minor`, matching `--cash-currency`, and positive plain-decimal `--quantity`, or one JSON array through `--allocations-json`. Every leg is validated before the set commits, so an invalid leg leaves the complete action unapplied. Optional fees require `--fee-amount-minor`, `--fee-currency`, `--fee-treatment capitalized|separate`, and `--fee-component-id`; `separate` additionally requires `--fee-expense-transaction-id` referencing a matching canonical expense created with the same `transactions add-expense --investment-fee-component-id`. The durable component identity prevents that expense from being capitalized and prevents a capitalized component from later being created as a linked expense. The response always includes active `allocations[]`, complete `allocation_history[]`, `allocation_status`, and `unallocated_amount_minor`.

`investments replace-allocation` accepts the same economic fields plus `--allocation-id` and required `--reason`. It appends a revision and atomically advances the active head; it never overwrites history. Over-allocation or incompatible input returns `ok: false` without a revision or head change.

Each allocation preserves `cash_amount_minor`, `cash_currency`, canonical-string `acquired_quantity`, `instrument_id`, fee fields, provenance source, and an exact `effective_rate` ratio:

```json
{
  "cash_amount_minor": 600000,
  "cash_currency": "COP",
  "acquired_quantity": "150.125",
  "instrument_id": "instr_example",
  "fee_amount_minor": 1000,
  "fee_currency": "COP",
  "fee_treatment": "capitalized",
  "fee_component_id": "fee_component_example",
  "fee_expense_transaction_id": null,
  "effective_rate": {
    "cost_minor_numerator": 600000,
    "cost_currency": "COP",
    "quantity_denominator": "150.125",
    "instrument_id": "instr_example"
  }
}
```

`investments positions [--account-id ID]` returns positions grouped by account, instrument, and cost currency with exact `quantity`, `accumulated_cost_minor`, `cost_currency`, and `latest_contributing_operation_id`. It performs no market valuation or implicit currency conversion.

## Top-level responses

### `tracky pdf inspect`

`pdf inspect` is read-only. It opens one document source, runs extraction/parsing, and returns the document, statuses, candidate transaction previews, and errors if any.

```json
{
  "schema_version": "tracky.pdf-inspect.v1",
  "command": "pdf inspect",
  "ok": true,
  "source_document": { "...": "SourceDocument" },
  "extractor_status": { "...": "ExtractorStatus" },
  "parser_status": { "...": "ParserStatus" },
  "candidates": [{ "...": "CandidateTransaction" }],
  "errors": []
}
```

### `tracky import pdf`

`import pdf` persists a `SourceDocument`, an `ImportBatch`, `Provenance`, and candidate transactions for later review. It does not accept, reject, or promote candidates.

```json
{
  "schema_version": "tracky.import-pdf.v1",
  "command": "import pdf",
  "ok": true,
  "import_batch": { "...": "ImportBatch" },
  "source_document": { "...": "SourceDocument" },
  "extractor_status": { "...": "ExtractorStatus" },
  "parser_status": { "...": "ParserStatus" },
  "candidates": [{ "...": "CandidateTransaction" }],
  "errors": []
}
```

For exact duplicate source documents, `ok` should be `false` unless a future explicit reimport flag changes behavior. The response should still include `source_document.document_duplicate_status` and a stable `duplicate_source_document` error.

## Shared envelope rules

| Field | Type | Meaning |
| --- | --- | --- |
| `schema_version` | string | Stable contract identifier. Change only when the external JSON shape changes incompatibly. |
| `command` | string | One of `pdf inspect` or `import pdf`. |
| `ok` | boolean | `true` when the command completed its intended action without stable errors. Partial extraction/parser problems may set this to `false` with error details. |
| `source_document` | `SourceDocument` | The document source being inspected/imported. |
| `extractor_status` | `ExtractorStatus` | PDF opening and text/layout extraction result. |
| `parser_status` | `ParserStatus` | Institution-specific parser result over extracted evidence. |
| `import_batch` | `ImportBatch` or omitted | Present for `import pdf`; omitted for read-only inspect. |
| `candidates` | array of `CandidateTransaction` | Reviewable candidate transactions, not canonical transactions. |
| `errors` | array of `TrackyError` | Stable machine-readable errors. Empty on success. |

## Domain objects

The headings in this section name stable JSON schema aliases. They map to Tracky's Spanish domain vocabulary from `CONTEXT.md`:

| JSON alias | Domain term |
| --- | --- |
| `CandidateTransaction` | **transacción candidata** |
| `SourceDocument` | **documento fuente** |
| `ImportBatch` | **lote de importación** |
| `Provenance` | **provenance** |

The Spanish terms remain the product/domain language; the English aliases are used only where the JSON contract needs implementation-friendly type names.

### SourceDocument

A `SourceDocument` represents the **documento fuente** such as a protected statement PDF. It identifies the file and reports document-level duplicate status without storing a **credencial de documento**.

```json
{
  "id": "srcdoc_01JZ0000000000000000000000",
  "input_name": "nequi-mayo.pdf",
  "content_sha256": "hex-encoded-sha256",
  "mime_type": "application/pdf",
  "byte_size": 123456,
  "institution_hint": "nequi",
  "account_hint": {
    "label": "Nequi wallet",
    "currency": "COP",
    "masked_identifier": "***1234"
  },
  "document_duplicate_status": {
    "status": "new",
    "matched_source_document_id": null,
    "reason": null
  }
}
```

`document_duplicate_status.status` values:

| Value | Meaning |
| --- | --- |
| `new` | No existing source document has the same content hash. |
| `duplicate_source_document` | The same document hash was already imported. `import pdf` should report a stable error and avoid creating another batch. |
| `unknown` | Duplicate lookup was not available, such as in read-only inspect without a database. |

### ImportBatch

An `ImportBatch` groups candidates created by one `import pdf` run so the user can review them together.

```json
{
  "id": "batch_01JZ0000000000000000000000",
  "source_document_id": "srcdoc_01JZ0000000000000000000000",
  "started_at": "2026-07-04T23:50:00Z",
  "completed_at": "2026-07-04T23:50:03Z",
  "status": "completed",
  "candidate_count": 12,
  "error_count": 0,
  "duplicate_count": 2
}
```

`ImportBatch.status` values:

| Value | Meaning |
| --- | --- |
| `completed` | Extraction and parsing finished and candidates were created. |
| `completed_with_errors` | Some candidates or evidence were produced, but stable errors also occurred. |
| `failed` | No candidates were created because extraction, parsing, validation, or duplicate-document checks failed. |

`duplicate_count` counts candidate transactions in the batch whose `duplicate_status.status` is `possible_duplicate` or `exact_duplicate`.

### CandidateTransaction

A `CandidateTransaction` is a **transacción candidata**. It is reviewable data only; it does not affect reports, balances, categories, transfers, income sources, or other canonical state until a later review command accepts it.

```json
{
  "id": "cand_01JZ0000000000000000000000",
  "import_batch_id": "batch_01JZ0000000000000000000000",
  "source_document_id": "srcdoc_01JZ0000000000000000000000",
  "status": "pending_review",
  "duplicate_status": {
    "status": "unique",
    "fingerprint": "normalized-account-date-amount-description-hash",
    "matched_candidate_ids": [],
    "matched_canonical_transaction_ids": [],
    "reason": null
  },
  "institution_hint": "nequi",
  "account_hint": {
    "label": "Nequi wallet",
    "currency": "COP",
    "masked_identifier": "***1234"
  },
  "posted_date": "2026-05-31",
  "description": "Redacted merchant or counterparty",
  "amount_minor": -4590000,
  "currency": "COP",
  "balance_minor": 12500000,
  "direction_hint": "outflow",
  "semantic_hint": "bank_movement",
  "confidence": 0.91,
  "provenance": { "...": "Provenance" },
  "validation_warnings": []
}
```

`CandidateTransaction.status` values:

| Value | Meaning |
| --- | --- |
| `pending_review` | Candidate is ready for human review and has no known duplicate warning. |
| `possible_duplicate` | Candidate resembles another candidate or canonical transaction and needs duplicate resolution. |
| `accepted` | Candidate has been accepted by a future review command; acceptance may create or link a canonical transaction outside this import command. |
| `rejected` | Candidate has been rejected by a future review command and should not become canonical. |

`direction_hint` remains a coarse sign/direction hint (`inflow` or `outflow`). For RappiCard statements, use `semantic_hint` to avoid interpreting card rows as ordinary income or expenses from sign alone.

`semantic_hint` values:

| Value | Meaning |
| --- | --- |
| `bank_movement` | Regular bank/wallet movement whose amount sign can be reviewed with the existing direction hint. |
| `card_charge` | Credit-card purchase, subscription, fee, interest, installment, restaurant, or supermarket row. These are expense-like card charges even when the raw statement amount is positive. |
| `card_payment` | Credit-card payment/liability reduction such as `PAGOS POR PSE`; it is distinct from purchases and should not be treated as ordinary income. |

`duplicate_status.status` values:

| Value | Meaning |
| --- | --- |
| `not_checked` | Duplicate detection did not run. |
| `unique` | No matching **huella de transacción** was found. |
| `possible_duplicate` | Similar candidate or canonical transaction found; keep candidate review-first. |
| `exact_duplicate` | Normalized fingerprint matches an existing candidate or canonical transaction exactly. |

When `duplicate_status.status` is `possible_duplicate` or `exact_duplicate`, the candidate `status` should be `possible_duplicate` unless it has already been reviewed as `accepted` or `rejected`.

### Provenance

`Provenance` explains why a candidate exists and how to audit it. JSON output should use redacted evidence by default.

```json
{
  "source_document_id": "srcdoc_01JZ0000000000000000000000",
  "page_number": 2,
  "row_index": 17,
  "bbox": {
    "x": 42.1,
    "y": 510.4,
    "width": 496.0,
    "height": 12.0,
    "unit": "pdf_point"
  },
  "extractor": {
    "name": "pdf_oxide",
    "version": "runtime-version-or-null"
  },
  "parser": {
    "id": "nequi.statement.v1",
    "version": "1"
  },
  "evidence": {
    "redaction": "redacted",
    "text": "2026-05-31 REDACTED_COUNTERPARTY -$REDACTED balance $REDACTED",
    "raw_storage_policy": "local_only_optional"
  },
  "confidence": 0.91
}
```

`raw_storage_policy` values:

| Value | Meaning |
| --- | --- |
| `not_stored` | Raw evidence was discarded after parsing. |
| `local_only_optional` | Raw evidence may be stored locally for audit, but should not appear in normal agent-facing JSON. |
| `redacted_only` | Only redacted evidence is kept. |

## Extractor and parser statuses

### ExtractorStatus

```json
{
  "status": "succeeded",
  "extractor": "pdf_oxide",
  "pages_seen": 4,
  "pages_extracted": 4,
  "requires_document_credential": true,
  "credential_source": "env",
  "warnings": []
}
```

`ExtractorStatus.status` values:

| Value | Meaning |
| --- | --- |
| `not_run` | Extraction was skipped, usually after validation or duplicate-document failure. |
| `succeeded` | PDF opened and text/layout evidence was extracted. |
| `partial` | Some pages or evidence failed but parsing may continue with available evidence. |
| `failed` | Extraction failed and parser should not run. |

`credential_source` values are `none`, `cli_flag`, `prompt`, `env`, or `unknown`. They describe how a runtime-only credential was supplied, not the credential value.

### ParserStatus

```json
{
  "status": "succeeded",
  "parser_id": "nequi.statement.v1",
  "parser_version": "1",
  "candidates_found": 12,
  "candidates_valid": 12,
  "warnings": []
}
```

`ParserStatus.status` values:

| Value | Meaning |
| --- | --- |
| `not_run` | Parser did not run because extraction or validation failed. |
| `succeeded` | Parser completed and all produced candidates passed validation. |
| `partial` | Parser produced some candidates plus warnings or invalid rows. |
| `failed` | Parser could not produce usable candidates from extracted evidence. |
| `unsupported_document` | No deterministic parser matched the document source. |

## Income source and income acceptance JSON

`tracky income-sources create/list --json` uses `tracky.income-sources.v1` and returns stable income source records independent from deposit accounts:

```json
{
  "schema_version": "tracky.income-sources.v1",
  "command": "income-sources create",
  "ok": true,
  "income_source": { "id": "incsrc_redacted_employer", "name": "REDACTED_EMPLOYER" },
  "income_sources": [],
  "errors": []
}
```

`tracky candidates accept-income CANDIDATE_ID --income-source-id ID --income-kind KIND --json` uses `tracky.candidate-review.v1`. It accepts only unreviewed positive `bank_movement` inflows and writes a canonical transaction with `transaction_kind: "income"`, `income_source_id`, `income_kind`, and the original candidate/provenance link. Supported first-slice income kinds are `salary`, `freelance`, `client_payment`, `sale`, `interest`, `reimbursement`, and `other`.

Stable refusal codes include `candidate_not_income_eligible`, `candidate_possible_own_account_transfer`, `candidate_already_accepted`, `candidate_already_rejected`, `income_source_not_found`, and `invalid_income_kind`.

## Category and expense acceptance JSON

`tracky categories create/list --json` uses `tracky.categories.v1` and returns stable category records used by transaction lines:

```json
{
  "schema_version": "tracky.categories.v1",
  "command": "categories create",
  "ok": true,
  "category": { "id": "cat_redacted_category", "name": "REDACTED_CATEGORY" },
  "categories": [],
  "errors": []
}
```

`tracky candidates accept-expense CANDIDATE_ID --category-id ID --json` uses `tracky.candidate-review.v1`. It accepts only unreviewed purchase/outflow candidates with an explicit category and keeps the single-line form for compatibility. A split uses repeated `--line CATEGORY_ID:AMOUNT_MINOR:CURRENCY` values; it creates one canonical transaction with `transaction_kind: "expense"` and stable `transaction_lines[]` entries containing `id`, `canonical_transaction_id`, `category_id`, `category_name`, `amount_minor`, `currency`, and `line_kind`.

Every split line must use an existing distinct category and the canonical transaction currency; the minor-unit sum must equal the canonical transaction amount. `tracky candidates set-expense-lines CANDIDATE_ID --line ... --json` replaces those lines only after the candidate has been accepted as an expense. It leaves the canonical transaction, candidate, source document, and provenance links intact. Stable split validation codes include `expense_lines_required`, `expense_lines_unbalanced`, `expense_line_currency_mismatch`, `expense_line_category_required`, and `category_not_found`.

Eligible first-slice expense candidates are:

- `bank_movement` outflows with negative amounts, such as Nequi purchases.
- `card_charge` outflows, such as RappiCard purchases/subscriptions/fees, even when the source statement amount is positive; the canonical expense and its line are normalized to a negative outflow amount.

`accept-expense` refuses income/inflows, `card_payment` rows, likely own-account transfer outflows that match an unreviewed owned counterparty candidate (including card-payment rows or bank/wallet inflows), missing categories, and already accepted/rejected candidates. Stable refusal codes include `candidate_not_expense_eligible`, `candidate_possible_own_account_transfer`, `candidate_already_accepted`, `candidate_already_rejected`, and `category_not_found`.

## Investment contribution acceptance JSON

`tracky candidates accept-investment CANDIDATE_ID --json` uses `tracky.candidate-review.v1`. It accepts only an unreviewed negative `bank_movement` outflow that is not a likely own-account transfer. The canonical row keeps the candidate account, date, signed amount, currency, description, and provenance link, with `transaction_kind: "investment_contribution"` and `investment_allocation_status: "pending_allocation"`. Stable refusal codes include `candidate_not_investment_eligible`, `candidate_possible_own_account_transfer`, `candidate_already_accepted`, and `candidate_already_rejected`.

## Manual canonical transaction JSON

Manual entry is an explicit route outside PDF inspection/import. All commands require `--json` and return `schema_version: "tracky.manual-transactions.v1"`; they never create source documents, import batches, or candidates.

| Command | Writes storage? | Canonical result |
| --- | --- | --- |
| `tracky transactions add-expense --db <PATH> --account-id ID --posted-date YYYY-MM-DD --description TEXT --amount-minor NEGATIVE --currency CODE --category-id ID [--investment-fee-component-id ID] --json` | Yes | One `expense` canonical transaction and one categorized line. `--line CATEGORY_ID:AMOUNT_MINOR:CURRENCY` may be repeated for a balanced split instead of `--category-id`. The optional fee identity cannot reference a capitalized component. |
| `tracky transactions add-income --db <PATH> --account-id ID --posted-date YYYY-MM-DD --description TEXT --amount-minor POSITIVE --currency CODE --income-source-id ID --income-kind KIND --json` | Yes | One `income` canonical transaction. |
| `tracky transactions add-investment --db <PATH> --account-id ID --posted-date YYYY-MM-DD --description TEXT --amount-minor NEGATIVE --currency CODE --json` | Yes | One `investment_contribution` canonical transaction with `investment_allocation_status: "pending_allocation"`. |
| `tracky transactions add-transfer --db <PATH> --from-account-id ID --to-account-id ID --posted-date YYYY-MM-DD --description TEXT --amount-minor POSITIVE --currency CODE --json` | Yes | Two balancing `own_account_transfer` canonical legs and one manual transfer-pair record. |

Each successful response includes `canonical_transactions[]` and distinct manual audit metadata:

```json
{
  "schema_version": "tracky.manual-transactions.v1",
  "command": "transactions add-expense",
  "ok": true,
  "canonical_transactions": [{ "transaction_kind": "expense", "created_from_candidate_id": null }],
  "transaction_lines": [{ "category_id": "cat_REDACTED", "amount_minor": -150000, "currency": "COP" }],
  "provenance": [{ "source": "manual_entry", "entry_id": "manual_REDACTED" }],
  "errors": []
}
```

Manual commands require registered owned accounts whose currency matches the submitted currency. Expenses and investment contributions must use negative minor units; income and transfer amounts must be positive. Expense categories and income sources must exist, and split expense lines must reconcile exactly. Transfers require two distinct owned accounts and write one negative and one positive leg of the same amount/currency; they are not income or expenses. Stable validation codes include `owned_account_not_found`, `account_currency_mismatch`, `invalid_amount_sign`, `income_source_not_found`, `invalid_income_kind`, `category_not_found`, `expense_lines_unbalanced`, and `transfer_accounts_must_differ`.

## Stable errors

Errors must be safe for scripts and agents to branch on. Human wording can improve over time, but `code`, `category`, and `path` semantics should stay stable within a schema version.

```json
{
  "category": "extractor_failure",
  "code": "pdf_open_failed",
  "message": "PDF extraction failed before candidate transactions could be produced.",
  "path": "extractor_status",
  "recoverable": true,
  "details": {
    "extractor": "pdf_oxide",
    "credential_required": true
  }
}
```

Required `TrackyError.category` values:

| Category | Typical codes | Meaning |
| --- | --- | --- |
| `extractor_failure` | `pdf_open_failed`, `pdf_text_extraction_failed`, `pdf_layout_extraction_failed` | The document source could not be opened or converted into usable text/layout evidence. |
| `parser_failure` | `unsupported_document`, `movement_rows_not_found`, `ambiguous_row_shape` | Extracted evidence could not be parsed into candidate transactions. |
| `validation_failure` | `missing_posted_date`, `invalid_amount`, `currency_mismatch`, `invalid_candidate_shape` | A parsed candidate or response object failed contract/domain validation. |
| `duplicate_source_document` | `source_document_already_imported` | The same document source hash was already imported; do not create another batch or candidates. |

A command may return multiple errors. For example, a partial parser run can return candidates plus `validation_failure` errors for rejected rows. A duplicate source document should be reported before extraction whenever the database can identify it by hash.


## Owned account registry CLI JSON contract

Issue 0011 adds a small registry for accounts that belong to the user. These commands are separate from PDF inspection/import and candidate review; they only maintain account metadata used for conservative account resolution.

| Command | Writes storage? | Creates canonical transactions? |
| --- | --- | --- |
| `tracky accounts register --db <PATH> --institution <NAME> --label <LABEL> --account-type <TYPE> --currency <CODE> [--masked-identifier <MASKED>] --json` | Yes | Never |
| `tracky accounts list --db <PATH> --json` | No | Never |

Both commands return `schema_version: "tracky.accounts.v1"` and machine-readable JSON. `accounts register` returns one `account`; `accounts list` returns `accounts[]`.

```json
{
  "schema_version": "tracky.accounts.v1",
  "command": "accounts register",
  "ok": true,
  "account": {
    "id": "acct_REDACTED",
    "institution_id": "inst_nequi",
    "institution": "nequi",
    "label": "Nequi wallet",
    "account_type": "wallet",
    "currency": "COP",
    "masked_identifier": null
  },
  "accounts": [],
  "errors": []
}
```

PDF import remains review-first. When an imported source document or candidate has an institution/account hint, Tracky links it to a registered owned account only when institution, normalized label/type, currency, and any provided masked identifier identify exactly one account. If no account matches, or more than one account matches, `account_id` remains null and the original account hint remains stored for review.

## Redaction and wording expectations

- Use domain vocabulary consistently: **transacción candidata**, **documento fuente**, **provenance**, **posible duplicado**, **credencial de documento**, **lote de importación**, and **huella de transacción**.
- Agent-facing JSON should include redacted evidence by default. Do not expose full account numbers, document credentials, long identifiers, emails, addresses, or unredacted counterparties when a redacted value is enough for review.
- `message` fields should be concise, actionable, and non-blaming. Prefer "PDF extraction failed before candidate transactions could be produced" over implementation-specific stack traces.
- Use `details` for structured diagnostics. Do not require agents to parse prose.
- Preserve provenance even for low-confidence candidates when safe, because review-first imports depend on auditability.

## Runtime-only document credentials

A **credencial de documento** may be supplied through a CLI flag, prompt, or environment variable loaded from a `.env`-style file, but Tracky must not persist the credential in `SourceDocument`, `ImportBatch`, `CandidateTransaction`, `Provenance`, logs, or normal JSON output.

The contract may report:

```json
{
  "requires_document_credential": true,
  "credential_source": "env"
}
```

The contract must not report:

```json
{
  "password": "1234",
  "document_credential": "1234"
}
```

## Review-first invariants

- `pdf inspect` is read-only and creates no stored rows.
- `import pdf` creates only source/import/provenance/candidate records needed for review.
- Neither command creates **transacciones canónicas** directly.
- `accepted` and `rejected` are candidate review states for future review commands, not states that `import pdf` should assign during first import.
- Possible duplicates are flagged, not auto-merged.
- Canonical transaction creation requires an explicit future review/accept action that preserves provenance.

## Candidate review CLI JSON contract

Issue 0007 adds the first explicit review actions. These commands are separate from `pdf inspect` and `import pdf`; they operate only on rows already persisted in SQLite.

| Command | Writes storage? | Creates canonical transactions? |
| --- | --- | --- |
| `tracky candidates list --db <PATH> --json` | No | Never |
| `tracky candidates accept <CANDIDATE_ID> --db <PATH> --json` | Yes | Only for a legacy non-specialized candidate; typed candidates are refused without mutation |
| `tracky candidates reject <CANDIDATE_ID> --db <PATH> --json` | Yes | Never |

All three commands return `schema_version: "tracky.candidate-review.v1"` and machine-readable JSON. The legacy `accept` path refuses positive `bank_movement` inflows with `required_command: "candidates accept-income"`, `card_payment` and transfer-like candidates with `required_command: "candidates accept-transfer-pair"`, and purchase candidates with `required_command: "candidates accept-expense"`. These refusals do not change candidate status or create canonical transactions. `reject` sets the candidate to `rejected` without deleting provenance, fingerprints, or duplicate markers. Re-accepting an accepted candidate returns a stable `candidate_already_accepted` error.

## Own-account transfer/card-payment review JSON contract

Issue 0012 adds explicit review of likely owned-account transfer pairs. Tracky may suggest pairs, but accepting the pair is still a separate review action.

| Command | Writes storage? | Creates canonical transactions? |
| --- | --- | --- |
| `tracky candidates list-transfer-pairs --db <PATH> --json` | No | Never |
| `tracky candidates accept-transfer-pair <FROM_CANDIDATE_ID> <TO_CANDIDATE_ID> --db <PATH> --json` | Yes | Yes, two canonical transfer legs linked by one `canonical_transfer_pairs` row |

Both commands return `schema_version: "tracky.transfer-review.v1"` and machine-readable JSON. Suggested pairs require:

- both candidates are `pending_review` or `possible_duplicate`;
- both candidates have resolved account IDs;
- both resolved accounts are registered as owned accounts;
- the source candidate is an outflow `bank_movement`;
- the destination candidate is a `card_payment`;
- posted date, absolute amount, and currency match.

Accepted pairs set both candidates to `accepted`, create two canonical rows with `transaction_kind: "own_account_transfer"`, normalize the canonical leg amounts to a balancing transfer outflow/inflow pair, and preserve each candidate's provenance link. Reports must treat these rows as transfers/card payments, not income or expense.

## Canonical transaction ledger JSON

`tracky transactions list/inspect/update --json` uses `tracky.transactions.v1`. These commands operate only on canonical transactions and never delete records, source documents, candidates, provenance, or fingerprints.

| Command | Purpose |
| --- | --- |
| `transactions list` | Lists canonical transactions; optional `--start-date`, `--end-date`, `--account-id`, `--category-id`, `--income-source-id`, and `--type` filters compose with AND semantics. |
| `transactions inspect TRANSACTION_ID` | Returns one canonical transaction, its PDF candidate link when present, manual or PDF provenance, category split lines, and transfer-pair metadata. |
| `transactions update TRANSACTION_ID` | Safely updates `--description`, income metadata (`--income-source-id`, `--income-kind`), one expense `--category-id`, or balanced repeated expense `--line` values. |

A successful inspection/update returns `canonical_transaction`, optional `candidate`, `transaction_lines`, `provenance`, and optional `transfer`. A list returns `canonical_transactions[]`. Manual provenance remains `{ "source": "manual_entry", "entry_id": "..." }`; imported records retain their candidate provenance and redacted evidence.

`update` refuses empty descriptions, missing income sources, unsupported income kinds, unbalanced/mismatched expense lines, and category/income changes to transfer legs. It does not change a transaction kind, candidate link, or provenance. Every successful update writes an append-only `edits[]` audit record with before/after change data and timestamp, returned by inspect/update.

## Canonical finance report JSON

`tracky reports summary --db <PATH> --start-date YYYY-MM-DD --end-date YYYY-MM-DD --json` uses `tracky.finance-report.v1`. Both range endpoints are required and inclusive. The command reads canonical transactions and transfer-pair records only; candidate rows in `pending_review`, `possible_duplicate`, or `rejected` state cannot affect the report.

Because canonical accounts may use different currencies, the report never combines unlike currencies into one number. `totals[]`, `category_totals[]`, `income_source_totals[]`, `excluded_transfer_totals[]`, and `investment_contribution_totals[]` are deterministically ordered and identify their currency explicitly. Expense and investment-contribution totals are positive magnitudes even though their canonical rows use negative amounts. Investment principal appears only in `investment_contribution_totals[]`; it does not affect income, expense, category, or net-cash-flow totals.

```json
{
  "schema_version": "tracky.finance-report.v1",
  "command": "reports summary",
  "ok": true,
  "date_range": {
    "start_date": "2026-06-01",
    "end_date": "2026-06-30"
  },
  "totals": [
    {
      "currency": "COP",
      "total_income_minor": 500000,
      "total_expenses_minor": 200000,
      "net_cash_flow_minor": 300000,
      "excluded_transfer_total_minor": 190000,
      "excluded_transfer_count": 2
    }
  ],
  "category_totals": [
    {
      "category_id": "cat_synthetic_food",
      "category_name": "Synthetic food",
      "currency": "COP",
      "total_expenses_minor": 170000
    }
  ],
  "income_source_totals": [
    {
      "income_source_id": "incsrc_synthetic_employer",
      "income_source_name": "Synthetic employer",
      "currency": "COP",
      "total_income_minor": 500000
    }
  ],
  "excluded_transfer_totals": [
    {
      "transfer_kind": "card_payment",
      "currency": "COP",
      "total_amount_minor": 70000,
      "transfer_count": 1
    },
    {
      "transfer_kind": "own_account_transfer",
      "currency": "COP",
      "total_amount_minor": 120000,
      "transfer_count": 1
    }
  ],
  "investment_contribution_totals": [
    {
      "currency": "COP",
      "total_contributed_minor": 2000000,
      "contribution_count": 1
    }
  ],
  "errors": []
}
```

An expense split contributes each transaction line once to its category while the canonical transaction contributes once to overall expenses. Own-account transfers and reviewed card-payment pairs never contribute to income, expenses, category totals, or net cash flow. Their positive pair amount is counted once in excluded transfer totals rather than summing or double-counting their balancing canonical legs. Stable report validation codes include `json_output_required`, `invalid_start_date`, `invalid_end_date`, and `invalid_date_range`.

## Batch review ergonomics JSON

Issue 0019 adds stateless commands for reviewing large import batches. Every command requires `--json` and returns `schema_version: "tracky.batch-review.v1"`. Suggestions have deterministic ids for correlation, but are not saved and cannot be applied by id in this slice; explicit candidate ids remain mandatory.

| Command | Writes SQLite? | Purpose |
| --- | --- | --- |
| `candidates batch-summary --import-batch-id ID [--largest-limit 10]` | No; opens the existing database read-only | Group one import batch and show the largest absolute movements. |
| `candidates compare-duplicate CANDIDATE_ID` | No; opens the existing database read-only | Compare the candidate with matched candidates, canonical records, fingerprints, duplicate markers, and redacted provenance/evidence. |
| `candidates suggest-actions --import-batch-id ID` | No; opens the existing database read-only | Explain obvious duplicate rejections and structurally validated owned-account transfer pairs without applying them. A transfer pair is included when either candidate belongs to the selected batch, including pairs spanning two import batches. |
| `candidates apply-actions --action ACTION [--action ACTION ...] --dry-run` | No; opens the existing database read-only | Preflight every explicit action and return per-action validation results. |
| `candidates apply-actions --action ACTION [--action ACTION ...]` | Yes | Preflight the complete set, then apply all actions in one SQLite transaction or none. |

The batch summary uses deterministic `{ "key", "count" }` arrays for `by_status`, `by_duplicate_status`, `by_institution`, `by_account_resolution`, `by_direction_hint`, and `by_semantic_hint`. `largest_amounts` is ordered by descending `absolute_amount_minor` and then ascending `candidate_id`; each item includes candidate id, date, description, signed and absolute minor amount, currency, and status. `--largest-limit` defaults to 10 and must be greater than zero.

Duplicate comparison returns the source `candidate`, `matched_candidates[]`, `matched_canonical_transactions[]`, `fingerprints[]`, and `duplicate_markers[]`. A matched canonical entry includes its canonical transaction, optional originating candidate, and PDF or manual provenance. Normal agent-facing evidence remains redacted.

Suggested actions use this stable shape:

```json
{
  "id": "suggest_REDACTED",
  "proposed_action": "reject_duplicate",
  "candidate_ids": ["cand_REDACTED"],
  "import_batch_ids": ["batch_REDACTED"],
  "reason": "exact_fingerprint_matches_reviewed_record",
  "evidence": {
    "duplicate_status": "exact_duplicate",
    "fingerprint": "fp_REDACTED",
    "matched_canonical_transaction_ids": ["txn_REDACTED"]
  }
}
```

Transfer suggestions contain both candidate ids and their aligned `import_batch_ids`, plus the structured transfer evidence (`posted_date`, `amount_minor`, `currency`, account ids, semantic hints, and redacted provenance for both candidates). Suggestion ids remain stable for the ordered candidate pair; suggestions are never persisted.

`reject_duplicate` is suggested only for an unreviewed exact fingerprint match to a canonical transaction or accepted candidate; a description similarity alone is insufficient. `accept_transfer_pair` is suggested only when the existing individual transfer validation confirms distinct resolved owned accounts plus matching date, absolute amount, currency, direction, and semantic hints.

Apply accepts repeated explicit action values:

- `--action reject-duplicate:CANDIDATE_ID`
- `--action accept-transfer-pair:FROM_CANDIDATE_ID:TO_CANDIDATE_ID`

Each candidate id may appear in only one action in the submitted set. Every `action_results[]` item reports `action`, `candidate_ids`, `status` (`validated`, `failed`, or `applied`), `canonical_transaction_ids`, and stable `errors`. If any preflight fails, the top-level error is `batch_preflight_failed`, no action is written, and successful preflights remain labelled `validated`, not `applied`. Dry-run always stops after preflight.

Stable validation/error codes include `json_output_required`, `import_batch_not_found`, `invalid_largest_limit`, `candidate_not_found`, `actions_required`, `candidate_ids_required`, `invalid_batch_action`, `candidate_reused_in_batch`, `candidate_not_obvious_duplicate`, `candidate_already_accepted`, `candidate_already_rejected`, the existing `transfer_pair_*` codes, `batch_preflight_failed`, `database_open_failed`, and `database_operation_failed`.
