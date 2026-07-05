use rusqlite::Connection;
use tracky::pdf::{
    AccountHint, CandidateStatus, CandidateTransaction, CredentialSource, DirectionHint,
    DocumentDuplicateState, DocumentDuplicateStatus, DuplicateStatus, DuplicateStatusState,
    Evidence, ExtractorRef, ExtractorState, ExtractorStatus, ParserRef, ParserState, ParserStatus,
    PdfInspectResponse, Provenance, SemanticHint, SourceDocument, TrackyError,
    PDF_INSPECT_SCHEMA_VERSION,
};
use tracky::storage::{apply_migrations, persist_pdf_import};

fn temporary_database() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let connection = Connection::open(db_path).expect("open temp sqlite db");
    (dir, connection)
}

fn inspect_response(hash: &str) -> PdfInspectResponse {
    inspect_response_with_fingerprint(hash, "fp_redacted_001")
}

fn inspect_response_with_fingerprint(hash: &str, fingerprint: &str) -> PdfInspectResponse {
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
            fingerprint: fingerprint.to_string(),
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
        semantic_hint: SemanticHint::BankMovement,
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

fn second_candidate(
    mut response: PdfInspectResponse,
    id_suffix: &str,
    fingerprint: &str,
) -> PdfInspectResponse {
    let mut candidate = response.candidates[0].clone();
    candidate.id = format!("cand_{}", id_suffix);
    candidate.duplicate_status.fingerprint = fingerprint.to_string();
    candidate.description = format!("{} alt", candidate.description);
    response.parser_status.candidates_found = 2;
    response.parser_status.candidates_valid = 2;
    response.candidates.push(candidate);
    response
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
    assert_eq!(response.import_batch.as_ref().unwrap().duplicate_count, 0);
    assert_eq!(
        response.candidates[0].duplicate_status.status,
        DuplicateStatusState::Unique
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

    let persisted: (String, String, String, String, String, String) = connection
        .query_row(
            "SELECT c.status, c.duplicate_status, c.semantic_hint, b.status, p.raw_storage_policy, p.evidence_text_redacted
             FROM candidate_transactions c
             JOIN import_batches b ON b.id = c.import_batch_id
             JOIN provenance p ON p.candidate_transaction_id = c.id",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )
        .expect("read persisted statuses");
    assert_eq!(
        persisted,
        (
            "pending_review".to_string(),
            "unique".to_string(),
            "bank_movement".to_string(),
            "completed".to_string(),
            "redacted_only".to_string(),
            "2026-05-31 REDACTED_COUNTERPARTY <amount>".to_string(),
        )
    );
}

#[test]
fn importing_matching_transaction_from_different_document_marks_possible_duplicate() {
    let (_dir, mut connection) = temporary_database();
    apply_migrations(&connection).expect("apply migrations");
    let first_hash = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    let second_hash = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

    let first = persist_pdf_import(
        &mut connection,
        inspect_response_with_fingerprint(first_hash, "fp_same_transaction"),
    )
    .expect("first import");
    let duplicate = persist_pdf_import(
        &mut connection,
        inspect_response_with_fingerprint(second_hash, "fp_same_transaction"),
    )
    .expect("second import");

    assert!(first.ok);
    assert!(duplicate.ok);
    assert_eq!(duplicate.import_batch.as_ref().unwrap().duplicate_count, 1);
    assert_eq!(
        duplicate.candidates[0].status,
        CandidateStatus::PossibleDuplicate
    );
    assert_eq!(
        duplicate.candidates[0].duplicate_status.status,
        DuplicateStatusState::ExactDuplicate
    );
    assert_eq!(
        duplicate.candidates[0]
            .duplicate_status
            .matched_candidate_ids,
        vec![first.candidates[0].id.clone()]
    );
    assert_eq!(
        duplicate.candidates[0].duplicate_status.reason.as_deref(),
        Some("normalized_transaction_fingerprint_matched")
    );

    let persisted: (String, String, i64, i64, i64, i64) = connection
        .query_row(
            "SELECT
                (SELECT status FROM candidate_transactions WHERE id = ?1),
                (SELECT duplicate_status FROM candidate_transactions WHERE id = ?1),
                (SELECT COUNT(*) FROM transaction_duplicate_markers WHERE candidate_transaction_id = ?1),
                (SELECT COUNT(*) FROM canonical_transactions),
                (SELECT COUNT(*) FROM import_batches),
                (SELECT duplicate_count FROM import_batches WHERE id = ?2)",
            rusqlite::params![
                &duplicate.candidates[0].id,
                duplicate.import_batch.as_ref().unwrap().id
            ],
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
        .expect("read duplicate persistence");
    assert_eq!(
        persisted,
        (
            "possible_duplicate".to_string(),
            "exact_duplicate".to_string(),
            1,
            0,
            2,
            1,
        )
    );

    let json = serde_json::to_value(&duplicate).expect("serializes import response");
    assert_eq!(json["import_batch"]["duplicate_count"], 1);
    assert_eq!(json["candidates"][0]["status"], "possible_duplicate");
    assert_eq!(
        json["candidates"][0]["duplicate_status"]["matched_candidate_ids"],
        serde_json::json!([first.candidates[0].id])
    );
    assert_eq!(
        json["candidates"][0]["duplicate_status"]["status"],
        "exact_duplicate"
    );
}

#[test]
fn near_match_detection_is_scoped_by_institution() {
    let (_dir, mut connection) = temporary_database();
    apply_migrations(&connection).expect("apply migrations");

    persist_pdf_import(
        &mut connection,
        inspect_response_with_fingerprint(
            "4444444444444444444444444444444444444444444444444444444444444444",
            "fp_nequi_transaction",
        ),
    )
    .expect("first import");
    let mut rappi = inspect_response_with_fingerprint(
        "5555555555555555555555555555555555555555555555555555555555555555",
        "fp_rappi_same_fields",
    );
    rappi.source_document.institution_hint = "rappi".to_string();
    rappi.source_document.account_hint.label = "Rappi card".to_string();
    rappi.candidates[0].institution_hint = "rappi".to_string();
    rappi.candidates[0].account_hint.label = "Rappi card".to_string();

    let response = persist_pdf_import(&mut connection, rappi).expect("rappi import");

    assert_eq!(response.import_batch.as_ref().unwrap().duplicate_count, 0);
    assert_eq!(
        response.candidates[0].status,
        CandidateStatus::PendingReview
    );
    assert_eq!(
        response.candidates[0].duplicate_status.status,
        DuplicateStatusState::Unique
    );
}

#[test]
fn near_match_finds_legacy_fingerprint_keys_with_case_normalized_currency() {
    let (_dir, mut connection) = temporary_database();
    apply_migrations(&connection).expect("apply migrations");
    connection
        .execute(
            "INSERT INTO institutions (id, name) VALUES (?1, ?2)",
            rusqlite::params!["inst_nequi", "nequi"],
        )
        .expect("seed institution");
    connection
        .execute(
            "INSERT INTO accounts (id, institution_id, label, currency)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["acct_nequi", "inst_nequi", "Nequi wallet", "COP"],
        )
        .expect("seed account");
    connection
        .execute(
            "INSERT INTO canonical_transactions (id, account_id, posted_date, description, amount_minor, currency)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                "txn_legacy",
                "acct_nequi",
                "2026-05-31",
                "Redacted merchant legacy",
                -4590000_i64,
                "COP"
            ],
        )
        .expect("seed legacy canonical transaction");
    connection
        .execute(
            "INSERT INTO transaction_fingerprints (
                id, fingerprint, canonical_transaction_id, duplicate_status,
                normalized_account_key, normalized_posted_date, normalized_amount_minor,
                normalized_currency, normalized_description
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                "fp_legacy_canonical",
                "legacy_fingerprint_shape",
                "txn_legacy",
                "unique",
                "nequi wallet",
                "2026-05-31",
                -4590000_i64,
                "cop",
                "redacted merchant legacy"
            ],
        )
        .expect("seed legacy fingerprint");

    let response = persist_pdf_import(
        &mut connection,
        inspect_response_with_fingerprint(
            "8888888888888888888888888888888888888888888888888888888888888888",
            "new_fingerprint_shape",
        ),
    )
    .expect("persist import");

    assert_eq!(response.import_batch.as_ref().unwrap().duplicate_count, 1);
    assert_eq!(
        response.candidates[0].duplicate_status.status,
        DuplicateStatusState::PossibleDuplicate
    );
    assert_eq!(
        response.candidates[0]
            .duplicate_status
            .matched_canonical_transaction_ids,
        vec!["txn_legacy".to_string()]
    );
}

#[test]
fn legacy_label_only_near_match_is_scoped_by_institution_metadata() {
    let (_dir, mut connection) = temporary_database();
    apply_migrations(&connection).expect("apply migrations");
    connection
        .execute(
            "INSERT INTO institutions (id, name) VALUES (?1, ?2)",
            rusqlite::params!["inst_rappi", "rappi"],
        )
        .expect("seed institution");
    connection
        .execute(
            "INSERT INTO accounts (id, institution_id, label, currency)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["acct_rappi", "inst_rappi", "Nequi wallet", "COP"],
        )
        .expect("seed account");
    connection
        .execute(
            "INSERT INTO canonical_transactions (id, account_id, posted_date, description, amount_minor, currency)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                "txn_legacy_rappi",
                "acct_rappi",
                "2026-05-31",
                "Redacted merchant legacy",
                -4590000_i64,
                "COP"
            ],
        )
        .expect("seed cross-institution canonical transaction");
    connection
        .execute(
            "INSERT INTO transaction_fingerprints (
                id, fingerprint, canonical_transaction_id, duplicate_status,
                normalized_account_key, normalized_posted_date, normalized_amount_minor,
                normalized_currency, normalized_description
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                "fp_legacy_rappi",
                "legacy_rappi_fingerprint_shape",
                "txn_legacy_rappi",
                "unique",
                "nequi wallet",
                "2026-05-31",
                -4590000_i64,
                "cop",
                "redacted merchant legacy"
            ],
        )
        .expect("seed cross-institution legacy fingerprint");

    let response = persist_pdf_import(
        &mut connection,
        inspect_response_with_fingerprint(
            "9999999999999999999999999999999999999999999999999999999999999999",
            "new_nequi_fingerprint_shape",
        ),
    )
    .expect("persist import");

    assert_eq!(response.import_batch.as_ref().unwrap().duplicate_count, 0);
    assert_eq!(
        response.candidates[0].duplicate_status.status,
        DuplicateStatusState::Unique
    );
}

#[test]
fn duplicate_detection_does_not_override_reviewed_candidate_status() {
    let (_dir, mut connection) = temporary_database();
    apply_migrations(&connection).expect("apply migrations");
    let first = persist_pdf_import(
        &mut connection,
        inspect_response_with_fingerprint(
            "6666666666666666666666666666666666666666666666666666666666666666",
            "fp_reviewed_match",
        ),
    )
    .expect("first import");
    let mut reviewed = inspect_response_with_fingerprint(
        "7777777777777777777777777777777777777777777777777777777777777777",
        "fp_reviewed_match",
    );
    reviewed.candidates[0].status = CandidateStatus::Accepted;

    let response =
        persist_pdf_import(&mut connection, reviewed).expect("reviewed duplicate import");

    assert_eq!(response.import_batch.as_ref().unwrap().duplicate_count, 1);
    assert_eq!(response.candidates[0].status, CandidateStatus::Accepted);
    assert_eq!(
        response.candidates[0].duplicate_status.status,
        DuplicateStatusState::ExactDuplicate
    );
    assert_eq!(
        response.candidates[0]
            .duplicate_status
            .matched_candidate_ids,
        vec![first.candidates[0].id.clone()]
    );
}

#[test]
fn importing_near_match_marks_possible_duplicate_status() {
    let (_dir, mut connection) = temporary_database();
    apply_migrations(&connection).expect("apply migrations");

    persist_pdf_import(
        &mut connection,
        inspect_response_with_fingerprint(
            "1111111111111111111111111111111111111111111111111111111111111111",
            "fp_original_description",
        ),
    )
    .expect("first import");
    let near_duplicate = persist_pdf_import(
        &mut connection,
        inspect_response_with_fingerprint(
            "2222222222222222222222222222222222222222222222222222222222222222",
            "fp_different_description_same_core_fields",
        ),
    )
    .expect("near duplicate import");

    assert_eq!(
        near_duplicate
            .import_batch
            .as_ref()
            .unwrap()
            .duplicate_count,
        1
    );
    assert_eq!(
        near_duplicate.candidates[0].status,
        CandidateStatus::PossibleDuplicate
    );
    assert_eq!(
        near_duplicate.candidates[0].duplicate_status.status,
        DuplicateStatusState::PossibleDuplicate
    );
    assert_eq!(
        near_duplicate.candidates[0]
            .duplicate_status
            .reason
            .as_deref(),
        Some("normalized_transaction_fields_matched")
    );
}

#[test]
fn same_batch_exact_duplicate_marks_both_candidates() {
    let (_dir, mut connection) = temporary_database();
    apply_migrations(&connection).expect("apply migrations");
    let response = second_candidate(
        inspect_response_with_fingerprint(
            "3333333333333333333333333333333333333333333333333333333333333333",
            "fp_repeated_in_batch",
        ),
        "same_batch_second",
        "fp_repeated_in_batch",
    );

    let persisted = persist_pdf_import(&mut connection, response).expect("persist import");

    assert_eq!(persisted.import_batch.as_ref().unwrap().duplicate_count, 2);
    assert!(persisted
        .candidates
        .iter()
        .all(|candidate| candidate.status == CandidateStatus::PossibleDuplicate));
    assert!(
        persisted
            .candidates
            .iter()
            .all(|candidate| candidate.duplicate_status.status
                == DuplicateStatusState::ExactDuplicate)
    );
}

#[test]
fn importing_candidate_matching_canonical_records_duplicate_without_creating_canonical() {
    let (_dir, mut connection) = temporary_database();
    apply_migrations(&connection).expect("apply migrations");
    connection
        .execute(
            "INSERT INTO canonical_transactions (id, posted_date, description, amount_minor, currency)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                "txn_existing",
                "2026-05-31",
                "Redacted merchant",
                -4590000_i64,
                "COP"
            ],
        )
        .expect("seed canonical transaction");
    connection
        .execute(
            "INSERT INTO transaction_fingerprints (
                id, fingerprint, canonical_transaction_id, duplicate_status,
                normalized_posted_date, normalized_amount_minor,
                normalized_currency, normalized_description
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                "fp_existing_canonical",
                "fp_canonical_match",
                "txn_existing",
                "unique",
                "2026-05-31",
                -4590000_i64,
                "COP",
                "redacted merchant"
            ],
        )
        .expect("seed canonical fingerprint");

    let response = persist_pdf_import(
        &mut connection,
        inspect_response_with_fingerprint(
            "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
            "fp_canonical_match",
        ),
    )
    .expect("persist import");

    assert!(response.ok);
    assert_eq!(response.import_batch.as_ref().unwrap().duplicate_count, 1);
    assert_eq!(
        response.candidates[0].status,
        CandidateStatus::PossibleDuplicate
    );
    assert_eq!(
        response.candidates[0]
            .duplicate_status
            .matched_canonical_transaction_ids,
        vec!["txn_existing".to_string()]
    );

    let counts: (i64, i64) = connection
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM canonical_transactions),
                (SELECT COUNT(*) FROM transaction_duplicate_markers WHERE candidate_transaction_id = ?1)",
            [&response.candidates[0].id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read counts");
    assert_eq!(counts, (1, 1));
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
