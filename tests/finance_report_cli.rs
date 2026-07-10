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

fn tracky() -> &'static str {
    env!("CARGO_BIN_EXE_tracky")
}

fn run(args: &[&str]) -> (bool, serde_json::Value) {
    let output = Command::new(tracky())
        .args(args)
        .output()
        .expect("run tracky");
    let json = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("stderr: {}", String::from_utf8_lossy(&output.stderr)));
    (output.status.success(), json)
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
    .unwrap()
    .account
    .unwrap()
    .id
}

struct CandidateFixture<'a> {
    hash: &'a str,
    institution: &'a str,
    account_label: &'a str,
    candidate_id: &'a str,
    posted_date: &'a str,
    description: &'a str,
    amount_minor: i64,
    direction_hint: DirectionHint,
    semantic_hint: SemanticHint,
}

fn candidate_response(fixture: CandidateFixture<'_>) -> PdfInspectResponse {
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
        posted_date: fixture.posted_date.to_string(),
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
                text: format!("{} {} <amount>", fixture.posted_date, fixture.description),
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

fn persist_candidate(connection: &mut Connection, fixture: CandidateFixture<'_>) {
    persist_pdf_import(connection, candidate_response(fixture)).unwrap();
}

#[test]
fn report_summarizes_only_canonical_activity_without_double_counting_splits_or_transfers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tracky.sqlite");
    let db = path.to_str().unwrap();
    let mut connection = Connection::open(&path).unwrap();
    apply_migrations(&connection).unwrap();
    let wallet_id = register_account(&connection, "nequi", "Synthetic wallet", "wallet");
    let card_id = register_account(&connection, "rappi", "Synthetic card", "credit_card");

    let fixtures = [
        CandidateFixture {
            hash: "1111111111111111111111111111111111111111111111111111111111111111",
            institution: "nequi",
            account_label: "Synthetic wallet",
            candidate_id: "cand_report_income",
            posted_date: "2026-06-03",
            description: "SYNTHETIC REDACTED INCOME",
            amount_minor: 500_000,
            direction_hint: DirectionHint::Inflow,
            semantic_hint: SemanticHint::BankMovement,
        },
        CandidateFixture {
            hash: "2222222222222222222222222222222222222222222222222222222222222222",
            institution: "nequi",
            account_label: "Synthetic wallet",
            candidate_id: "cand_report_expense",
            posted_date: "2026-06-04",
            description: "SYNTHETIC REDACTED EXPENSE",
            amount_minor: -120_000,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::BankMovement,
        },
        CandidateFixture {
            hash: "3333333333333333333333333333333333333333333333333333333333333333",
            institution: "nequi",
            account_label: "Synthetic wallet",
            candidate_id: "cand_report_split",
            posted_date: "2026-06-05",
            description: "SYNTHETIC REDACTED SPLIT",
            amount_minor: -80_000,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::BankMovement,
        },
        CandidateFixture {
            hash: "4444444444444444444444444444444444444444444444444444444444444444",
            institution: "nequi",
            account_label: "Synthetic wallet",
            candidate_id: "cand_report_pending",
            posted_date: "2026-06-06",
            description: "SYNTHETIC PENDING",
            amount_minor: 9_999_999,
            direction_hint: DirectionHint::Inflow,
            semantic_hint: SemanticHint::BankMovement,
        },
        CandidateFixture {
            hash: "5555555555555555555555555555555555555555555555555555555555555555",
            institution: "nequi",
            account_label: "Synthetic wallet",
            candidate_id: "cand_report_rejected",
            posted_date: "2026-06-07",
            description: "SYNTHETIC REJECTED",
            amount_minor: -8_888_888,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::BankMovement,
        },
        CandidateFixture {
            hash: "6666666666666666666666666666666666666666666666666666666666666666",
            institution: "nequi",
            account_label: "Synthetic wallet",
            candidate_id: "cand_report_transfer_from",
            posted_date: "2026-06-08",
            description: "SYNTHETIC CARD PAYMENT",
            amount_minor: -70_000,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::BankMovement,
        },
        CandidateFixture {
            hash: "7777777777777777777777777777777777777777777777777777777777777777",
            institution: "rappi",
            account_label: "Synthetic card",
            candidate_id: "cand_report_transfer_to",
            posted_date: "2026-06-08",
            description: "SYNTHETIC CARD PAYMENT",
            amount_minor: -70_000,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::CardPayment,
        },
    ];
    for fixture in fixtures {
        persist_candidate(&mut connection, fixture);
    }
    drop(connection);

    let (_, income_source) = run(&[
        "income-sources",
        "create",
        "--db",
        db,
        "--name",
        "Synthetic employer",
        "--json",
    ]);
    let source_id = income_source["income_source"]["id"].as_str().unwrap();
    let (_, food_category) = run(&[
        "categories",
        "create",
        "--db",
        db,
        "--name",
        "Synthetic food",
        "--json",
    ]);
    let food_id = food_category["category"]["id"].as_str().unwrap();
    let (_, home_category) = run(&[
        "categories",
        "create",
        "--db",
        db,
        "--name",
        "Synthetic home",
        "--json",
    ]);
    let home_id = home_category["category"]["id"].as_str().unwrap();

    assert!(
        run(&[
            "candidates",
            "accept-income",
            "cand_report_income",
            "--db",
            db,
            "--income-source-id",
            source_id,
            "--income-kind",
            "salary",
            "--json",
        ])
        .0
    );
    assert!(
        run(&[
            "candidates",
            "accept-expense",
            "cand_report_expense",
            "--db",
            db,
            "--category-id",
            food_id,
            "--json",
        ])
        .0
    );
    assert!(
        run(&[
            "candidates",
            "accept-expense",
            "cand_report_split",
            "--db",
            db,
            "--line",
            &format!("{food_id}:-50000:COP"),
            "--line",
            &format!("{home_id}:-30000:COP"),
            "--json",
        ])
        .0
    );
    assert!(
        run(&[
            "candidates",
            "reject",
            "cand_report_rejected",
            "--db",
            db,
            "--json",
        ])
        .0
    );
    assert!(
        run(&[
            "candidates",
            "accept-transfer-pair",
            "cand_report_transfer_from",
            "cand_report_transfer_to",
            "--db",
            db,
            "--json",
        ])
        .0
    );
    assert!(
        run(&[
            "transactions",
            "add-transfer",
            "--db",
            db,
            "--from-account-id",
            &wallet_id,
            "--to-account-id",
            &card_id,
            "--posted-date",
            "2026-06-09",
            "--description",
            "SYNTHETIC OWN TRANSFER",
            "--amount-minor",
            "120000",
            "--currency",
            "COP",
            "--json",
        ])
        .0
    );
    assert!(
        run(&[
            "transactions",
            "add-income",
            "--db",
            db,
            "--account-id",
            &wallet_id,
            "--posted-date",
            "2026-07-01",
            "--description",
            "SYNTHETIC OUTSIDE RANGE",
            "--amount-minor",
            "123456",
            "--currency",
            "COP",
            "--income-source-id",
            source_id,
            "--income-kind",
            "other",
            "--json",
        ])
        .0
    );

    let (success, report) = run(&[
        "reports",
        "summary",
        "--db",
        db,
        "--start-date",
        "2026-06-01",
        "--end-date",
        "2026-06-30",
        "--json",
    ]);
    assert!(success);
    assert_eq!(report["schema_version"], "tracky.finance-report.v1");
    assert_eq!(report["command"], "reports summary");
    assert_eq!(report["date_range"]["start_date"], "2026-06-01");
    assert_eq!(report["date_range"]["end_date"], "2026-06-30");
    assert_eq!(report["totals"][0]["currency"], "COP");
    assert_eq!(report["totals"][0]["total_income_minor"], 500_000);
    assert_eq!(report["totals"][0]["total_expenses_minor"], 200_000);
    assert_eq!(report["totals"][0]["net_cash_flow_minor"], 300_000);
    assert_eq!(
        report["totals"][0]["excluded_transfer_total_minor"],
        190_000
    );
    assert_eq!(report["totals"][0]["excluded_transfer_count"], 2);
    assert_eq!(report["category_totals"].as_array().unwrap().len(), 2);
    assert_eq!(report["category_totals"][0]["category_id"], food_id);
    assert_eq!(
        report["category_totals"][0]["total_expenses_minor"],
        170_000
    );
    assert_eq!(report["category_totals"][1]["category_id"], home_id);
    assert_eq!(report["category_totals"][1]["total_expenses_minor"], 30_000);
    assert_eq!(
        report["income_source_totals"][0]["income_source_id"],
        source_id
    );
    assert_eq!(
        report["income_source_totals"][0]["total_income_minor"],
        500_000
    );
    assert_eq!(
        report["excluded_transfer_totals"][0]["transfer_kind"],
        "card_payment"
    );
    assert_eq!(
        report["excluded_transfer_totals"][0]["total_amount_minor"],
        70_000
    );
    assert_eq!(
        report["excluded_transfer_totals"][1]["transfer_kind"],
        "own_account_transfer"
    );
    assert_eq!(
        report["excluded_transfer_totals"][1]["total_amount_minor"],
        120_000
    );

    let connection = Connection::open(&path).unwrap();
    let statuses: (String, String) = connection
        .query_row(
            "SELECT
                (SELECT status FROM candidate_transactions WHERE id = 'cand_report_pending'),
                (SELECT status FROM candidate_transactions WHERE id = 'cand_report_rejected')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(statuses, ("pending_review".into(), "rejected".into()));
}

#[test]
fn report_requires_json_and_valid_inclusive_date_range() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tracky.sqlite");
    let db = path.to_str().unwrap();

    let (success, missing_json) = run(&[
        "reports",
        "summary",
        "--db",
        db,
        "--start-date",
        "2026-06-01",
        "--end-date",
        "2026-06-30",
    ]);
    assert!(!success);
    assert_eq!(missing_json["schema_version"], "tracky.finance-report.v1");
    assert_eq!(missing_json["errors"][0]["code"], "json_output_required");

    let (success, invalid_date) = run(&[
        "reports",
        "summary",
        "--db",
        db,
        "--start-date",
        "2026-02-30",
        "--end-date",
        "2026-06-30",
        "--json",
    ]);
    assert!(!success);
    assert_eq!(invalid_date["errors"][0]["code"], "invalid_start_date");

    let (success, reversed) = run(&[
        "reports",
        "summary",
        "--db",
        db,
        "--start-date",
        "2026-07-01",
        "--end-date",
        "2026-06-30",
        "--json",
    ]);
    assert!(!success);
    assert_eq!(reversed["errors"][0]["code"], "invalid_date_range");
}
