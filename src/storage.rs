use crate::pdf::{
    CandidateStatus, CandidateTransaction, DirectionHint, DocumentDuplicateState,
    DocumentDuplicateStatus, DuplicateStatusState, PdfInspectResponse, SourceDocument, TrackyError,
};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

const REVIEW_FIRST_SCHEMA: &str = include_str!("../migrations/0001_review_first_schema.sql");
pub const IMPORT_PDF_SCHEMA_VERSION: &str = "tracky.import-pdf.v1";

/// Apply Tracky's SQLite migrations needed for the review-first import store.
pub fn apply_migrations(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(REVIEW_FIRST_SCHEMA)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ImportBatch {
    pub id: String,
    pub source_document_id: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub status: ImportBatchStatus,
    pub candidate_count: usize,
    pub error_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImportBatchStatus {
    Completed,
    CompletedWithErrors,
    Failed,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ImportPdfResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_batch: Option<ImportBatch>,
    pub source_document: SourceDocument,
    pub extractor_status: crate::pdf::ExtractorStatus,
    pub parser_status: crate::pdf::ParserStatus,
    pub candidates: Vec<CandidateTransaction>,
    pub errors: Vec<TrackyError>,
}

pub fn find_source_document_by_hash(
    connection: &Connection,
    content_sha256: &str,
) -> rusqlite::Result<Option<String>> {
    connection
        .query_row(
            "SELECT id FROM source_documents WHERE content_sha256 = ?1",
            params![content_sha256],
            |row| row.get(0),
        )
        .optional()
}

pub fn persist_pdf_import(
    connection: &mut Connection,
    inspect: PdfInspectResponse,
) -> Result<ImportPdfResponse> {
    if let Some(existing_id) =
        find_source_document_by_hash(connection, &inspect.source_document.content_sha256)?
    {
        let source_document = duplicate_source_document(inspect.source_document, existing_id);
        return Ok(ImportPdfResponse {
            schema_version: IMPORT_PDF_SCHEMA_VERSION,
            command: "import pdf",
            ok: false,
            import_batch: None,
            source_document,
            extractor_status: inspect.extractor_status,
            parser_status: inspect.parser_status,
            candidates: Vec::new(),
            errors: vec![duplicate_source_document_error()],
        });
    }

    let started_at = sqlite_now(connection)?;
    let completed_at = sqlite_now(connection)?;
    let status = if inspect.errors.is_empty() {
        ImportBatchStatus::Completed
    } else if inspect.candidates.is_empty() {
        ImportBatchStatus::Failed
    } else {
        ImportBatchStatus::CompletedWithErrors
    };
    let mut source_document = inspect.source_document;
    source_document.document_duplicate_status = DocumentDuplicateStatus {
        status: DocumentDuplicateState::New,
        matched_source_document_id: None,
        reason: None,
    };
    let batch = ImportBatch {
        id: import_batch_id(&source_document.content_sha256),
        source_document_id: source_document.id.clone(),
        started_at,
        completed_at: Some(completed_at),
        status,
        candidate_count: inspect.candidates.len(),
        error_count: inspect.errors.len(),
    };
    let mut candidates = inspect.candidates;
    for candidate in &mut candidates {
        candidate.import_batch_id = Some(batch.id.clone());
        candidate.source_document_id = source_document.id.clone();
        candidate.provenance.source_document_id = source_document.id.clone();
    }

    let tx = connection
        .transaction()
        .context("starting import transaction")?;
    insert_source_document(&tx, &source_document)?;
    insert_import_batch(&tx, &batch, &inspect.errors)?;
    for candidate in &candidates {
        insert_candidate(&tx, candidate, &source_document, &batch.id)?;
        insert_provenance(&tx, candidate, &source_document, &batch.id)?;
        insert_fingerprint(&tx, candidate)?;
    }
    tx.commit().context("committing pdf import")?;

    Ok(ImportPdfResponse {
        schema_version: IMPORT_PDF_SCHEMA_VERSION,
        command: "import pdf",
        ok: inspect.errors.is_empty(),
        import_batch: Some(batch),
        source_document,
        extractor_status: inspect.extractor_status,
        parser_status: inspect.parser_status,
        candidates,
        errors: inspect.errors,
    })
}

pub fn duplicate_import_response(
    source_document: SourceDocument,
    existing_source_document_id: String,
    extractor_status: crate::pdf::ExtractorStatus,
    parser_status: crate::pdf::ParserStatus,
) -> ImportPdfResponse {
    ImportPdfResponse {
        schema_version: IMPORT_PDF_SCHEMA_VERSION,
        command: "import pdf",
        ok: false,
        import_batch: None,
        source_document: duplicate_source_document(source_document, existing_source_document_id),
        extractor_status,
        parser_status,
        candidates: Vec::new(),
        errors: vec![duplicate_source_document_error()],
    }
}

fn duplicate_source_document(
    mut source_document: SourceDocument,
    existing_id: String,
) -> SourceDocument {
    source_document.document_duplicate_status = DocumentDuplicateStatus {
        status: DocumentDuplicateState::DuplicateSourceDocument,
        matched_source_document_id: Some(existing_id),
        reason: Some("source_document_already_imported".to_string()),
    };
    source_document
}

pub fn duplicate_source_document_error() -> TrackyError {
    TrackyError {
        category: crate::pdf::TrackyErrorCategory::ValidationFailure,
        code: crate::pdf::TrackyErrorCode::DuplicateSourceDocument,
        message: "Source document already imported.".to_string(),
        path: crate::pdf::TrackyErrorPath::SourceDocumentDuplicateStatus,
        recoverable: true,
        details: serde_json::json!({ "reason": "source_document_already_imported" }),
    }
}

fn sqlite_now(connection: &Connection) -> rusqlite::Result<String> {
    connection.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
        row.get(0)
    })
}

fn insert_source_document(connection: &Connection, source: &SourceDocument) -> Result<()> {
    connection.execute(
        "INSERT INTO source_documents (
            id, input_name, content_sha256, mime_type, byte_size,
            institution_hint, account_label_hint, account_currency_hint,
            account_masked_identifier_hint
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            source.id,
            source.input_name,
            source.content_sha256,
            source.mime_type,
            source.byte_size as i64,
            source.institution_hint,
            source.account_hint.label,
            source.account_hint.currency,
            source.account_hint.masked_identifier,
        ],
    )?;
    Ok(())
}

fn insert_import_batch(
    connection: &Connection,
    batch: &ImportBatch,
    errors: &[TrackyError],
) -> Result<()> {
    connection.execute(
        "INSERT INTO import_batches (
            id, source_document_id, started_at, completed_at, status,
            candidate_count, error_count, error_details_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            batch.id,
            batch.source_document_id,
            batch.started_at,
            batch.completed_at,
            import_batch_status_value(&batch.status),
            batch.candidate_count as i64,
            batch.error_count as i64,
            serde_json::to_string(errors).context("serializing import errors")?,
        ],
    )?;
    Ok(())
}

fn insert_candidate(
    connection: &Connection,
    candidate: &CandidateTransaction,
    source: &SourceDocument,
    batch_id: &str,
) -> Result<()> {
    connection.execute(
        "INSERT INTO candidate_transactions (
            id, import_batch_id, source_document_id, institution_hint,
            account_label_hint, account_currency_hint, account_masked_identifier_hint,
            posted_date, description, amount_minor, currency, balance_minor,
            direction_hint, confidence, status, duplicate_status, fingerprint,
            validation_warnings_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            candidate.id,
            batch_id,
            source.id,
            candidate.institution_hint,
            candidate.account_hint.label,
            candidate.account_hint.currency,
            candidate.account_hint.masked_identifier,
            candidate.posted_date,
            candidate.description,
            candidate.amount_minor,
            candidate.currency,
            candidate.balance_minor,
            direction_hint_value(&candidate.direction_hint),
            candidate.confidence as f64,
            candidate_status_value(&candidate.status),
            duplicate_status_value(&candidate.duplicate_status.status),
            candidate.duplicate_status.fingerprint,
            serde_json::to_string(&candidate.validation_warnings)
                .context("serializing validation warnings")?,
        ],
    )?;
    Ok(())
}

fn insert_provenance(
    connection: &Connection,
    candidate: &CandidateTransaction,
    source: &SourceDocument,
    batch_id: &str,
) -> Result<()> {
    let bbox = candidate.provenance.bbox;
    connection.execute(
        "INSERT INTO provenance (
            id, candidate_transaction_id, source_document_id, import_batch_id,
            page_number, row_index, bbox_x, bbox_y, bbox_width, bbox_height, bbox_unit,
            extractor_name, extractor_version, parser_id, parser_version,
            evidence_redaction, evidence_text_redacted, raw_storage_policy, confidence
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
        params![
            provenance_id(&candidate.id),
            candidate.id,
            source.id,
            batch_id,
            candidate.provenance.page_number as i64,
            candidate.provenance.row_index as i64,
            bbox.map(|value| value.x as f64),
            bbox.map(|value| value.y as f64),
            bbox.map(|value| value.width as f64),
            bbox.map(|value| value.height as f64),
            bbox.map(|value| value.unit),
            candidate.provenance.extractor.name,
            candidate.provenance.extractor.version,
            candidate.provenance.parser.id,
            candidate.provenance.parser.version,
            candidate.provenance.evidence.redaction,
            candidate.provenance.evidence.text,
            candidate.provenance.evidence.raw_storage_policy,
            candidate.provenance.confidence as f64,
        ],
    )?;
    Ok(())
}

fn insert_fingerprint(connection: &Connection, candidate: &CandidateTransaction) -> Result<()> {
    connection.execute(
        "INSERT OR IGNORE INTO transaction_fingerprints (
            id, fingerprint, candidate_transaction_id, duplicate_status,
            normalized_account_key, normalized_posted_date, normalized_amount_minor,
            normalized_currency, normalized_description
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            fingerprint_row_id(&candidate.id),
            candidate.duplicate_status.fingerprint,
            candidate.id,
            duplicate_status_value(&candidate.duplicate_status.status),
            candidate.account_hint.label.to_ascii_lowercase(),
            candidate.posted_date,
            candidate.amount_minor,
            candidate.currency,
            candidate.description.to_ascii_lowercase(),
        ],
    )?;
    Ok(())
}

fn import_batch_id(content_sha256: &str) -> String {
    format!("batch_{}", &content_sha256[..26.min(content_sha256.len())])
}

fn provenance_id(candidate_id: &str) -> String {
    format!("prov_{}", candidate_id.trim_start_matches("cand_"))
}

fn fingerprint_row_id(candidate_id: &str) -> String {
    let digest = Sha256::digest(candidate_id.as_bytes());
    format!(
        "fp_{}",
        digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn import_batch_status_value(status: &ImportBatchStatus) -> &'static str {
    match status {
        ImportBatchStatus::Completed => "completed",
        ImportBatchStatus::CompletedWithErrors => "completed_with_errors",
        ImportBatchStatus::Failed => "failed",
    }
}

fn candidate_status_value(status: &CandidateStatus) -> &'static str {
    match status {
        CandidateStatus::PendingReview => "pending_review",
    }
}

fn duplicate_status_value(status: &DuplicateStatusState) -> &'static str {
    match status {
        DuplicateStatusState::NotChecked => "not_checked",
    }
}

fn direction_hint_value(direction_hint: &DirectionHint) -> &'static str {
    match direction_hint {
        DirectionHint::Inflow => "inflow",
        DirectionHint::Outflow => "outflow",
    }
}
