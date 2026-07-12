use rusqlite::Connection;
use std::fs;
use std::process::{Command, Output};
use tracky::pdf::{
    AccountHint, CandidateStatus, CandidateTransaction, CredentialSource, DirectionHint,
    DocumentDuplicateState, DocumentDuplicateStatus, DuplicateStatus, DuplicateStatusState,
    Evidence, ExtractorRef, ExtractorState, ExtractorStatus, ParserRef, ParserState, ParserStatus,
    PdfInspectResponse, Provenance, SemanticHint, SourceDocument, TrackyError,
    PDF_INSPECT_SCHEMA_VERSION,
};
use tracky::storage::{
    accept_expense_candidate, apply_migrations, create_category, create_income_source,
    persist_pdf_import, register_owned_account, reject_candidate, AccountRegisterInput,
    CategoryCreateInput, ExpenseLineInput, IncomeSourceCreateInput,
};

const HASH: &str = "1919191919191919191919191919191919191919191919191919191919191919";

fn tracky() -> &'static str {
    env!("CARGO_BIN_EXE_tracky")
}

struct CandidateSpec<'a> {
    id: &'a str,
    fingerprint: &'a str,
    institution: &'a str,
    account_label: &'a str,
    amount_minor: i64,
    direction_hint: DirectionHint,
    semantic_hint: SemanticHint,
    row_index: u32,
    posted_date: &'a str,
}

fn candidate(spec: CandidateSpec<'_>) -> CandidateTransaction {
    let CandidateSpec {
        id,
        fingerprint,
        institution,
        account_label,
        amount_minor,
        direction_hint,
        semantic_hint,
        row_index,
        posted_date,
    } = spec;
    let source_document_id = format!("srcdoc_{}", &HASH[..26]);
    CandidateTransaction {
        id: id.to_string(),
        import_batch_id: None,
        source_document_id: source_document_id.clone(),
        status: CandidateStatus::PendingReview,
        duplicate_status: DuplicateStatus {
            status: DuplicateStatusState::NotChecked,
            fingerprint: fingerprint.to_string(),
            matched_candidate_ids: Vec::new(),
            matched_canonical_transaction_ids: Vec::new(),
            reason: None,
        },
        institution_hint: institution.to_string(),
        account_hint: AccountHint {
            label: account_label.to_string(),
            currency: "COP",
            masked_identifier: None,
        },
        posted_date: posted_date.to_string(),
        description: format!("Synthetic redacted movement {id}"),
        amount_minor,
        currency: "COP",
        balance_minor: None,
        direction_hint,
        semantic_hint,
        confidence: 0.93,
        provenance: Provenance {
            source_document_id,
            page_number: 1,
            row_index: row_index as usize,
            bbox: None,
            extractor: ExtractorRef {
                name: "synthetic_extractor",
                version: Some("1"),
            },
            parser: ParserRef {
                id: "synthetic.statement.v1".to_string(),
                version: "1",
            },
            evidence: Evidence {
                redaction: "redacted",
                text: format!("2026-06-15 REDACTED_{row_index} <amount>"),
                raw_storage_policy: "redacted_only",
            },
            confidence: 0.93,
        },
        validation_warnings: Vec::new(),
        account_resolution: None,
    }
}

fn inspect_response() -> PdfInspectResponse {
    let source_document = SourceDocument {
        id: format!("srcdoc_{}", &HASH[..26]),
        input_name: "synthetic-redacted-batch.pdf".to_string(),
        content_sha256: HASH.to_string(),
        mime_type: "application/pdf",
        byte_size: 64,
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
    let candidates = vec![
        candidate(CandidateSpec {
            id: "cand_dup_a",
            fingerprint: "fp_exact_reviewed",
            institution: "nequi",
            account_label: "Nequi wallet",
            amount_minor: -5000,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::BankMovement,
            row_index: 1,
            posted_date: "2026-03-31",
        }),
        candidate(CandidateSpec {
            id: "cand_dup_b",
            fingerprint: "fp_exact_reviewed",
            institution: "nequi",
            account_label: "Nequi wallet",
            amount_minor: -5000,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::BankMovement,
            row_index: 2,
            posted_date: "2026-03-31",
        }),
        candidate(CandidateSpec {
            id: "cand_transfer_from",
            fingerprint: "fp_transfer_from",
            institution: "nequi",
            account_label: "Nequi wallet",
            amount_minor: -4000,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::BankMovement,
            row_index: 3,
            posted_date: "2026-06-15",
        }),
        candidate(CandidateSpec {
            id: "cand_transfer_to",
            fingerprint: "fp_transfer_to",
            institution: "rappi",
            account_label: "RappiCard",
            amount_minor: 4000,
            direction_hint: DirectionHint::Inflow,
            semantic_hint: SemanticHint::CardPayment,
            row_index: 4,
            posted_date: "2026-06-15",
        }),
        candidate(CandidateSpec {
            id: "cand_accepted",
            fingerprint: "fp_accepted",
            institution: "nequi",
            account_label: "Nequi wallet",
            amount_minor: -300,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::CardCharge,
            row_index: 5,
            posted_date: "2026-06-15",
        }),
        candidate(CandidateSpec {
            id: "cand_rejected",
            fingerprint: "fp_rejected",
            institution: "nequi",
            account_label: "Nequi wallet",
            amount_minor: -200,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::CardPayment,
            row_index: 6,
            posted_date: "2026-06-15",
        }),
        candidate(CandidateSpec {
            id: "cand_unresolved",
            fingerprint: "fp_unresolved",
            institution: "synthetic-bank",
            account_label: "Unresolved account",
            amount_minor: 100,
            direction_hint: DirectionHint::Inflow,
            semantic_hint: SemanticHint::BankMovement,
            row_index: 7,
            posted_date: "2026-06-15",
        }),
        candidate(CandidateSpec {
            id: "cand_batch_income",
            fingerprint: "fp_batch_income",
            institution: "nequi",
            account_label: "Nequi wallet",
            amount_minor: 700,
            direction_hint: DirectionHint::Inflow,
            semantic_hint: SemanticHint::BankMovement,
            row_index: 8,
            posted_date: "2026-04-01",
        }),
        candidate(CandidateSpec {
            id: "cand_batch_expense",
            fingerprint: "fp_batch_expense",
            institution: "nequi",
            account_label: "Nequi wallet",
            amount_minor: -900,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::CardCharge,
            row_index: 9,
            posted_date: "2026-06-30",
        }),
    ];
    PdfInspectResponse {
        schema_version: PDF_INSPECT_SCHEMA_VERSION,
        command: "pdf inspect",
        ok: true,
        source_document,
        extractor_status: ExtractorStatus {
            status: ExtractorState::Succeeded,
            extractor: "synthetic_extractor",
            pages_seen: 1,
            pages_extracted: 1,
            requires_document_credential: false,
            credential_source: CredentialSource::None,
            warnings: Vec::new(),
        },
        parser_status: ParserStatus {
            status: ParserState::Succeeded,
            parser_id: "synthetic.statement.v1".to_string(),
            parser_version: "1",
            candidates_found: candidates.len(),
            candidates_valid: candidates.len(),
            warnings: Vec::new(),
        },
        candidates,
        errors: Vec::<TrackyError>::new(),
    }
}

fn register_account(connection: &Connection, institution: &str, label: &str, kind: &str) -> String {
    register_owned_account(
        connection,
        AccountRegisterInput {
            institution: institution.to_string(),
            label: label.to_string(),
            account_type: kind.to_string(),
            currency: "COP".to_string(),
            masked_identifier: None,
        },
    )
    .expect("register synthetic account")
    .account
    .expect("registered account")
    .id
}

struct Fixture {
    _dir: tempfile::TempDir,
    db_path: std::path::PathBuf,
    batch_id: String,
    category_id: String,
    second_category_id: String,
    income_source_id: String,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let mut connection = Connection::open(&db_path).expect("open db");
    apply_migrations(&connection).expect("apply migrations");
    let nequi_id = register_account(&connection, "nequi", "Nequi wallet", "wallet");
    register_account(&connection, "rappi", "RappiCard", "credit_card");
    connection
        .execute(
            "INSERT INTO canonical_transactions
             (id, account_id, posted_date, description, amount_minor, currency)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                "txn_existing_duplicate",
                nequi_id,
                "2026-06-15",
                "Synthetic redacted canonical match",
                -5000_i64,
                "COP"
            ],
        )
        .expect("seed canonical duplicate");
    connection
        .execute(
            "INSERT INTO transaction_fingerprints
             (id, fingerprint, canonical_transaction_id, duplicate_status,
              normalized_account_key, normalized_posted_date, normalized_amount_minor,
              normalized_currency, normalized_description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                "fprow_existing_duplicate",
                "fp_exact_reviewed",
                "txn_existing_duplicate",
                "unique",
                "nequi|nequi wallet",
                "2026-06-15",
                -5000_i64,
                "COP",
                "synthetic redacted canonical match"
            ],
        )
        .expect("seed canonical fingerprint");
    let imported = persist_pdf_import(&mut connection, inspect_response()).expect("persist batch");
    let batch_id = imported.import_batch.expect("batch").id;
    let category = create_category(
        &connection,
        CategoryCreateInput {
            name: "Synthetic review category".to_string(),
        },
    )
    .expect("create fixture category")
    .category
    .expect("fixture category")
    .id;
    let second_category_id = create_category(
        &connection,
        CategoryCreateInput {
            name: "Synthetic second category".to_string(),
        },
    )
    .unwrap()
    .category
    .unwrap()
    .id;
    let income_source_id = create_income_source(
        &connection,
        IncomeSourceCreateInput {
            name: "Synthetic source".to_string(),
        },
    )
    .unwrap()
    .income_source
    .unwrap()
    .id;
    assert!(
        accept_expense_candidate(
            &mut connection,
            "cand_accepted",
            &[ExpenseLineInput {
                category_id: category.clone(),
                amount_minor: -300,
                currency: "COP".to_string(),
            }],
        )
        .expect("accept fixture expense")
        .ok
    );
    assert!(
        reject_candidate(&mut connection, "cand_rejected")
            .expect("reject fixture candidate")
            .ok
    );
    drop(connection);
    Fixture {
        _dir: dir,
        db_path,
        batch_id,
        category_id: category,
        second_category_id,
        income_source_id,
    }
}

fn add_cross_batch_transfer_candidate(fixture: &Fixture) -> String {
    let mut connection = Connection::open(&fixture.db_path).expect("reopen fixture db");
    let mut response = inspect_response();
    response.source_document.id = "srcdoc_29292929292929292929292929".to_string();
    response.source_document.content_sha256 =
        "2929292929292929292929292929292929292929292929292929292929292929".to_string();
    response.candidates = response
        .candidates
        .into_iter()
        .filter(|candidate| candidate.id == "cand_transfer_to")
        .map(|mut candidate| {
            candidate.id = "cand_cross_batch_to".to_string();
            candidate.source_document_id = response.source_document.id.clone();
            candidate.provenance.source_document_id = response.source_document.id.clone();
            candidate
        })
        .collect();
    persist_pdf_import(&mut connection, response)
        .expect("persist cross-batch fixture")
        .import_batch
        .expect("cross-batch import batch")
        .id
}

fn run(args: &[&str]) -> Output {
    Command::new(tracky())
        .args(args)
        .output()
        .expect("run tracky")
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "invalid JSON: {error}; stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn group_count(value: &serde_json::Value, path: &str, key: &str) -> u64 {
    value["summary"][path]
        .as_array()
        .expect("group array")
        .iter()
        .find(|entry| entry["key"] == key)
        .unwrap_or_else(|| panic!("missing group {path}:{key}"))["count"]
        .as_u64()
        .expect("count")
}

#[test]
fn candidate_listing_uses_inclusive_posted_date_boundaries() {
    let fixture = fixture();
    let output = run(&[
        "candidates",
        "list",
        "--db",
        fixture.db_path.to_str().unwrap(),
        "--from",
        "2026-04-01",
        "--to",
        "2026-06-30",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let body = json(&output);
    let ids = body["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|candidate| candidate["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(ids.contains(&"cand_batch_income"));
    assert!(ids.contains(&"cand_batch_expense"));
    assert!(!ids.contains(&"cand_dup_a"));

    let suggestions = run(&[
        "candidates",
        "suggest-actions",
        "--db",
        fixture.db_path.to_str().unwrap(),
        "--import-batch-id",
        &fixture.batch_id,
        "--from",
        "2026-04-01",
        "--to",
        "2026-06-30",
        "--json",
    ]);
    assert!(suggestions.status.success());
    assert!(json(&suggestions)["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|suggestion| suggestion["candidate_ids"]
            .as_array()
            .unwrap()
            .iter()
            .all(|id| id != "cand_dup_a" && id != "cand_dup_b")));

    for (from, to, code) in [
        ("not-a-date", "2026-06-30", "invalid_from_date"),
        ("2026-06-30", "2026-04-01", "invalid_date_range"),
    ] {
        let invalid = run(&[
            "candidates",
            "list",
            "--db",
            fixture.db_path.to_str().unwrap(),
            "--from",
            from,
            "--to",
            to,
            "--json",
        ]);
        assert!(!invalid.status.success());
        assert_eq!(json(&invalid)["errors"][0]["code"], code);
    }
}

#[test]
fn date_scoped_dry_run_plans_explicit_ids_and_apply_requires_the_approved_plan() {
    let fixture = fixture();
    let db = fixture.db_path.to_str().unwrap();
    let action = format!(
        "accept-income:cand_batch_income:{}:salary",
        fixture.income_source_id
    );
    let before: (i64, i64, i64) = Connection::open(&fixture.db_path).unwrap().query_row(
        "SELECT (SELECT COUNT(*) FROM candidate_transactions), (SELECT COUNT(*) FROM source_documents), (SELECT COUNT(*) FROM provenance)",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).unwrap();
    let dry = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        &action,
        "--from",
        "2026-04-01",
        "--to",
        "2026-04-01",
        "--dry-run",
        "--json",
    ]);
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stdout)
    );
    let plan = json(&dry);
    assert_eq!(
        plan["date_scope"]["candidate_ids"],
        serde_json::json!(["cand_batch_income"])
    );
    assert_eq!(
        plan["date_scope"]["selected_by_status_and_month"][0],
        serde_json::json!({"status":"pending_review","month":"2026-04","count":1})
    );
    assert!(plan["date_scope"]["excluded_by_status_and_month"]
        .as_array()
        .unwrap()
        .iter()
        .any(|group| group
            == &serde_json::json!({"status":"possible_duplicate","month":"2026-03","count":2})));
    let plan_id = plan["date_scope"]["plan_id"].as_str().unwrap();

    let missing = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        &action,
        "--from",
        "2026-04-01",
        "--to",
        "2026-04-01",
        "--json",
    ]);
    assert_eq!(json(&missing)["errors"][0]["code"], "plan_id_required");

    let applied = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        &action,
        "--from",
        "2026-04-01",
        "--to",
        "2026-04-01",
        "--plan-id",
        plan_id,
        "--json",
    ]);
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stdout)
    );
    let after: (i64, i64, i64) = Connection::open(&fixture.db_path).unwrap().query_row(
        "SELECT (SELECT COUNT(*) FROM candidate_transactions), (SELECT COUNT(*) FROM source_documents), (SELECT COUNT(*) FROM provenance)",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).unwrap();
    assert_eq!(after.0, before.0);
    assert_eq!(after.1, before.1);
    assert_eq!(after.2, before.2);
    assert_eq!(
        Connection::open(&fixture.db_path)
            .unwrap()
            .query_row(
                "SELECT status FROM candidate_transactions WHERE id='cand_dup_a'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "possible_duplicate"
    );
}

#[test]
fn date_scoped_apply_rejects_stale_plans_atomically() {
    let fixture = fixture();
    let db = fixture.db_path.to_str().unwrap();
    let action = format!(
        "accept-income:cand_batch_income:{}:salary",
        fixture.income_source_id
    );
    let dry = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        &action,
        "--from",
        "2026-04-01",
        "--to",
        "2026-04-01",
        "--dry-run",
        "--json",
    ]);
    let plan_id = json(&dry)["date_scope"]["plan_id"]
        .as_str()
        .unwrap()
        .to_string();
    let reassigned_account = Connection::open(&fixture.db_path)
        .unwrap()
        .query_row(
            "SELECT a.id FROM accounts a JOIN institutions i ON i.id=a.institution_id WHERE i.name='rappi'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap();
    assert!(run(&[
        "candidates",
        "assign-account",
        "cand_batch_income",
        "--db",
        db,
        "--account-id",
        &reassigned_account,
        "--json"
    ])
    .status
    .success());
    let stale = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        &action,
        "--from",
        "2026-04-01",
        "--to",
        "2026-04-01",
        "--plan-id",
        &plan_id,
        "--json",
    ]);
    assert_eq!(json(&stale)["errors"][0]["code"], "stale_review_plan");
    assert_eq!(Connection::open(&fixture.db_path).unwrap().query_row(
        "SELECT COUNT(*) FROM canonical_transactions WHERE created_from_candidate_id='cand_batch_income'",
        [], |row| row.get::<_, i64>(0),
    ).unwrap(), 0);
}

#[test]
fn batch_summary_compare_and_suggest_are_complete_deterministic_and_read_only() {
    let fixture = fixture();
    let db = fixture.db_path.to_str().unwrap();
    let before = fs::read(&fixture.db_path).expect("read db before");

    let summary = run(&[
        "candidates",
        "batch-summary",
        "--db",
        db,
        "--import-batch-id",
        &fixture.batch_id,
        "--largest-limit",
        "3",
        "--json",
    ]);
    assert!(summary.status.success());
    let summary = json(&summary);
    assert_eq!(summary["schema_version"], "tracky.batch-review.v1");
    assert_eq!(summary["summary"]["total_candidates"], 9);
    assert_eq!(group_count(&summary, "by_status", "accepted"), 1);
    assert_eq!(group_count(&summary, "by_status", "possible_duplicate"), 2);
    assert_eq!(group_count(&summary, "by_status", "pending_review"), 5);
    assert_eq!(group_count(&summary, "by_status", "rejected"), 1);
    assert_eq!(
        group_count(&summary, "by_duplicate_status", "exact_duplicate"),
        2
    );
    assert_eq!(group_count(&summary, "by_institution", "nequi"), 7);
    assert_eq!(group_count(&summary, "by_institution", "rappi"), 1);
    assert_eq!(
        group_count(&summary, "by_account_resolution", "resolved"),
        8
    );
    assert_eq!(
        group_count(&summary, "by_account_resolution", "unresolved"),
        1
    );
    assert_eq!(group_count(&summary, "by_direction_hint", "outflow"), 6);
    assert_eq!(
        group_count(&summary, "by_semantic_hint", "bank_movement"),
        5
    );
    let largest = summary["summary"]["largest_amounts"].as_array().unwrap();
    assert_eq!(largest[0]["candidate_id"], "cand_dup_a");
    assert_eq!(largest[1]["candidate_id"], "cand_dup_b");
    assert_eq!(largest[2]["candidate_id"], "cand_transfer_from");

    let comparison = run(&[
        "candidates",
        "compare-duplicate",
        "cand_dup_b",
        "--db",
        db,
        "--json",
    ]);
    assert!(comparison.status.success());
    let comparison = json(&comparison);
    assert_eq!(comparison["comparison"]["candidate"]["id"], "cand_dup_b");
    assert_eq!(
        comparison["comparison"]["matched_candidates"][0]["id"],
        "cand_dup_a"
    );
    assert_eq!(
        comparison["comparison"]["matched_canonical_transactions"][0]["transaction"]["id"],
        "txn_existing_duplicate"
    );
    assert_eq!(
        comparison["comparison"]["candidate"]["provenance"]["evidence_redaction"],
        "redacted"
    );
    assert!(!comparison["comparison"]["fingerprints"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(!comparison["comparison"]["duplicate_markers"]
        .as_array()
        .unwrap()
        .is_empty());

    let suggestions = run(&[
        "candidates",
        "suggest-actions",
        "--db",
        db,
        "--import-batch-id",
        &fixture.batch_id,
        "--json",
    ]);
    assert!(suggestions.status.success());
    let suggestions = json(&suggestions);
    let actions = suggestions["suggestions"].as_array().unwrap();
    assert_eq!(actions.len(), 3);
    assert_eq!(
        actions
            .iter()
            .filter(|suggestion| suggestion["proposed_action"] == "reject_duplicate")
            .count(),
        2
    );
    assert!(actions.iter().any(|suggestion| {
        suggestion["proposed_action"] == "accept_transfer_pair"
            && suggestion["candidate_ids"][0] == "cand_transfer_from"
            && suggestion["candidate_ids"][1] == "cand_transfer_to"
    }));
    assert_eq!(
        fs::read(&fixture.db_path).expect("read db after"),
        before,
        "read-only commands must not change SQLite"
    );
}

#[test]
fn suggest_actions_includes_unique_cross_batch_transfer_with_both_batches() {
    let fixture = fixture();
    let other_batch_id = add_cross_batch_transfer_candidate(&fixture);
    let db = fixture.db_path.to_str().unwrap();
    let before = fs::read(&fixture.db_path).expect("read db before cross-batch suggestion");

    let suggestions = run(&[
        "candidates",
        "suggest-actions",
        "--db",
        db,
        "--import-batch-id",
        &fixture.batch_id,
        "--json",
    ]);
    assert!(suggestions.status.success());
    let suggestions = json(&suggestions);
    let transfers = suggestions["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|suggestion| suggestion["proposed_action"] == "accept_transfer_pair")
        .collect::<Vec<_>>();
    assert_eq!(transfers.len(), 2);
    let cross_batch = transfers
        .iter()
        .find(|suggestion| {
            suggestion["candidate_ids"]
                == serde_json::json!(["cand_transfer_from", "cand_cross_batch_to"])
        })
        .expect("cross-batch transfer suggestion");
    assert_eq!(
        cross_batch["import_batch_ids"],
        serde_json::json!([fixture.batch_id, other_batch_id])
    );
    assert_eq!(cross_batch["evidence"]["posted_date"], "2026-06-15");
    assert_eq!(cross_batch["evidence"]["amount_minor"], 4000);
    assert_eq!(
        cross_batch["id"],
        "suggest_0eab60ae7590bad915dc4727779fd147"
    );
    assert_eq!(
        transfers
            .iter()
            .filter(|suggestion| suggestion["candidate_ids"]
                == serde_json::json!(["cand_transfer_from", "cand_cross_batch_to"]))
            .count(),
        1
    );
    assert_eq!(fs::read(&fixture.db_path).unwrap(), before);
}

#[test]
fn dry_run_does_not_write_and_explicit_apply_is_atomic_and_auditable() {
    let fixture = fixture();
    let db = fixture.db_path.to_str().unwrap();
    let before = fs::read(&fixture.db_path).expect("db before dry run");
    let reject_action = "reject-duplicate:cand_dup_a";
    let transfer_action = "accept-transfer-pair:cand_transfer_from:cand_transfer_to";

    let dry_run = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        reject_action,
        "--action",
        transfer_action,
        "--dry-run",
        "--json",
    ]);
    assert!(dry_run.status.success());
    let dry_run_json = json(&dry_run);
    assert_eq!(dry_run_json["dry_run"], true);
    assert!(dry_run_json["action_results"]
        .as_array()
        .unwrap()
        .iter()
        .all(|result| result["status"] == "validated"));
    assert_eq!(fs::read(&fixture.db_path).unwrap(), before);

    let apply = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        reject_action,
        "--action",
        transfer_action,
        "--json",
    ]);
    assert!(
        apply.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&apply.stderr)
    );
    let apply_json = json(&apply);
    assert_eq!(apply_json["dry_run"], false);
    assert!(apply_json["action_results"]
        .as_array()
        .unwrap()
        .iter()
        .all(|result| result["status"] == "applied"));
    assert_eq!(
        apply_json["action_results"][1]["canonical_transaction_ids"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let connection = Connection::open(&fixture.db_path).expect("reopen db");
    let state: (String, String, String, i64, i64, i64) = connection
        .query_row(
            "SELECT
                (SELECT status FROM candidate_transactions WHERE id = 'cand_dup_a'),
                (SELECT status FROM candidate_transactions WHERE id = 'cand_transfer_from'),
                (SELECT status FROM candidate_transactions WHERE id = 'cand_transfer_to'),
                (SELECT COUNT(*) FROM canonical_transfer_pairs),
                (SELECT COUNT(*) FROM canonical_transactions WHERE transaction_kind = 'own_account_transfer'),
                (SELECT COUNT(*) FROM provenance WHERE candidate_transaction_id IN ('cand_dup_a', 'cand_transfer_from', 'cand_transfer_to'))",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )
        .expect("batch audit state");
    assert_eq!(
        state,
        (
            "rejected".to_string(),
            "accepted".to_string(),
            "accepted".to_string(),
            1,
            2,
            3,
        )
    );
}

#[test]
fn mixed_typed_actions_dry_run_and_apply_are_deterministic_atomic_and_refuse_replay() {
    let fixture = fixture();
    let db = fixture.db_path.to_str().unwrap();
    let income = format!(
        "accept-income:cand_batch_income:{}:salary",
        fixture.income_source_id
    );
    let expense = format!(
        "accept-expense:cand_batch_expense:{}:-400:COP:{}:-500:COP",
        fixture.category_id, fixture.second_category_id
    );
    let transfer = "accept-transfer-pair:cand_transfer_from:cand_transfer_to";
    let before = fs::read(&fixture.db_path).unwrap();

    let dry_run = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        &expense,
        "--action",
        transfer,
        "--action",
        &income,
        "--dry-run",
        "--json",
    ]);
    assert!(dry_run.status.success());
    let body = json(&dry_run);
    assert_eq!(
        body["action_results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["action"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["accept_expense", "accept_transfer_pair", "accept_income"]
    );
    assert_eq!(fs::read(&fixture.db_path).unwrap(), before);

    let apply = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        &expense,
        "--action",
        transfer,
        "--action",
        &income,
        "--json",
    ]);
    assert!(
        apply.status.success(),
        "{}",
        String::from_utf8_lossy(&apply.stdout)
    );
    let connection = Connection::open(&fixture.db_path).unwrap();
    let state: (i64, i64, i64, i64) = connection.query_row("SELECT (SELECT COUNT(*) FROM canonical_transactions WHERE transaction_kind='income' AND income_source_id=?1 AND income_kind='salary'), (SELECT COUNT(*) FROM canonical_transactions WHERE transaction_kind='expense' AND created_from_candidate_id='cand_batch_expense'), (SELECT COUNT(*) FROM transaction_lines l JOIN canonical_transactions t ON t.id=l.canonical_transaction_id WHERE t.created_from_candidate_id='cand_batch_expense'), (SELECT COUNT(*) FROM canonical_transfer_pairs)", [&fixture.income_source_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    assert_eq!(state, (1, 1, 2, 1));
    drop(connection);

    let replay = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        &income,
        "--json",
    ]);
    assert!(!replay.status.success());
    assert_eq!(
        json(&replay)["action_results"][0]["errors"][0]["code"],
        "candidate_already_accepted"
    );
}

#[test]
fn typed_batch_preflight_rejects_invalid_reference_reuse_and_rolls_back_every_action() {
    let fixture = fixture();
    let db = fixture.db_path.to_str().unwrap();
    let income = format!(
        "accept-income:cand_batch_income:{}:salary",
        fixture.income_source_id
    );
    let invalid_expense = "accept-expense:cand_batch_expense:category_missing:-900:COP";
    let failed = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        &income,
        "--action",
        invalid_expense,
        "--json",
    ]);
    assert!(!failed.status.success());
    assert_eq!(
        json(&failed)["action_results"][1]["errors"][0]["code"],
        "category_not_found"
    );
    let connection = Connection::open(&fixture.db_path).unwrap();
    assert_eq!(connection.query_row("SELECT COUNT(*) FROM canonical_transactions WHERE created_from_candidate_id IN ('cand_batch_income','cand_batch_expense')", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    drop(connection);

    let reused = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        &income,
        "--action",
        "accept-expense:cand_batch_income:category_missing:700:COP",
        "--dry-run",
        "--json",
    ]);
    assert_eq!(
        json(&reused)["action_results"][1]["errors"][0]["code"],
        "candidate_reused_in_batch"
    );
}

#[test]
fn apply_rejects_missing_ids_invalid_states_and_validation_failures_without_partial_success() {
    let fixture = fixture();
    let db = fixture.db_path.to_str().unwrap();
    let failed = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        "reject-duplicate:cand_dup_a",
        "--action",
        "accept-transfer-pair:cand_transfer_from:cand_missing",
        "--json",
    ]);
    assert!(!failed.status.success());
    let failed = json(&failed);
    assert_eq!(failed["errors"][0]["code"], "batch_preflight_failed");
    assert_eq!(
        failed["action_results"][1]["errors"][0]["code"],
        "candidate_not_found"
    );
    let connection = Connection::open(&fixture.db_path).expect("reopen after failed batch");
    let unchanged: (String, i64) = connection
        .query_row(
            "SELECT
                (SELECT status FROM candidate_transactions WHERE id = 'cand_dup_a'),
                (SELECT COUNT(*) FROM canonical_transfer_pairs)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("unchanged state");
    assert_eq!(unchanged, ("possible_duplicate".to_string(), 0));
    drop(connection);

    let rejected_state = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        "reject-duplicate:cand_rejected",
        "--json",
    ]);
    assert_eq!(
        json(&rejected_state)["action_results"][0]["errors"][0]["code"],
        "candidate_already_rejected"
    );

    let missing_ids = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        "reject-duplicate",
        "--json",
    ]);
    assert_eq!(
        json(&missing_ids)["errors"][0]["code"],
        "candidate_ids_required"
    );

    let no_actions = run(&["candidates", "apply-actions", "--db", db, "--json"]);
    assert_eq!(json(&no_actions)["errors"][0]["code"], "actions_required");

    let invalid_pair = run(&[
        "candidates",
        "apply-actions",
        "--db",
        db,
        "--action",
        "accept-transfer-pair:cand_dup_a:cand_dup_b",
        "--json",
    ]);
    assert_eq!(
        json(&invalid_pair)["action_results"][0]["errors"][0]["code"],
        "transfer_pair_not_matching"
    );
}

#[test]
fn batch_commands_require_json_and_return_stable_errors() {
    let fixture = fixture();
    let db = fixture.db_path.to_str().unwrap();
    let commands = vec![
        vec![
            "candidates",
            "batch-summary",
            "--db",
            db,
            "--import-batch-id",
            &fixture.batch_id,
        ],
        vec!["candidates", "compare-duplicate", "cand_dup_a", "--db", db],
        vec![
            "candidates",
            "suggest-actions",
            "--db",
            db,
            "--import-batch-id",
            &fixture.batch_id,
        ],
        vec![
            "candidates",
            "apply-actions",
            "--db",
            db,
            "--action",
            "reject-duplicate:cand_dup_a",
        ],
    ];
    for args in commands {
        let output = run(&args);
        assert!(!output.status.success());
        let response = json(&output);
        assert_eq!(response["schema_version"], "tracky.batch-review.v1");
        assert_eq!(response["ok"], false);
        assert_eq!(response["errors"][0]["code"], "json_output_required");
    }

    let invalid_limit = run(&[
        "candidates",
        "batch-summary",
        "--db",
        db,
        "--import-batch-id",
        &fixture.batch_id,
        "--largest-limit",
        "0",
        "--json",
    ]);
    assert_eq!(
        json(&invalid_limit)["errors"][0]["code"],
        "invalid_largest_limit"
    );
}

#[test]
fn batch_commands_wrap_sqlite_query_failures_in_stable_json() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("empty.sqlite");
    drop(Connection::open(&db_path).expect("create empty sqlite file"));
    let db = db_path.to_str().unwrap();
    let commands = vec![
        vec![
            "candidates",
            "batch-summary",
            "--db",
            db,
            "--import-batch-id",
            "batch_missing_schema",
            "--json",
        ],
        vec![
            "candidates",
            "compare-duplicate",
            "cand_missing_schema",
            "--db",
            db,
            "--json",
        ],
        vec![
            "candidates",
            "suggest-actions",
            "--db",
            db,
            "--import-batch-id",
            "batch_missing_schema",
            "--json",
        ],
        vec![
            "candidates",
            "apply-actions",
            "--db",
            db,
            "--action",
            "reject-duplicate:cand_missing_schema",
            "--dry-run",
            "--json",
        ],
    ];
    for args in commands {
        let output = run(&args);
        assert!(!output.status.success());
        assert!(output.stderr.is_empty());
        let response = json(&output);
        assert_eq!(response["schema_version"], "tracky.batch-review.v1");
        assert_eq!(response["errors"][0]["code"], "database_operation_failed");
    }
}
