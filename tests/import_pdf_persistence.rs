use rusqlite::Connection;
use tracky::pdf::{
    AccountHint, CandidateStatus, CandidateTransaction, CredentialSource, DirectionHint,
    DocumentDuplicateState, DocumentDuplicateStatus, DuplicateStatus, DuplicateStatusState,
    Evidence, ExtractorRef, ExtractorState, ExtractorStatus, ParserRef, ParserState, ParserStatus,
    PdfInspectResponse, Provenance, SourceDocument, TrackyError, PDF_INSPECT_SCHEMA_VERSION,
};
use tracky::storage::{apply_migrations, persist_pdf_import};

fn temporary_database() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let connection = Connection::open(db_path).expect("open temp sqlite db");
    (dir, connection)
}

fn inspect_response(hash: &str) -> PdfInspectResponse {
    let source_document = SourceDocument {
        id: format!("srcdoc_{}", &hash[..26]),
        input_name: "nequi-redacted.pdf".to_string(),
        content_sha256: hash.to_string(),
        mime_type: "application/pdf",
        byte_size: 42,
        institution_hint: "nequi".to_string(),
        account_hint: AccountHint {
            label: "Nequi wallet".to_string(),
            currency: "COP",
            masked_identifier: None,
        },
        document_duplicate_status: DocumentDuplicateStatus {
            status: DocumentDuplicateState::Unknown,
            matched_source_document_id: None,
            reason: None,
        },
    };
    let candidate = CandidateTransaction {
        id: format!("cand_{}_{:04}", &hash[..26], 1),
        import_batch_id: None,
        source_document_id: source_document.id.clone(),
        status: CandidateStatus::PendingReview,
        duplicate_status: DuplicateStatus {
            status: DuplicateStatusState::NotChecked,
            fingerprint: "fp_redacted_001".to_string(),
            matched_candidate_ids: Vec::new(),
            matched_canonical_transaction_ids: Vec::new(),
            reason: None,
        },
        institution_hint: "nequi".to_string(),
        account_hint: source_document.account_hint.clone(),
        posted_date: "2026-05-31".to_string(),
        description: "Redacted merchant".to_string(),
        amount_minor: -4590000,
        currency: "COP",
        balance_minor: Some(12500000),
        direction_hint: DirectionHint::Outflow,
        confidence: 0.91,
        provenance: Provenance {
            source_document_id: source_document.id.clone(),
            page_number: 2,
            row_index: 17,
            bbox: None,
            extractor: ExtractorRef {
                name: "pdf_oxide",
                version: None,
            },
            parser: ParserRef {
                id: "nequi.statement.v1".to_string(),
                version: "1",
            },
            evidence: Evidence {
                redaction: "redacted",
                text: "2026-05-31 REDACTED_COUNTERPARTY <amount>".to_string(),
                raw_storage_policy: "redacted_only",
            },
            confidence: 0.91,
        },
        validation_warnings: Vec::new(),
    };
    PdfInspectResponse {
        schema_version: PDF_INSPECT_SCHEMA_VERSION,
        command: "pdf inspect",
        ok: true,
        source_document,
        extractor_status: ExtractorStatus {
            status: ExtractorState::Succeeded,
            extractor: "pdf_oxide",
            pages_seen: 2,
            pages_extracted: 2,
            requires_document_credential: false,
            credential_source: CredentialSource::None,
            warnings: Vec::new(),
        },
        parser_status: ParserStatus {
            status: ParserState::Succeeded,
            parser_id: "nequi.statement.v1".to_string(),
            parser_version: "1",
            candidates_found: 1,
            candidates_valid: 1,
            warnings: Vec::new(),
        },
        candidates: vec![candidate],
        errors: Vec::<TrackyError>::new(),
    }
}

#[test]
fn successful_import_persists_review_first_records() {
    let (_dir, mut connection) = temporary_database();
    apply_migrations(&connection).expect("apply migrations");

    let response = persist_pdf_import(
        &mut connection,
        inspect_response("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    )
    .expect("persist import");

    assert!(response.ok);
    assert_eq!(response.schema_version, "tracky.import-pdf.v1");
    assert_eq!(response.command, "import pdf");
    assert_eq!(
        response.import_batch.as_ref().unwrap().status,
        tracky::storage::ImportBatchStatus::Completed
    );
    assert_eq!(
        response.source_document.document_duplicate_status.status,
        DocumentDuplicateState::New
    );
    assert_eq!(
        response.candidates[0].status,
        CandidateStatus::PendingReview
    );
    assert_eq!(
        response.candidates[0].duplicate_status.status,
        DuplicateStatusState::NotChecked
    );
    assert!(response.candidates[0].import_batch_id.is_some());

    let counts: (i64, i64, i64, i64, i64, i64) = connection
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM source_documents),
                (SELECT COUNT(*) FROM import_batches),
                (SELECT COUNT(*) FROM candidate_transactions),
                (SELECT COUNT(*) FROM provenance),
                (SELECT COUNT(*) FROM transaction_fingerprints),
                (SELECT COUNT(*) FROM canonical_transactions)",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .expect("read counts");
    assert_eq!(counts, (1, 1, 1, 1, 1, 0));

    let persisted: (String, String, String, String, String) = connection
        .query_row(
            "SELECT c.status, c.duplicate_status, b.status, p.raw_storage_policy, p.evidence_text_redacted
             FROM candidate_transactions c
             JOIN import_batches b ON b.id = c.import_batch_id
             JOIN provenance p ON p.candidate_transaction_id = c.id",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .expect("read persisted statuses");
    assert_eq!(
        persisted,
        (
            "pending_review".to_string(),
            "not_checked".to_string(),
            "completed".to_string(),
            "redacted_only".to_string(),
            "2026-05-31 REDACTED_COUNTERPARTY <amount>".to_string(),
        )
    );
}

#[test]
fn reimporting_same_source_hash_reports_duplicate_without_new_batch_or_candidates() {
    let (_dir, mut connection) = temporary_database();
    apply_migrations(&connection).expect("apply migrations");
    let hash = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    persist_pdf_import(&mut connection, inspect_response(hash)).expect("first import");
    let duplicate =
        persist_pdf_import(&mut connection, inspect_response(hash)).expect("duplicate import");

    assert!(!duplicate.ok);
    assert!(duplicate.import_batch.is_none());
    assert!(duplicate.candidates.is_empty());
    assert_eq!(
        duplicate.source_document.document_duplicate_status.status,
        DocumentDuplicateState::DuplicateSourceDocument
    );
    assert_eq!(
        duplicate.errors[0].code,
        tracky::pdf::TrackyErrorCode::DuplicateSourceDocument
    );
    assert_eq!(
        duplicate.errors[0].details["reason"],
        "source_document_already_imported"
    );

    let counts: (i64, i64, i64, i64) = connection
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM source_documents),
                (SELECT COUNT(*) FROM import_batches),
                (SELECT COUNT(*) FROM candidate_transactions),
                (SELECT COUNT(*) FROM canonical_transactions)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("read counts");
    assert_eq!(counts, (1, 1, 1, 0));
}
