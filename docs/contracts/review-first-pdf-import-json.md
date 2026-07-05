# Review-first PDF import JSON contract

This document defines the stable JSON contract for the future `tracky pdf inspect` and `tracky import pdf` commands. Both commands expose PDF extraction as reviewable evidence: they may produce **transacciones candidatas**, but they must never create **transacciones canónicas** directly.

## Scope

| Command | Writes storage? | Creates candidates? | Creates canonical transactions? |
| --- | --- | --- | --- |
| `tracky pdf inspect` | No | Returns transient candidate-shaped results in JSON | Never |
| `tracky import pdf` | Yes, once implemented | Persists candidate transactions in an import batch | Never |

Out of scope for this contract: parser implementation, SQLite migrations, canonical transaction promotion, TUI review, MCP wrappers, AI fallback, and password storage.

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
| `tracky candidates accept <CANDIDATE_ID> --db <PATH> --json` | Yes | Yes, exactly one canonical transaction for an eligible candidate |
| `tracky candidates reject <CANDIDATE_ID> --db <PATH> --json` | Yes | Never |

All three commands return `schema_version: "tracky.candidate-review.v1"` and machine-readable JSON. `accept` only accepts candidates in `pending_review` or `possible_duplicate` state. It sets the candidate to `accepted`, creates a canonical transaction, links the candidate and provenance to that canonical transaction, and keeps the original candidate/provenance audit trail. `reject` sets the candidate to `rejected` without deleting provenance, fingerprints, or duplicate markers. Re-accepting an accepted candidate returns a stable `candidate_already_accepted` error.
