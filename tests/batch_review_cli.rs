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
    accept_candidate, apply_migrations, persist_pdf_import, register_owned_account,
    reject_candidate, AccountRegisterInput,
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
        posted_date: "2026-06-15".to_string(),
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
        }),
        candidate(CandidateSpec {
            id: "cand_accepted",
            fingerprint: "fp_accepted",
            institution: "nequi",
            account_label: "Nequi wallet",
            amount_minor: -300,
            direction_hint: DirectionHint::Outflow,
            semantic_hint: SemanticHint::CardPayment,
            row_index: 5,
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
    assert!(
        accept_candidate(&mut connection, "cand_accepted")
            .expect("accept fixture candidate")
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
    }
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
    assert_eq!(summary["summary"]["total_candidates"], 7);
    assert_eq!(group_count(&summary, "by_status", "accepted"), 1);
    assert_eq!(group_count(&summary, "by_status", "possible_duplicate"), 2);
    assert_eq!(group_count(&summary, "by_status", "pending_review"), 3);
    assert_eq!(group_count(&summary, "by_status", "rejected"), 1);
    assert_eq!(
        group_count(&summary, "by_duplicate_status", "exact_duplicate"),
        2
    );
    assert_eq!(group_count(&summary, "by_institution", "nequi"), 5);
    assert_eq!(group_count(&summary, "by_institution", "rappi"), 1);
    assert_eq!(
        group_count(&summary, "by_account_resolution", "resolved"),
        6
    );
    assert_eq!(
        group_count(&summary, "by_account_resolution", "unresolved"),
        1
    );
    assert_eq!(group_count(&summary, "by_direction_hint", "outflow"), 5);
    assert_eq!(
        group_count(&summary, "by_semantic_hint", "bank_movement"),
        4
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
