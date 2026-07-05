use rusqlite::Connection;
use std::process::Command;
use tracky::pdf::{
    AccountHint, CandidateStatus, CandidateTransaction, CredentialSource, DirectionHint,
    DocumentDuplicateState, DocumentDuplicateStatus, DuplicateStatus, DuplicateStatusState,
    Evidence, ExtractorRef, ExtractorState, ExtractorStatus, ParserRef, ParserState, ParserStatus,
    PdfInspectResponse, Provenance, SourceDocument, TrackyError, PDF_INSPECT_SCHEMA_VERSION,
};
use tracky::storage::{apply_migrations, persist_pdf_import};

fn inspect_response(hash: &str) -> PdfInspectResponse {
    let source_document = SourceDocument {
        id: format!("srcdoc_{}", &hash[..26]),
        input_name: "synthetic-redacted.pdf".to_string(),
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
        id: "cand_review_001".to_string(),
        import_batch_id: None,
        source_document_id: source_document.id.clone(),
        status: CandidateStatus::PendingReview,
        duplicate_status: DuplicateStatus {
            status: DuplicateStatusState::NotChecked,
            fingerprint: "fp_review_001".to_string(),
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
    let mut second = candidate.clone();
    second.id = "cand_review_002".to_string();
    second.duplicate_status.fingerprint = "fp_review_002".to_string();
    second.description = "Another redacted merchant".to_string();
    second.amount_minor = -123400;
    second.provenance.row_index = 18;

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
            candidates_found: 2,
            candidates_valid: 2,
            warnings: Vec::new(),
        },
        candidates: vec![candidate, second],
        errors: Vec::<TrackyError>::new(),
    }
}

fn tracky() -> &'static str {
    env!("CARGO_BIN_EXE_tracky")
}

#[test]
fn candidate_review_cli_lists_accepts_and_rejects_with_audit_links() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let mut connection = Connection::open(&db_path).expect("open db");
    apply_migrations(&connection).expect("apply migrations");
    let import = persist_pdf_import(
        &mut connection,
        inspect_response("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"),
    )
    .expect("persist synthetic import");
    let batch_id = import.import_batch.as_ref().unwrap().id.clone();
    connection
        .execute(
            "UPDATE candidate_transactions
             SET status = 'possible_duplicate', duplicate_status = 'possible_duplicate'
             WHERE id = 'cand_review_001'",
            [],
        )
        .expect("mark candidate as possible duplicate");
    let canonical_count_before: i64 = connection
        .query_row("SELECT COUNT(*) FROM canonical_transactions", [], |row| {
            row.get(0)
        })
        .expect("count canonical before review");
    assert_eq!(canonical_count_before, 0);
    drop(connection);

    let list_output = Command::new(tracky())
        .args([
            "candidates",
            "list",
            "--db",
            db_path.to_str().unwrap(),
            "--import-batch-id",
            &batch_id,
            "--status",
            "possible_duplicate",
            "--json",
        ])
        .output()
        .expect("run candidates list");
    assert!(
        list_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&list_output.stderr)
    );
    let list_json: serde_json::Value =
        serde_json::from_slice(&list_output.stdout).expect("list json");
    assert_eq!(list_json["schema_version"], "tracky.candidate-review.v1");
    assert_eq!(list_json["command"], "candidates list");
    assert_eq!(list_json["candidates"].as_array().unwrap().len(), 1);
    assert_eq!(list_json["candidates"][0]["status"], "possible_duplicate");

    let accept_output = Command::new(tracky())
        .args([
            "candidates",
            "accept",
            "cand_review_001",
            "--db",
            db_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("run candidates accept");
    assert!(
        accept_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&accept_output.stderr)
    );
    let accept_json: serde_json::Value =
        serde_json::from_slice(&accept_output.stdout).expect("accept json");
    assert_eq!(accept_json["ok"], true);
    assert_eq!(accept_json["candidate"]["status"], "accepted");
    let canonical_id = accept_json["canonical_transaction"]["id"].as_str().unwrap();
    assert_eq!(
        accept_json["canonical_transaction"]["created_from_candidate_id"],
        "cand_review_001"
    );
    assert_eq!(
        accept_json["candidate"]["provenance"]["canonical_transaction_id"],
        canonical_id
    );

    let reject_output = Command::new(tracky())
        .args([
            "candidates",
            "reject",
            "cand_review_002",
            "--db",
            db_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("run candidates reject");
    assert!(
        reject_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&reject_output.stderr)
    );
    let reject_json: serde_json::Value =
        serde_json::from_slice(&reject_output.stdout).expect("reject json");
    assert_eq!(reject_json["ok"], true);
    assert_eq!(reject_json["candidate"]["status"], "rejected");
    assert_eq!(
        reject_json["candidate"]["provenance"]["candidate_transaction_id"],
        "cand_review_002"
    );

    let double_accept = Command::new(tracky())
        .args([
            "candidates",
            "accept",
            "cand_review_001",
            "--db",
            db_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("run second accept");
    assert!(!double_accept.status.success());
    let double_json: serde_json::Value =
        serde_json::from_slice(&double_accept.stdout).expect("double json");
    assert_eq!(
        double_json["errors"][0]["code"],
        "candidate_already_accepted"
    );

    let connection = Connection::open(&db_path).expect("reopen db");
    let audit_links: (i64, i64, i64, i64, i64) = connection
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM canonical_transactions),
                (SELECT COUNT(*) FROM candidate_transactions WHERE id = 'cand_review_001' AND status = 'accepted' AND canonical_transaction_id IS NOT NULL),
                (SELECT COUNT(*) FROM provenance WHERE candidate_transaction_id = 'cand_review_001' AND canonical_transaction_id IS NOT NULL),
                (SELECT COUNT(*) FROM transaction_fingerprints WHERE candidate_transaction_id IS NULL AND canonical_transaction_id IS NOT NULL),
                (SELECT COUNT(*) FROM provenance WHERE candidate_transaction_id = 'cand_review_002' AND canonical_transaction_id IS NULL)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .expect("read audit links");
    assert_eq!(audit_links, (1, 1, 1, 1, 1));
}
