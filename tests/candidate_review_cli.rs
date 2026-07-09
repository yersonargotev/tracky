use rusqlite::Connection;
use std::process::Command;
use tracky::pdf::{
    AccountHint, CandidateStatus, CandidateTransaction, CredentialSource, DirectionHint,
    DocumentDuplicateState, DocumentDuplicateStatus, DuplicateStatus, DuplicateStatusState,
    Evidence, ExtractorRef, ExtractorState, ExtractorStatus, ParserRef, ParserState, ParserStatus,
    PdfInspectResponse, Provenance, SemanticHint, SourceDocument, TrackyError,
    PDF_INSPECT_SCHEMA_VERSION,
};
use tracky::storage::{
    apply_migrations, persist_pdf_import, register_owned_account, AccountRegisterInput,
};

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

fn register_account(
    connection: &Connection,
    institution: &str,
    label: &str,
    account_type: &str,
) -> String {
    register_owned_account(
        connection,
        AccountRegisterInput {
            institution: institution.to_string(),
            label: label.to_string(),
            account_type: account_type.to_string(),
            currency: "COP".to_string(),
            masked_identifier: None,
        },
    )
    .expect("register owned account")
    .account
    .expect("registered account")
    .id
}

struct TransferCandidateFixture<'a> {
    hash: &'a str,
    institution: &'a str,
    account_label: &'a str,
    candidate_id: &'a str,
    description: &'a str,
    amount_minor: i64,
    direction_hint: DirectionHint,
    semantic_hint: SemanticHint,
}

fn transfer_inspect_response(fixture: TransferCandidateFixture<'_>) -> PdfInspectResponse {
    let source_document = SourceDocument {
        id: format!("srcdoc_{}", &fixture.hash[..26]),
        input_name: format!("{}-synthetic-redacted.pdf", fixture.institution),
        content_sha256: fixture.hash.to_string(),
        mime_type: "application/pdf",
        byte_size: 42,
        institution_hint: fixture.institution.to_string(),
        account_hint: AccountHint {
            label: fixture.account_label.to_string(),
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
        id: fixture.candidate_id.to_string(),
        import_batch_id: None,
        source_document_id: source_document.id.clone(),
        status: CandidateStatus::PendingReview,
        duplicate_status: DuplicateStatus {
            status: DuplicateStatusState::NotChecked,
            fingerprint: format!("fp_{}", fixture.candidate_id),
            matched_candidate_ids: Vec::new(),
            matched_canonical_transaction_ids: Vec::new(),
            reason: None,
        },
        institution_hint: fixture.institution.to_string(),
        account_hint: source_document.account_hint.clone(),
        posted_date: "2026-05-31".to_string(),
        description: fixture.description.to_string(),
        amount_minor: fixture.amount_minor,
        currency: "COP",
        balance_minor: None,
        direction_hint: fixture.direction_hint,
        semantic_hint: fixture.semantic_hint,
        confidence: 0.95,
        provenance: Provenance {
            source_document_id: source_document.id.clone(),
            page_number: 1,
            row_index: 1,
            bbox: None,
            extractor: ExtractorRef {
                name: "pdf_oxide",
                version: None,
            },
            parser: ParserRef {
                id: format!("{}.statement.v1", fixture.institution),
                version: "1",
            },
            evidence: Evidence {
                redaction: "redacted",
                text: format!("2026-05-31 {} <amount>", fixture.description),
                raw_storage_policy: "redacted_only",
            },
            confidence: 0.95,
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
            pages_seen: 1,
            pages_extracted: 1,
            requires_document_credential: false,
            credential_source: CredentialSource::None,
            warnings: Vec::new(),
        },
        parser_status: ParserStatus {
            status: ParserState::Succeeded,
            parser_id: format!("{}.statement.v1", fixture.institution),
            parser_version: "1",
            candidates_found: 1,
            candidates_valid: 1,
            warnings: Vec::new(),
        },
        candidates: vec![candidate],
        errors: Vec::<TrackyError>::new(),
    }
}

fn persist_transfer_candidates(
    connection: &mut Connection,
    rappi_amount_minor: i64,
) -> (String, String) {
    persist_pdf_import(
        connection,
        transfer_inspect_response(TransferCandidateFixture {
            hash: "1111111111111111111111111111111111111111111111111111111111111111",
            institution: "nequi",
            account_label: "Nequi wallet",
            candidate_id: "cand_nequi_pse_payment",
            description: "COMPRA PSE EN BANCO REDACTED",
            amount_minor: -4590000,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::BankMovement,
        }),
    )
    .expect("persist nequi transfer candidate");
    persist_pdf_import(
        connection,
        transfer_inspect_response(TransferCandidateFixture {
            hash: "2222222222222222222222222222222222222222222222222222222222222222",
            institution: "rappi",
            account_label: "Rappi card",
            candidate_id: "cand_rappi_pse_payment",
            description: "PAGOS POR PSE REDACTED",
            amount_minor: rappi_amount_minor,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::CardPayment,
        }),
    )
    .expect("persist rappi transfer candidate");
    (
        "cand_nequi_pse_payment".to_string(),
        "cand_rappi_pse_payment".to_string(),
    )
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

#[test]
fn candidate_review_cli_lists_and_accepts_nequi_to_rappicard_pse_pair() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let mut connection = Connection::open(&db_path).expect("open db");
    apply_migrations(&connection).expect("apply migrations");
    register_account(&connection, "nequi", "Nequi wallet", "wallet");
    register_account(&connection, "rappi", "RappiCard", "credit_card");
    let (from_candidate_id, to_candidate_id) =
        persist_transfer_candidates(&mut connection, -4590000);
    drop(connection);

    let list_output = Command::new(tracky())
        .args([
            "candidates",
            "list-transfer-pairs",
            "--db",
            db_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("run transfer pair list");
    assert!(
        list_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&list_output.stderr)
    );
    let list_json: serde_json::Value =
        serde_json::from_slice(&list_output.stdout).expect("list transfer json");
    assert_eq!(list_json["schema_version"], "tracky.transfer-review.v1");
    assert_eq!(list_json["command"], "candidates list-transfer-pairs");
    assert_eq!(list_json["transfer_pairs"].as_array().unwrap().len(), 1);
    assert_eq!(list_json["transfer_pairs"][0]["amount_minor"], 4590000);
    assert_eq!(
        list_json["transfer_pairs"][0]["from_candidate"]["id"],
        from_candidate_id
    );
    assert_eq!(
        list_json["transfer_pairs"][0]["to_candidate"]["semantic_hint"],
        "card_payment"
    );
    assert_eq!(
        list_json["canonical_transactions"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let accept_output = Command::new(tracky())
        .args([
            "candidates",
            "accept-transfer-pair",
            &from_candidate_id,
            &to_candidate_id,
            "--db",
            db_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("run transfer pair accept");
    assert!(
        accept_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&accept_output.stderr)
    );
    let accept_json: serde_json::Value =
        serde_json::from_slice(&accept_output.stdout).expect("accept transfer json");
    assert_eq!(accept_json["ok"], true);
    assert_eq!(
        accept_json["transfer_pair"]["transfer_kind"],
        "card_payment"
    );
    assert_eq!(
        accept_json["transfer_pair"]["from_candidate"]["status"],
        "accepted"
    );
    assert_eq!(
        accept_json["transfer_pair"]["to_candidate"]["status"],
        "accepted"
    );
    assert_eq!(
        accept_json["canonical_transactions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(accept_json["canonical_transactions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|transaction| transaction["transaction_kind"] == "own_account_transfer"));
    assert_eq!(
        accept_json["canonical_transactions"][0]["amount_minor"],
        -4590000
    );
    assert_eq!(
        accept_json["canonical_transactions"][1]["amount_minor"],
        4590000
    );

    let double_accept = Command::new(tracky())
        .args([
            "candidates",
            "accept-transfer-pair",
            &from_candidate_id,
            &to_candidate_id,
            "--db",
            db_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("run second transfer pair accept");
    assert!(!double_accept.status.success());
    let double_json: serde_json::Value =
        serde_json::from_slice(&double_accept.stdout).expect("double transfer json");
    assert_eq!(
        double_json["errors"][0]["code"],
        "transfer_pair_not_reviewable"
    );

    let connection = Connection::open(&db_path).expect("reopen db");
    let audit: (i64, i64, i64, i64) = connection
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM canonical_transfer_pairs),
                (SELECT COUNT(*) FROM canonical_transactions WHERE transaction_kind = 'own_account_transfer'),
                (SELECT COUNT(*) FROM provenance WHERE candidate_transaction_id IN (?1, ?2) AND canonical_transaction_id IS NOT NULL),
                (SELECT COALESCE(SUM(amount_minor), 0) FROM canonical_transactions WHERE transaction_kind = 'own_account_transfer')",
            rusqlite::params![&from_candidate_id, &to_candidate_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("read transfer audit");
    assert_eq!(audit, (1, 2, 2, 0));
}

#[test]
fn candidate_review_cli_refuses_non_matching_or_unresolved_transfer_pairs() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let mut connection = Connection::open(&db_path).expect("open db");
    apply_migrations(&connection).expect("apply migrations");
    register_account(&connection, "nequi", "Nequi wallet", "wallet");
    register_account(&connection, "rappi", "RappiCard", "credit_card");
    let (from_candidate_id, to_candidate_id) = persist_transfer_candidates(&mut connection, 123400);
    drop(connection);

    let list_output = Command::new(tracky())
        .args([
            "candidates",
            "list-transfer-pairs",
            "--db",
            db_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("run non-match transfer pair list");
    assert!(
        list_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&list_output.stderr)
    );
    let list_json: serde_json::Value =
        serde_json::from_slice(&list_output.stdout).expect("non-match list transfer json");
    assert_eq!(list_json["transfer_pairs"].as_array().unwrap().len(), 0);

    let accept_output = Command::new(tracky())
        .args([
            "candidates",
            "accept-transfer-pair",
            &from_candidate_id,
            &to_candidate_id,
            "--db",
            db_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("run non-match transfer pair accept");
    assert!(!accept_output.status.success());
    let accept_json: serde_json::Value =
        serde_json::from_slice(&accept_output.stdout).expect("non-match accept json");
    assert_eq!(accept_json["ok"], false);
    assert_eq!(
        accept_json["errors"][0]["code"],
        "transfer_pair_not_matching"
    );

    let unresolved_dir = tempfile::tempdir().expect("temp dir");
    let unresolved_db = unresolved_dir.path().join("tracky.sqlite");
    let mut unresolved_connection = Connection::open(&unresolved_db).expect("open db");
    apply_migrations(&unresolved_connection).expect("apply migrations");
    register_account(&unresolved_connection, "nequi", "Nequi wallet", "wallet");
    let (unresolved_from, unresolved_to) =
        persist_transfer_candidates(&mut unresolved_connection, 4590000);
    drop(unresolved_connection);

    let unresolved_accept = Command::new(tracky())
        .args([
            "candidates",
            "accept-transfer-pair",
            &unresolved_from,
            &unresolved_to,
            "--db",
            unresolved_db.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("run unresolved transfer pair accept");
    assert!(!unresolved_accept.status.success());
    let unresolved_json: serde_json::Value =
        serde_json::from_slice(&unresolved_accept.stdout).expect("unresolved accept json");
    assert_eq!(
        unresolved_json["errors"][0]["code"],
        "transfer_pair_account_unresolved"
    );

    let rejected_dir = tempfile::tempdir().expect("temp dir");
    let rejected_db = rejected_dir.path().join("tracky.sqlite");
    let mut rejected_connection = Connection::open(&rejected_db).expect("open db");
    apply_migrations(&rejected_connection).expect("apply migrations");
    register_account(&rejected_connection, "nequi", "Nequi wallet", "wallet");
    register_account(&rejected_connection, "rappi", "RappiCard", "credit_card");
    let (rejected_from, rejected_to) =
        persist_transfer_candidates(&mut rejected_connection, -4590000);
    rejected_connection
        .execute(
            "UPDATE candidate_transactions SET status = 'rejected' WHERE id = ?1",
            rusqlite::params![&rejected_to],
        )
        .expect("reject candidate");
    drop(rejected_connection);

    let rejected_accept = Command::new(tracky())
        .args([
            "candidates",
            "accept-transfer-pair",
            &rejected_from,
            &rejected_to,
            "--db",
            rejected_db.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("run rejected transfer pair accept");
    assert!(!rejected_accept.status.success());
    let rejected_json: serde_json::Value =
        serde_json::from_slice(&rejected_accept.stdout).expect("rejected accept json");
    assert_eq!(
        rejected_json["errors"][0]["code"],
        "transfer_pair_not_reviewable"
    );

    let non_owned_dir = tempfile::tempdir().expect("temp dir");
    let non_owned_db = non_owned_dir.path().join("tracky.sqlite");
    let mut non_owned_connection = Connection::open(&non_owned_db).expect("open db");
    apply_migrations(&non_owned_connection).expect("apply migrations");
    register_account(&non_owned_connection, "nequi", "Nequi wallet", "wallet");
    let rappi_id = register_account(&non_owned_connection, "rappi", "RappiCard", "credit_card");
    let (non_owned_from, non_owned_to) =
        persist_transfer_candidates(&mut non_owned_connection, -4590000);
    non_owned_connection
        .execute(
            "UPDATE accounts SET is_owned = 0 WHERE id = ?1",
            rusqlite::params![rappi_id],
        )
        .expect("mark account non-owned");
    drop(non_owned_connection);

    let non_owned_accept = Command::new(tracky())
        .args([
            "candidates",
            "accept-transfer-pair",
            &non_owned_from,
            &non_owned_to,
            "--db",
            non_owned_db.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("run non-owned transfer pair accept");
    assert!(!non_owned_accept.status.success());
    let non_owned_json: serde_json::Value =
        serde_json::from_slice(&non_owned_accept.stdout).expect("non-owned accept json");
    assert_eq!(
        non_owned_json["errors"][0]["code"],
        "transfer_pair_account_not_owned"
    );
}

fn create_income_source_cli(db: &str, name: &str) -> serde_json::Value {
    let output = Command::new(tracky())
        .args([
            "income-sources",
            "create",
            "--db",
            db,
            "--name",
            name,
            "--json",
        ])
        .output()
        .expect("run income source create");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("income source create json")
}

#[test]
fn candidate_review_cli_accepts_nequi_income_with_explicit_source_and_kind() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();
    let mut connection = Connection::open(&db_path).expect("open db");
    apply_migrations(&connection).expect("apply migrations");
    register_account(&connection, "nequi", "Nequi wallet", "wallet");
    persist_pdf_import(
        &mut connection,
        transfer_inspect_response(TransferCandidateFixture {
            hash: "3333333333333333333333333333333333333333333333333333333333333333",
            institution: "nequi",
            account_label: "Nequi wallet",
            candidate_id: "cand_nequi_salary_income",
            description: "PAGO NOMINA REDACTED EMPLOYER",
            amount_minor: 650000000,
            direction_hint: DirectionHint::Inflow,
            semantic_hint: SemanticHint::BankMovement,
        }),
    )
    .expect("persist salary-like income candidate");
    persist_pdf_import(
        &mut connection,
        transfer_inspect_response(TransferCandidateFixture {
            hash: "4444444444444444444444444444444444444444444444444444444444444444",
            institution: "nequi",
            account_label: "Nequi wallet",
            candidate_id: "cand_nequi_side_income",
            description: "PAGO CLIENTE REDACTED",
            amount_minor: 18000000,
            direction_hint: DirectionHint::Inflow,
            semantic_hint: SemanticHint::BankMovement,
        }),
    )
    .expect("persist smaller non-salary income candidate");
    drop(connection);

    let payroll_source = create_income_source_cli(db, "Redacted Employer");
    assert_eq!(payroll_source["schema_version"], "tracky.income-sources.v1");
    assert_eq!(payroll_source["command"], "income-sources create");
    assert_eq!(
        payroll_source["income_source"]["id"],
        "incsrc_redacted_employer"
    );
    let client_source = create_income_source_cli(db, "Redacted Client");
    assert_eq!(
        client_source["income_source"]["id"],
        "incsrc_redacted_client"
    );

    let list_sources = Command::new(tracky())
        .args(["income-sources", "list", "--db", db, "--json"])
        .output()
        .expect("run income source list");
    assert!(
        list_sources.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&list_sources.stderr)
    );
    let list_json: serde_json::Value =
        serde_json::from_slice(&list_sources.stdout).expect("income source list json");
    assert_eq!(list_json["schema_version"], "tracky.income-sources.v1");
    assert_eq!(list_json["income_sources"].as_array().unwrap().len(), 2);

    let accept_salary = Command::new(tracky())
        .args([
            "candidates",
            "accept-income",
            "cand_nequi_salary_income",
            "--db",
            db,
            "--income-source-id",
            "incsrc_redacted_employer",
            "--income-kind",
            "salary",
            "--json",
        ])
        .output()
        .expect("run accept salary income");
    assert!(
        accept_salary.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&accept_salary.stderr)
    );
    let salary_json: serde_json::Value =
        serde_json::from_slice(&accept_salary.stdout).expect("salary accept json");
    assert_eq!(salary_json["ok"], true);
    assert_eq!(salary_json["candidate"]["status"], "accepted");
    assert_eq!(
        salary_json["canonical_transaction"]["transaction_kind"],
        "income"
    );
    assert_eq!(
        salary_json["canonical_transaction"]["income_source_id"],
        "incsrc_redacted_employer"
    );
    assert_eq!(
        salary_json["canonical_transaction"]["income_kind"],
        "salary"
    );
    assert_eq!(
        salary_json["canonical_transaction"]["amount_minor"],
        650000000
    );
    let canonical_id = salary_json["canonical_transaction"]["id"].as_str().unwrap();
    assert_eq!(
        salary_json["candidate"]["provenance"]["canonical_transaction_id"],
        canonical_id
    );

    let double_accept = Command::new(tracky())
        .args([
            "candidates",
            "accept-income",
            "cand_nequi_salary_income",
            "--db",
            db,
            "--income-source-id",
            "incsrc_redacted_employer",
            "--income-kind",
            "salary",
            "--json",
        ])
        .output()
        .expect("run second accept income");
    assert!(!double_accept.status.success());
    let double_json: serde_json::Value =
        serde_json::from_slice(&double_accept.stdout).expect("double accept income json");
    assert_eq!(
        double_json["errors"][0]["code"],
        "candidate_already_accepted"
    );

    let accept_client = Command::new(tracky())
        .args([
            "candidates",
            "accept-income",
            "cand_nequi_side_income",
            "--db",
            db,
            "--income-source-id",
            "incsrc_redacted_client",
            "--income-kind",
            "freelance",
            "--json",
        ])
        .output()
        .expect("run accept client income");
    assert!(
        accept_client.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&accept_client.stderr)
    );
    let client_json: serde_json::Value =
        serde_json::from_slice(&accept_client.stdout).expect("client accept json");
    assert_eq!(
        client_json["canonical_transaction"]["income_kind"],
        "freelance"
    );
    assert_eq!(
        client_json["canonical_transaction"]["income_source_id"],
        "incsrc_redacted_client"
    );

    let connection = Connection::open(&db_path).expect("reopen db");
    let audit: (i64, i64, i64) = connection
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM canonical_transactions WHERE transaction_kind = 'income'),
                (SELECT COUNT(*) FROM canonical_transactions WHERE income_source_id IS NOT NULL AND income_kind IS NOT NULL),
                (SELECT COUNT(*) FROM provenance WHERE candidate_transaction_id IN ('cand_nequi_salary_income', 'cand_nequi_side_income') AND canonical_transaction_id IS NOT NULL)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read income audit");
    assert_eq!(audit, (2, 2, 2));
}

#[test]
fn candidate_review_cli_refuses_non_income_transfer_like_or_already_reviewed_income() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();
    let mut connection = Connection::open(&db_path).expect("open db");
    apply_migrations(&connection).expect("apply migrations");
    register_account(&connection, "nequi", "Nequi wallet", "wallet");
    persist_pdf_import(
        &mut connection,
        transfer_inspect_response(TransferCandidateFixture {
            hash: "5555555555555555555555555555555555555555555555555555555555555555",
            institution: "nequi",
            account_label: "Nequi wallet",
            candidate_id: "cand_nequi_outflow_not_income",
            description: "COMPRA REDACTED",
            amount_minor: -2500000,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::BankMovement,
        }),
    )
    .expect("persist non-income candidate");
    persist_pdf_import(
        &mut connection,
        transfer_inspect_response(TransferCandidateFixture {
            hash: "6666666666666666666666666666666666666666666666666666666666666666",
            institution: "rappi",
            account_label: "RappiCard",
            candidate_id: "cand_rappi_card_payment_not_income",
            description: "PAGOS POR PSE REDACTED",
            amount_minor: 4590000,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::CardPayment,
        }),
    )
    .expect("persist card-payment candidate");
    persist_pdf_import(
        &mut connection,
        transfer_inspect_response(TransferCandidateFixture {
            hash: "7777777777777777777777777777777777777777777777777777777777777777",
            institution: "nequi",
            account_label: "Nequi wallet",
            candidate_id: "cand_nequi_rejected_income",
            description: "INGRESO REDACTED",
            amount_minor: 9000000,
            direction_hint: DirectionHint::Inflow,
            semantic_hint: SemanticHint::BankMovement,
        }),
    )
    .expect("persist rejected income candidate");
    connection
        .execute(
            "UPDATE candidate_transactions SET status = 'rejected' WHERE id = 'cand_nequi_rejected_income'",
            [],
        )
        .expect("mark rejected");
    drop(connection);
    create_income_source_cli(db, "Redacted Income Source");

    for (candidate_id, expected_code) in [
        (
            "cand_nequi_outflow_not_income",
            "candidate_not_income_eligible",
        ),
        (
            "cand_rappi_card_payment_not_income",
            "candidate_not_income_eligible",
        ),
        ("cand_nequi_rejected_income", "candidate_already_rejected"),
    ] {
        let output = Command::new(tracky())
            .args([
                "candidates",
                "accept-income",
                candidate_id,
                "--db",
                db,
                "--income-source-id",
                "incsrc_redacted_income_source",
                "--income-kind",
                "other",
                "--json",
            ])
            .output()
            .expect("run refused accept income");
        assert!(!output.status.success());
        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("refused income json");
        assert_eq!(json["errors"][0]["code"], expected_code);
    }

    let transfer_dir = tempfile::tempdir().expect("temp dir");
    let transfer_db_path = transfer_dir.path().join("tracky.sqlite");
    let transfer_db = transfer_db_path.to_str().unwrap();
    let mut transfer_connection = Connection::open(&transfer_db_path).expect("open transfer db");
    apply_migrations(&transfer_connection).expect("apply migrations");
    register_account(&transfer_connection, "nequi", "Nequi wallet", "wallet");
    register_account(
        &transfer_connection,
        "bancolombia",
        "Bancolombia checking",
        "checking",
    );
    persist_pdf_import(
        &mut transfer_connection,
        transfer_inspect_response(TransferCandidateFixture {
            hash: "8888888888888888888888888888888888888888888888888888888888888888",
            institution: "bancolombia",
            account_label: "Bancolombia checking",
            candidate_id: "cand_bancolombia_own_transfer_out",
            description: "TRANSFERENCIA A NEQUI REDACTED",
            amount_minor: -7000000,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::BankMovement,
        }),
    )
    .expect("persist transfer outflow candidate");
    persist_pdf_import(
        &mut transfer_connection,
        transfer_inspect_response(TransferCandidateFixture {
            hash: "9999999999999999999999999999999999999999999999999999999999999999",
            institution: "nequi",
            account_label: "Nequi wallet",
            candidate_id: "cand_nequi_own_transfer_in",
            description: "TRANSFERENCIA DESDE CUENTA PROPIA REDACTED",
            amount_minor: 7000000,
            direction_hint: DirectionHint::Inflow,
            semantic_hint: SemanticHint::BankMovement,
        }),
    )
    .expect("persist transfer inflow candidate");
    drop(transfer_connection);
    create_income_source_cli(transfer_db, "Redacted Transfer Source");

    let transfer_like = Command::new(tracky())
        .args([
            "candidates",
            "accept-income",
            "cand_nequi_own_transfer_in",
            "--db",
            transfer_db,
            "--income-source-id",
            "incsrc_redacted_transfer_source",
            "--income-kind",
            "other",
            "--json",
        ])
        .output()
        .expect("run transfer-like accept income");
    assert!(!transfer_like.status.success());
    let transfer_like_json: serde_json::Value =
        serde_json::from_slice(&transfer_like.stdout).expect("transfer-like income json");
    assert_eq!(
        transfer_like_json["errors"][0]["code"],
        "candidate_possible_own_account_transfer"
    );
}
