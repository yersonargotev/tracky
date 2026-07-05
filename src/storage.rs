use crate::pdf::{
    CandidateStatus, CandidateTransaction, DirectionHint, DocumentDuplicateState,
    DocumentDuplicateStatus, DuplicateStatus, DuplicateStatusState, PdfInspectResponse,
    SemanticHint, SourceDocument, TrackyError,
};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

const REVIEW_FIRST_SCHEMA: &str = include_str!("../migrations/0001_review_first_schema.sql");
pub const IMPORT_PDF_SCHEMA_VERSION: &str = "tracky.import-pdf.v1";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CandidateReviewResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate: Option<ReviewCandidate>,
    pub candidates: Vec<ReviewCandidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_transaction: Option<CanonicalTransaction>,
    pub errors: Vec<ReviewError>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReviewCandidate {
    pub id: String,
    pub import_batch_id: String,
    pub source_document_id: String,
    pub status: CandidateStatus,
    pub duplicate_status: ReviewDuplicateStatus,
    pub institution_hint: Option<String>,
    pub account_hint: ReviewAccountHint,
    pub posted_date: String,
    pub description: String,
    pub amount_minor: i64,
    pub currency: String,
    pub balance_minor: Option<i64>,
    pub direction_hint: Option<String>,
    pub semantic_hint: Option<String>,
    pub confidence: f64,
    pub provenance: ReviewProvenance,
    pub validation_warnings: Vec<String>,
    pub canonical_transaction_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReviewAccountHint {
    pub label: Option<String>,
    pub currency: Option<String>,
    pub masked_identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReviewDuplicateStatus {
    pub status: DuplicateStatusState,
    pub fingerprint: Option<String>,
    pub matched_candidate_ids: Vec<String>,
    pub matched_canonical_transaction_ids: Vec<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReviewProvenance {
    pub candidate_transaction_id: Option<String>,
    pub source_document_id: String,
    pub import_batch_id: Option<String>,
    pub page_number: Option<i64>,
    pub row_index: Option<i64>,
    pub evidence_redaction: String,
    pub evidence_text_redacted: String,
    pub raw_storage_policy: String,
    pub extractor_name: String,
    pub extractor_version: Option<String>,
    pub parser_id: String,
    pub parser_version: String,
    pub confidence: f64,
    pub canonical_transaction_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CanonicalTransaction {
    pub id: String,
    pub posted_date: String,
    pub description: String,
    pub amount_minor: i64,
    pub currency: String,
    pub balance_minor: Option<i64>,
    pub created_from_candidate_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReviewError {
    pub category: &'static str,
    pub code: &'static str,
    pub message: String,
    pub path: &'static str,
    pub recoverable: bool,
    pub details: serde_json::Value,
}

pub const CANDIDATE_REVIEW_SCHEMA_VERSION: &str = "tracky.candidate-review.v1";

/// Apply Tracky's SQLite migrations needed for the review-first import store.
pub fn apply_migrations(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(REVIEW_FIRST_SCHEMA)?;
    add_column_if_missing(
        connection,
        "import_batches",
        "duplicate_count",
        "ALTER TABLE import_batches ADD COLUMN duplicate_count INTEGER NOT NULL DEFAULT 0 CHECK (duplicate_count >= 0)",
    )?;
    add_column_if_missing(
        connection,
        "candidate_transactions",
        "semantic_hint",
        "ALTER TABLE candidate_transactions ADD COLUMN semantic_hint TEXT CHECK (semantic_hint IN ('bank_movement', 'card_charge', 'card_payment'))",
    )
}

fn add_column_if_missing(
    connection: &Connection,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> rusqlite::Result<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for existing in columns {
        if existing? == column {
            return Ok(());
        }
    }
    connection.execute(alter_sql, [])?;
    Ok(())
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
    pub duplicate_count: usize,
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
        duplicate_count: 0,
    };
    let mut candidates = inspect.candidates;
    mark_candidate_duplicates(connection, &mut candidates)?;
    let duplicate_count = candidates
        .iter()
        .filter(|candidate| is_duplicate_status(&candidate.duplicate_status.status))
        .count();
    for candidate in &mut candidates {
        candidate.import_batch_id = Some(batch.id.clone());
        candidate.source_document_id = source_document.id.clone();
        candidate.provenance.source_document_id = source_document.id.clone();
    }
    let batch = ImportBatch {
        duplicate_count,
        ..batch
    };

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
    for candidate in &candidates {
        insert_duplicate_markers(&tx, candidate)?;
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
            candidate_count, error_count, duplicate_count, error_details_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            batch.id,
            batch.source_document_id,
            batch.started_at,
            batch.completed_at,
            import_batch_status_value(&batch.status),
            batch.candidate_count as i64,
            batch.error_count as i64,
            batch.duplicate_count as i64,
            serde_json::to_string(errors).context("serializing import errors")?,
        ],
    )?;
    Ok(())
}

fn mark_candidate_duplicates(
    connection: &Connection,
    candidates: &mut [CandidateTransaction],
) -> Result<()> {
    let exact_matches = in_batch_exact_matches(candidates);
    let near_matches = in_batch_near_matches(candidates);
    for candidate in candidates {
        let duplicate_status =
            duplicate_status_for_candidate(connection, candidate, &exact_matches, &near_matches)?;
        if should_mark_candidate_possible_duplicate(candidate, &duplicate_status.status) {
            candidate.status = CandidateStatus::PossibleDuplicate;
        }
        candidate.duplicate_status = duplicate_status;
    }
    Ok(())
}

fn in_batch_exact_matches(
    candidates: &[CandidateTransaction],
) -> std::collections::HashMap<String, Vec<String>> {
    let mut by_fingerprint: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for candidate in candidates {
        by_fingerprint
            .entry(candidate.duplicate_status.fingerprint.clone())
            .or_default()
            .push(candidate.id.clone());
    }
    by_fingerprint
}

fn in_batch_near_matches(
    candidates: &[CandidateTransaction],
) -> std::collections::HashMap<DuplicateNearKey, Vec<String>> {
    let mut by_near_key: std::collections::HashMap<DuplicateNearKey, Vec<String>> =
        std::collections::HashMap::new();
    for candidate in candidates {
        by_near_key
            .entry(DuplicateNearKey::from(candidate))
            .or_default()
            .push(candidate.id.clone());
    }
    by_near_key
}

fn duplicate_status_for_candidate(
    connection: &Connection,
    candidate: &CandidateTransaction,
    exact_matches: &std::collections::HashMap<String, Vec<String>>,
    near_matches: &std::collections::HashMap<DuplicateNearKey, Vec<String>>,
) -> Result<DuplicateStatus> {
    let mut matched_candidate_ids = Vec::new();
    let mut matched_canonical_transaction_ids = Vec::new();
    if let Some(candidate_ids) = exact_matches.get(&candidate.duplicate_status.fingerprint) {
        matched_candidate_ids.extend(
            candidate_ids
                .iter()
                .filter(|candidate_id| *candidate_id != &candidate.id)
                .cloned(),
        );
    }
    let existing_matches =
        existing_fingerprint_matches(connection, &candidate.duplicate_status.fingerprint)?;
    matched_candidate_ids.extend(existing_matches.matched_candidate_ids);
    matched_canonical_transaction_ids.extend(existing_matches.matched_canonical_transaction_ids);

    let mut status =
        if matched_candidate_ids.is_empty() && matched_canonical_transaction_ids.is_empty() {
            DuplicateStatusState::Unique
        } else {
            DuplicateStatusState::ExactDuplicate
        };

    if status == DuplicateStatusState::Unique {
        if let Some(candidate_ids) = near_matches.get(&DuplicateNearKey::from(candidate)) {
            matched_candidate_ids.extend(
                candidate_ids
                    .iter()
                    .filter(|candidate_id| *candidate_id != &candidate.id)
                    .cloned(),
            );
        }
        let near_existing_matches = existing_near_matches(connection, candidate)?;
        matched_candidate_ids.extend(near_existing_matches.matched_candidate_ids);
        matched_canonical_transaction_ids
            .extend(near_existing_matches.matched_canonical_transaction_ids);
        if !matched_candidate_ids.is_empty() || !matched_canonical_transaction_ids.is_empty() {
            status = DuplicateStatusState::PossibleDuplicate;
        }
    }

    let reason = match status {
        DuplicateStatusState::ExactDuplicate => {
            Some("normalized_transaction_fingerprint_matched".to_string())
        }
        DuplicateStatusState::PossibleDuplicate => {
            Some("normalized_transaction_fields_matched".to_string())
        }
        DuplicateStatusState::NotChecked | DuplicateStatusState::Unique => None,
    };

    Ok(DuplicateStatus {
        status,
        fingerprint: candidate.duplicate_status.fingerprint.clone(),
        matched_candidate_ids,
        matched_canonical_transaction_ids,
        reason,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DuplicateNearKey {
    account_key: String,
    posted_date: String,
    amount_minor: i64,
    currency: String,
}

impl From<&CandidateTransaction> for DuplicateNearKey {
    fn from(candidate: &CandidateTransaction) -> Self {
        Self {
            account_key: duplicate_account_key(candidate),
            posted_date: candidate.posted_date.clone(),
            amount_minor: candidate.amount_minor,
            currency: normalized_currency(candidate.currency),
        }
    }
}

struct ExistingFingerprintMatches {
    matched_candidate_ids: Vec<String>,
    matched_canonical_transaction_ids: Vec<String>,
}

fn existing_fingerprint_matches(
    connection: &Connection,
    fingerprint: &str,
) -> Result<ExistingFingerprintMatches> {
    let mut statement = connection.prepare(
        "SELECT candidate_transaction_id, canonical_transaction_id
         FROM transaction_fingerprints
         WHERE fingerprint = ?1",
    )?;
    let rows = statement.query_map(params![fingerprint], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, Option<String>>(1)?,
        ))
    })?;
    let mut matched_candidate_ids = Vec::new();
    let mut matched_canonical_transaction_ids = Vec::new();
    for row in rows {
        let (candidate_id, canonical_id) = row?;
        if let Some(candidate_id) = candidate_id {
            matched_candidate_ids.push(candidate_id);
        }
        if let Some(canonical_id) = canonical_id {
            matched_canonical_transaction_ids.push(canonical_id);
        }
    }
    Ok(ExistingFingerprintMatches {
        matched_candidate_ids,
        matched_canonical_transaction_ids,
    })
}

fn existing_near_matches(
    connection: &Connection,
    candidate: &CandidateTransaction,
) -> Result<ExistingFingerprintMatches> {
    let mut statement = connection.prepare(
        "SELECT tf.candidate_transaction_id, tf.canonical_transaction_id
         FROM transaction_fingerprints tf
         LEFT JOIN candidate_transactions c ON c.id = tf.candidate_transaction_id
         LEFT JOIN source_documents sd ON sd.id = c.source_document_id
         LEFT JOIN canonical_transactions ct ON ct.id = tf.canonical_transaction_id
         LEFT JOIN accounts candidate_account ON candidate_account.id = c.account_id
         LEFT JOIN institutions candidate_account_institution ON candidate_account_institution.id = candidate_account.institution_id
         LEFT JOIN accounts canonical_account ON canonical_account.id = ct.account_id
         LEFT JOIN institutions canonical_account_institution ON canonical_account_institution.id = canonical_account.institution_id
         WHERE (
             tf.normalized_account_key = ?1
             OR (
                 tf.normalized_account_key = ?2
                 AND (
                     LOWER(c.institution_hint) = ?3
                     OR LOWER(sd.institution_hint) = ?3
                     OR LOWER(candidate_account_institution.name) = ?3
                     OR LOWER(canonical_account_institution.name) = ?3
                 )
             )
         )
           AND tf.normalized_posted_date = ?4
           AND tf.normalized_amount_minor = ?5
           AND UPPER(tf.normalized_currency) = ?6
           AND tf.fingerprint != ?7",
    )?;
    let rows = statement.query_map(
        params![
            duplicate_account_key(candidate),
            legacy_duplicate_account_key(candidate),
            normalized_institution_key(&candidate.institution_hint),
            candidate.posted_date,
            candidate.amount_minor,
            normalized_currency(candidate.currency),
            candidate.duplicate_status.fingerprint,
        ],
        |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        },
    )?;
    let mut matched_candidate_ids = Vec::new();
    let mut matched_canonical_transaction_ids = Vec::new();
    for row in rows {
        let (candidate_id, canonical_id) = row?;
        if let Some(candidate_id) = candidate_id {
            matched_candidate_ids.push(candidate_id);
        }
        if let Some(canonical_id) = canonical_id {
            matched_canonical_transaction_ids.push(canonical_id);
        }
    }
    Ok(ExistingFingerprintMatches {
        matched_candidate_ids,
        matched_canonical_transaction_ids,
    })
}

fn is_duplicate_status(status: &DuplicateStatusState) -> bool {
    matches!(
        status,
        DuplicateStatusState::PossibleDuplicate | DuplicateStatusState::ExactDuplicate
    )
}

fn should_mark_candidate_possible_duplicate(
    candidate: &CandidateTransaction,
    duplicate_status: &DuplicateStatusState,
) -> bool {
    is_duplicate_status(duplicate_status)
        && !matches!(
            candidate.status,
            CandidateStatus::Accepted | CandidateStatus::Rejected
        )
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
            direction_hint, semantic_hint, confidence, status, duplicate_status, fingerprint,
            validation_warnings_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
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
            semantic_hint_value(&candidate.semantic_hint),
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

fn insert_duplicate_markers(
    connection: &Connection,
    candidate: &CandidateTransaction,
) -> Result<()> {
    for matched_candidate_id in &candidate.duplicate_status.matched_candidate_ids {
        connection.execute(
            "INSERT INTO transaction_duplicate_markers (
                id, candidate_transaction_id, matched_candidate_transaction_id,
                duplicate_status, reason
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                duplicate_marker_id(&candidate.id, matched_candidate_id),
                candidate.id,
                matched_candidate_id,
                duplicate_status_value(&candidate.duplicate_status.status),
                candidate.duplicate_status.reason,
            ],
        )?;
    }
    for matched_canonical_id in &candidate.duplicate_status.matched_canonical_transaction_ids {
        connection.execute(
            "INSERT INTO transaction_duplicate_markers (
                id, candidate_transaction_id, matched_canonical_transaction_id,
                duplicate_status, reason
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                duplicate_marker_id(&candidate.id, matched_canonical_id),
                candidate.id,
                matched_canonical_id,
                duplicate_status_value(&candidate.duplicate_status.status),
                candidate.duplicate_status.reason,
            ],
        )?;
    }
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
            duplicate_account_key(candidate),
            candidate.posted_date,
            candidate.amount_minor,
            normalized_currency(candidate.currency),
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

fn duplicate_marker_id(candidate_id: &str, matched_id: &str) -> String {
    let digest = Sha256::digest(format!("{candidate_id}|{matched_id}").as_bytes());
    format!(
        "dup_{}",
        digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn duplicate_account_key(candidate: &CandidateTransaction) -> String {
    format!(
        "{}|{}",
        candidate.institution_hint.to_ascii_lowercase(),
        candidate.account_hint.label.to_ascii_lowercase()
    )
}

fn legacy_duplicate_account_key(candidate: &CandidateTransaction) -> String {
    candidate.account_hint.label.to_ascii_lowercase()
}

fn normalized_institution_key(institution: &str) -> String {
    institution.to_ascii_lowercase()
}

fn normalized_currency(currency: &str) -> String {
    currency.to_ascii_uppercase()
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
        CandidateStatus::PossibleDuplicate => "possible_duplicate",
        CandidateStatus::Accepted => "accepted",
        CandidateStatus::Rejected => "rejected",
    }
}

fn duplicate_status_value(status: &DuplicateStatusState) -> &'static str {
    match status {
        DuplicateStatusState::NotChecked => "not_checked",
        DuplicateStatusState::Unique => "unique",
        DuplicateStatusState::PossibleDuplicate => "possible_duplicate",
        DuplicateStatusState::ExactDuplicate => "exact_duplicate",
    }
}

fn direction_hint_value(direction_hint: &DirectionHint) -> &'static str {
    match direction_hint {
        DirectionHint::Inflow => "inflow",
        DirectionHint::Outflow => "outflow",
    }
}

fn semantic_hint_value(semantic_hint: &SemanticHint) -> &'static str {
    match semantic_hint {
        SemanticHint::BankMovement => "bank_movement",
        SemanticHint::CardCharge => "card_charge",
        SemanticHint::CardPayment => "card_payment",
    }
}

pub fn list_review_candidates(
    connection: &Connection,
    import_batch_id: Option<&str>,
    status: Option<&str>,
) -> Result<Vec<ReviewCandidate>> {
    if let Some(status) = status {
        validate_candidate_status_filter(status)?;
    }
    let mut sql = review_candidate_select_sql();
    let mut filters = Vec::new();
    if import_batch_id.is_some() {
        filters.push("c.import_batch_id = ?");
    }
    if status.is_some() {
        filters.push("c.status = ?");
    }
    if !filters.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&filters.join(" AND "));
    }
    sql.push_str(" ORDER BY c.import_batch_id, p.row_index, c.id");

    let mut statement = connection.prepare(&sql)?;
    let rows = match (import_batch_id, status) {
        (Some(import_batch_id), Some(status)) => {
            statement.query_map(params![import_batch_id, status], review_candidate_from_row)?
        }
        (Some(import_batch_id), None) => {
            statement.query_map(params![import_batch_id], review_candidate_from_row)?
        }
        (None, Some(status)) => statement.query_map(params![status], review_candidate_from_row)?,
        (None, None) => statement.query_map([], review_candidate_from_row)?,
    };
    let mut candidates = Vec::new();
    for row in rows {
        let mut candidate = row?;
        hydrate_duplicate_matches(connection, &mut candidate)?;
        candidates.push(candidate);
    }
    Ok(candidates)
}

pub fn accept_candidate(
    connection: &mut Connection,
    candidate_id: &str,
) -> Result<CandidateReviewResponse> {
    let tx = connection
        .transaction()
        .context("starting candidate accept transaction")?;
    let Some(candidate) = find_review_candidate(&tx, candidate_id)? else {
        return Ok(review_error_response(
            "candidates accept",
            "not_found",
            "candidate_not_found",
            "Candidate transaction was not found.".to_string(),
            "candidate_id",
            true,
            serde_json::json!({ "candidate_id": candidate_id }),
        ));
    };
    if candidate.status == CandidateStatus::Accepted || candidate.canonical_transaction_id.is_some()
    {
        return Ok(review_error_response(
            "candidates accept",
            "conflict",
            "candidate_already_accepted",
            "Candidate transaction was already accepted.".to_string(),
            "candidate.status",
            true,
            serde_json::json!({
                "candidate_id": candidate_id,
                "canonical_transaction_id": candidate.canonical_transaction_id,
            }),
        ));
    }
    if !matches!(
        candidate.status,
        CandidateStatus::PendingReview | CandidateStatus::PossibleDuplicate
    ) {
        return Ok(review_error_response(
            "candidates accept",
            "conflict",
            "candidate_not_acceptable",
            "Only pending_review or possible_duplicate candidates can be accepted.".to_string(),
            "candidate.status",
            true,
            serde_json::json!({ "candidate_id": candidate_id, "status": candidate.status }),
        ));
    }

    let canonical_id = canonical_transaction_id(candidate_id);
    tx.execute(
        "INSERT INTO canonical_transactions (
            id, account_id, posted_date, description, amount_minor, currency,
            balance_minor, created_from_candidate_id
         )
         SELECT ?1, account_id, posted_date, description, amount_minor, currency,
                balance_minor, id
         FROM candidate_transactions
         WHERE id = ?2",
        params![canonical_id, candidate_id],
    )?;
    tx.execute(
        "UPDATE candidate_transactions
         SET status = 'accepted', canonical_transaction_id = ?1
         WHERE id = ?2",
        params![canonical_id, candidate_id],
    )?;
    tx.execute(
        "UPDATE provenance
         SET canonical_transaction_id = ?1
         WHERE candidate_transaction_id = ?2",
        params![canonical_id, candidate_id],
    )?;
    tx.execute(
        "UPDATE transaction_fingerprints
         SET candidate_transaction_id = NULL,
             canonical_transaction_id = ?1
         WHERE candidate_transaction_id = ?2",
        params![canonical_id, candidate_id],
    )?;
    tx.commit().context("committing candidate accept")?;

    let candidate = find_review_candidate(connection, candidate_id)?
        .expect("accepted candidate remains queryable");
    let canonical_transaction = canonical_transaction(connection, &canonical_id)?
        .expect("accepted canonical transaction remains queryable");
    Ok(CandidateReviewResponse {
        schema_version: CANDIDATE_REVIEW_SCHEMA_VERSION,
        command: "candidates accept",
        ok: true,
        candidate: Some(candidate),
        candidates: Vec::new(),
        canonical_transaction: Some(canonical_transaction),
        errors: Vec::new(),
    })
}

pub fn reject_candidate(
    connection: &mut Connection,
    candidate_id: &str,
) -> Result<CandidateReviewResponse> {
    let tx = connection
        .transaction()
        .context("starting candidate reject transaction")?;
    let Some(candidate) = find_review_candidate(&tx, candidate_id)? else {
        return Ok(review_error_response(
            "candidates reject",
            "not_found",
            "candidate_not_found",
            "Candidate transaction was not found.".to_string(),
            "candidate_id",
            true,
            serde_json::json!({ "candidate_id": candidate_id }),
        ));
    };
    if candidate.status == CandidateStatus::Accepted {
        return Ok(review_error_response(
            "candidates reject",
            "conflict",
            "candidate_already_accepted",
            "Accepted candidates cannot be rejected without a future reversal command.".to_string(),
            "candidate.status",
            true,
            serde_json::json!({
                "candidate_id": candidate_id,
                "canonical_transaction_id": candidate.canonical_transaction_id,
            }),
        ));
    }
    if candidate.status == CandidateStatus::Rejected {
        return Ok(review_error_response(
            "candidates reject",
            "conflict",
            "candidate_already_rejected",
            "Candidate transaction was already rejected.".to_string(),
            "candidate.status",
            true,
            serde_json::json!({ "candidate_id": candidate_id }),
        ));
    }

    tx.execute(
        "UPDATE candidate_transactions SET status = 'rejected' WHERE id = ?1",
        params![candidate_id],
    )?;
    tx.commit().context("committing candidate reject")?;
    let candidate = find_review_candidate(connection, candidate_id)?
        .expect("rejected candidate remains queryable");
    Ok(CandidateReviewResponse {
        schema_version: CANDIDATE_REVIEW_SCHEMA_VERSION,
        command: "candidates reject",
        ok: true,
        candidate: Some(candidate),
        candidates: Vec::new(),
        canonical_transaction: None,
        errors: Vec::new(),
    })
}

pub fn review_error_response(
    command: &'static str,
    category: &'static str,
    code: &'static str,
    message: String,
    path: &'static str,
    recoverable: bool,
    details: serde_json::Value,
) -> CandidateReviewResponse {
    CandidateReviewResponse {
        schema_version: CANDIDATE_REVIEW_SCHEMA_VERSION,
        command,
        ok: false,
        candidate: None,
        candidates: Vec::new(),
        canonical_transaction: None,
        errors: vec![ReviewError {
            category,
            code,
            message,
            path,
            recoverable,
            details,
        }],
    }
}

fn validate_candidate_status_filter(status: &str) -> Result<()> {
    match status {
        "pending_review" | "possible_duplicate" | "accepted" | "rejected" => Ok(()),
        other => anyhow::bail!("unsupported candidate status filter: {other}"),
    }
}

fn parse_review_candidate_status(status: String) -> rusqlite::Result<CandidateStatus> {
    match status.as_str() {
        "pending_review" => Ok(CandidateStatus::PendingReview),
        "possible_duplicate" => Ok(CandidateStatus::PossibleDuplicate),
        "accepted" => Ok(CandidateStatus::Accepted),
        "rejected" => Ok(CandidateStatus::Rejected),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unsupported candidate status: {other}").into(),
        )),
    }
}

fn parse_review_duplicate_status(status: String) -> rusqlite::Result<DuplicateStatusState> {
    match status.as_str() {
        "not_checked" => Ok(DuplicateStatusState::NotChecked),
        "unique" => Ok(DuplicateStatusState::Unique),
        "possible_duplicate" => Ok(DuplicateStatusState::PossibleDuplicate),
        "exact_duplicate" => Ok(DuplicateStatusState::ExactDuplicate),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unsupported duplicate status: {other}").into(),
        )),
    }
}

fn find_review_candidate(
    connection: &Connection,
    candidate_id: &str,
) -> Result<Option<ReviewCandidate>> {
    let sql = format!("{} WHERE c.id = ?1", review_candidate_select_sql());
    let mut candidate = connection
        .query_row(&sql, params![candidate_id], review_candidate_from_row)
        .optional()?;
    if let Some(candidate) = &mut candidate {
        hydrate_duplicate_matches(connection, candidate)?;
    }
    Ok(candidate)
}

fn review_candidate_select_sql() -> String {
    "SELECT
        c.id, c.import_batch_id, c.source_document_id, c.status,
        c.duplicate_status, c.fingerprint, c.institution_hint,
        c.account_label_hint, c.account_currency_hint, c.account_masked_identifier_hint,
        c.posted_date, c.description, c.amount_minor, c.currency, c.balance_minor,
        c.direction_hint, c.semantic_hint, c.confidence, c.validation_warnings_json, c.canonical_transaction_id,
        p.candidate_transaction_id, p.source_document_id, p.import_batch_id, p.page_number, p.row_index,
        p.evidence_redaction, p.evidence_text_redacted, p.raw_storage_policy,
        p.extractor_name, p.extractor_version, p.parser_id, p.parser_version,
        p.confidence, p.canonical_transaction_id
     FROM candidate_transactions c
     JOIN provenance p ON p.candidate_transaction_id = c.id"
        .to_string()
}

fn review_candidate_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewCandidate> {
    let validation_warnings_json: String = row.get(18)?;
    let validation_warnings = serde_json::from_str(&validation_warnings_json).unwrap_or_default();
    Ok(ReviewCandidate {
        id: row.get(0)?,
        import_batch_id: row.get(1)?,
        source_document_id: row.get(2)?,
        status: parse_review_candidate_status(row.get(3)?)?,
        duplicate_status: ReviewDuplicateStatus {
            status: parse_review_duplicate_status(row.get(4)?)?,
            fingerprint: row.get(5)?,
            matched_candidate_ids: Vec::new(),
            matched_canonical_transaction_ids: Vec::new(),
            reason: None,
        },
        institution_hint: row.get(6)?,
        account_hint: ReviewAccountHint {
            label: row.get(7)?,
            currency: row.get(8)?,
            masked_identifier: row.get(9)?,
        },
        posted_date: row.get(10)?,
        description: row.get(11)?,
        amount_minor: row.get(12)?,
        currency: row.get(13)?,
        balance_minor: row.get(14)?,
        direction_hint: row.get(15)?,
        semantic_hint: row.get(16)?,
        confidence: row.get(17)?,
        provenance: ReviewProvenance {
            candidate_transaction_id: row.get(20)?,
            source_document_id: row.get(21)?,
            import_batch_id: row.get(22)?,
            page_number: row.get(23)?,
            row_index: row.get(24)?,
            evidence_redaction: row.get(25)?,
            evidence_text_redacted: row.get(26)?,
            raw_storage_policy: row.get(27)?,
            extractor_name: row.get(28)?,
            extractor_version: row.get(29)?,
            parser_id: row.get(30)?,
            parser_version: row.get(31)?,
            confidence: row.get(32)?,
            canonical_transaction_id: row.get(33)?,
        },
        validation_warnings,
        canonical_transaction_id: row.get(19)?,
    })
}

fn hydrate_duplicate_matches(
    connection: &Connection,
    candidate: &mut ReviewCandidate,
) -> Result<()> {
    let mut statement = connection.prepare(
        "SELECT matched_candidate_transaction_id, matched_canonical_transaction_id, reason
         FROM transaction_duplicate_markers
         WHERE candidate_transaction_id = ?1
         ORDER BY id",
    )?;
    let rows = statement.query_map(params![candidate.id], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    for row in rows {
        let (matched_candidate_id, matched_canonical_id, reason) = row?;
        if let Some(matched_candidate_id) = matched_candidate_id {
            candidate
                .duplicate_status
                .matched_candidate_ids
                .push(matched_candidate_id);
        }
        if let Some(matched_canonical_id) = matched_canonical_id {
            candidate
                .duplicate_status
                .matched_canonical_transaction_ids
                .push(matched_canonical_id);
        }
        if candidate.duplicate_status.reason.is_none() {
            candidate.duplicate_status.reason = reason;
        }
    }
    Ok(())
}

fn canonical_transaction(
    connection: &Connection,
    canonical_id: &str,
) -> Result<Option<CanonicalTransaction>> {
    connection
        .query_row(
            "SELECT id, posted_date, description, amount_minor, currency, balance_minor,
                    created_from_candidate_id
             FROM canonical_transactions
             WHERE id = ?1",
            params![canonical_id],
            |row| {
                Ok(CanonicalTransaction {
                    id: row.get(0)?,
                    posted_date: row.get(1)?,
                    description: row.get(2)?,
                    amount_minor: row.get(3)?,
                    currency: row.get(4)?,
                    balance_minor: row.get(5)?,
                    created_from_candidate_id: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

fn canonical_transaction_id(candidate_id: &str) -> String {
    let digest = Sha256::digest(candidate_id.as_bytes());
    format!(
        "txn_{}",
        digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}
