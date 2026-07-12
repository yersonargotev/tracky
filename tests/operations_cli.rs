use rusqlite::{params, Connection};
use serde_json::Value;
use std::fs;
use std::process::Command;
use tempfile::tempdir;
use tracky::storage::apply_migrations;

fn run(args: &[&str]) -> (bool, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_tracky"))
        .args(args)
        .output()
        .unwrap();
    (
        output.status.success(),
        serde_json::from_slice(&output.stdout).unwrap(),
    )
}

fn database(path: &std::path::Path) -> Connection {
    let c = Connection::open(path).unwrap();
    apply_migrations(&c).unwrap();
    c
}

#[test]
fn backup_is_consistent_openable_and_does_not_overwrite() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source with spaces.sqlite3");
    let destination = temp.path().join("backup with spaces.sqlite3");
    let c = database(&source);
    c.execute("INSERT INTO categories(id,name) VALUES('cat_1','Food')", [])
        .unwrap();
    drop(c);
    let (ok, json) = run(&[
        "backup",
        "--db",
        source.to_str().unwrap(),
        "--destination",
        destination.to_str().unwrap(),
        "--json",
    ]);
    assert!(ok);
    assert_eq!(json["schema_version"], "tracky.backup.v1");
    let restored = Connection::open(&destination).unwrap();
    assert_eq!(
        restored
            .query_row::<i64, _, _>("SELECT count(*) FROM categories", [], |r| r.get(0))
            .unwrap(),
        1
    );
    let (ok, json) = run(&[
        "backup",
        "--db",
        source.to_str().unwrap(),
        "--destination",
        destination.to_str().unwrap(),
        "--json",
    ]);
    assert!(!ok);
    assert_eq!(json["errors"][0]["code"], "destination_exists");
}

#[test]
fn failed_backup_leaves_no_final_artifact() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("missing.sqlite3");
    let destination = temp.path().join("backup.sqlite3");
    let (ok, json) = run(&[
        "backup",
        "--db",
        source.to_str().unwrap(),
        "--destination",
        destination.to_str().unwrap(),
        "--json",
    ]);
    assert!(!ok);
    assert_eq!(json["errors"][0]["code"], "backup_failed");
    assert!(!destination.exists());
}

#[test]
fn integrity_is_deterministic_read_only_and_reports_broken_links() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("ledger.sqlite3");
    let c = database(&path);
    c.execute("PRAGMA foreign_keys=OFF", []).unwrap();
    c.execute("INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency) VALUES('txn','missing','2026-01-01','x',100,'COP')",[]).unwrap();
    drop(c);
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    let (ok, first) = run(&["integrity", "--db", path.to_str().unwrap(), "--json"]);
    let (_, second) = run(&["integrity", "--db", path.to_str().unwrap(), "--json"]);
    assert!(!ok);
    assert_eq!(first, second);
    assert_eq!(first["sqlite_integrity"], "ok");
    assert_eq!(first["findings"][0]["code"], "transaction_account_missing");
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
}

#[test]
fn integrity_distinguishes_incompatible_schema() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("ledger.sqlite3");
    let c = database(&path);
    c.pragma_update(None, "user_version", 99).unwrap();
    drop(c);
    let (ok, json) = run(&["integrity", "--db", path.to_str().unwrap(), "--json"]);
    assert!(!ok);
    assert_eq!(json["findings"][0]["category"], "schema_incompatibility");
}

#[test]
fn integrity_reports_orphan_lines_manual_transfers_and_provenance() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("broken.sqlite3");
    let c = database(&path);
    c.execute("PRAGMA foreign_keys=OFF", []).unwrap();
    c.execute("INSERT INTO categories(id,name) VALUES('cat','Food')", [])
        .unwrap();
    c.execute("INSERT INTO transaction_lines(id,canonical_transaction_id,category_id,amount_minor,currency,line_kind) VALUES('line','missing','cat',-1,'COP','expense')",[]).unwrap();
    c.execute("INSERT INTO manual_transfer_pairs(id,posted_date,amount_minor,currency,from_account_id,to_account_id,from_canonical_transaction_id,to_canonical_transaction_id) VALUES('pair','2026-01-01',100,'COP','missing-a','missing-b','missing-from','missing-to')",[]).unwrap();
    c.execute("INSERT INTO manual_transaction_provenance(canonical_transaction_id,entry_id,source) VALUES('missing-txn','entry','manual_entry')",[]).unwrap();
    drop(c);
    let (ok, json) = run(&["integrity", "--db", path.to_str().unwrap(), "--json"]);
    assert!(!ok);
    let codes = json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["code"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"line_transaction_missing"));
    assert!(codes.contains(&"manual_transfer_leg_missing_or_incompatible"));
    assert!(codes.contains(&"manual_provenance_target_missing"));
}

#[test]
fn integrity_check_execution_failure_is_operational() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("invalid.sqlite3");
    fs::write(&path, b"not a sqlite database").unwrap();
    let (ok, json) = run(&["integrity", "--db", path.to_str().unwrap(), "--json"]);
    assert!(!ok);
    assert_eq!(json["errors"][0]["category"], "operational");
    assert_eq!(json["errors"][0]["code"], "integrity_check_failed");
}

#[test]
fn integrity_distinguishes_actual_sqlite_corruption() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("corrupt.sqlite3");
    let c = database(&path);
    c.execute("INSERT INTO categories(id,name) VALUES('c','Food')", [])
        .unwrap();
    drop(c);
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    let half = file.metadata().unwrap().len() / 2;
    file.set_len(half).unwrap();
    drop(file);
    let (ok, json) = run(&["integrity", "--db", path.to_str().unwrap(), "--json"]);
    assert!(!ok);
    assert_eq!(json["errors"][0]["category"], "sqlite_corruption");
}

#[test]
fn empty_export_is_stable_and_read_only() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("empty.sqlite3");
    drop(database(&path));
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    let (ok, first) = run(&["export", "--db", path.to_str().unwrap(), "--json"]);
    let (_, second) = run(&["export", "--db", path.to_str().unwrap(), "--json"]);
    assert!(ok);
    assert_eq!(first, second);
    assert_eq!(
        first["entities"]["canonical_transactions"],
        serde_json::json!([])
    );
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
}

#[test]
fn export_includes_manual_transfer_and_manual_provenance_links() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("manual.sqlite3");
    let (ok, created) = run(&[
        "accounts",
        "register",
        "--db",
        path.to_str().unwrap(),
        "--institution",
        "Bank",
        "--label",
        "A",
        "--account-type",
        "wallet",
        "--currency",
        "COP",
        "--json",
    ]);
    assert!(ok);
    let a = created["account"]["id"].as_str().unwrap().to_owned();
    let (ok, created) = run(&[
        "accounts",
        "register",
        "--db",
        path.to_str().unwrap(),
        "--institution",
        "Bank",
        "--label",
        "B",
        "--account-type",
        "wallet",
        "--currency",
        "COP",
        "--json",
    ]);
    assert!(ok);
    let b = created["account"]["id"].as_str().unwrap().to_owned();
    let (ok, _) = run(&[
        "transactions",
        "add-transfer",
        "--db",
        path.to_str().unwrap(),
        "--from-account-id",
        &a,
        "--to-account-id",
        &b,
        "--posted-date",
        "2026-01-01",
        "--description",
        "move",
        "--amount-minor",
        "100",
        "--currency",
        "COP",
        "--json",
    ]);
    assert!(ok);
    let (ok, json) = run(&["export", "--db", path.to_str().unwrap(), "--json"]);
    assert!(ok);
    assert_eq!(
        json["entities"]["manual_transfer_pairs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        json["entities"]["manual_provenance"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn export_preserves_exact_canonical_links_and_review_is_opt_in_and_redacted() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("ledger.sqlite3");
    let c = database(&path);
    c.execute(
        "INSERT INTO institutions(id,name) VALUES('inst','Bank')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO accounts(id,institution_id,label,currency,is_owned) VALUES('acc','inst','Wallet','COP',1)",[]).unwrap();
    c.execute("INSERT INTO source_documents(id,input_name,content_sha256,mime_type,byte_size) VALUES('doc','/secret/private.pdf',?1,'application/pdf',1)",params!["a".repeat(64)]).unwrap();
    c.execute("INSERT INTO import_batches(id,source_document_id,started_at,status) VALUES('batch','doc','2026-01-01','completed')",[]).unwrap();
    c.execute("INSERT INTO candidate_transactions(id,import_batch_id,source_document_id,posted_date,description,amount_minor,currency,confidence,status) VALUES('candidate','batch','doc','2026-01-01','pending',123,'COP',1.0,'rejected')",[]).unwrap();
    c.execute("INSERT INTO candidate_transactions(id,import_batch_id,source_document_id,posted_date,description,amount_minor,currency,confidence,status) VALUES('pending','batch','doc','2026-01-02','pending',124,'COP',1.0,'pending_review')",[]).unwrap();
    c.execute("INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency) VALUES('txn','acc','2026-01-02','exact',9007199254740991,'COP')",[]).unwrap();
    c.execute("INSERT INTO provenance(id,canonical_transaction_id,source_document_id,extractor_name,parser_id,parser_version,evidence_redaction,evidence_text_redacted,raw_storage_policy,raw_evidence_ref,confidence) VALUES('prov','txn','doc','x','p','1','redacted','<redacted>','not_stored','/secret/raw',1.0)",[]).unwrap();
    drop(c);
    let (ok, plain) = run(&["export", "--db", path.to_str().unwrap(), "--json"]);
    assert!(ok);
    assert!(plain["entities"].get("review_candidates").is_none());
    assert_eq!(
        plain["entities"]["canonical_transactions"][0]["amount_minor"],
        9007199254740991_i64
    );
    let serialized = serde_json::to_string(&plain).unwrap();
    assert!(!serialized.contains("private.pdf"));
    assert!(!serialized.contains("/secret/raw"));
    let (ok, audit) = run(&[
        "export",
        "--db",
        path.to_str().unwrap(),
        "--include-review-audit",
        "--json",
    ]);
    assert!(ok);
    assert_eq!(
        audit["entities"]["review_candidates"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(audit["entities"]["import_batches"][0]
        .get("error_details_json")
        .is_none());
    assert!(!serde_json::to_string(&audit)
        .unwrap()
        .contains("private.pdf"));
}

#[test]
fn assignment_audit_is_exported_backed_up_and_integrity_checked() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("assignment.sqlite3");
    let backup = temp.path().join("assignment-backup.sqlite3");
    let c = database(&path);
    c.execute(
        "INSERT INTO institutions(id,name) VALUES('inst-audit','Synthetic Bank')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO accounts(id,institution_id,label,currency,kind,is_owned) VALUES('acc-audit','inst-audit','Reviewed','COP','wallet',1)", []).unwrap();
    c.execute("INSERT INTO source_documents(id,input_name,content_sha256,mime_type,byte_size) VALUES('doc-audit','redacted.pdf',?1,'application/pdf',1)", params!["c".repeat(64)]).unwrap();
    c.execute("INSERT INTO import_batches(id,source_document_id,started_at,status) VALUES('batch-audit','doc-audit','2026-01-01','completed')", []).unwrap();
    c.execute("INSERT INTO candidate_transactions(id,import_batch_id,source_document_id,account_id,posted_date,description,amount_minor,currency,confidence,status) VALUES('candidate-audit','batch-audit','doc-audit','acc-audit','2026-01-01','redacted',100,'COP',1.0,'pending_review')", []).unwrap();
    c.execute("INSERT INTO candidate_account_assignment_events(id,candidate_transaction_id,revision,account_id,decision,reviewed_at) VALUES('assignment-audit','candidate-audit',1,'acc-audit','assign_owned_account','2026-01-02T00:00:00.000Z')", []).unwrap();
    drop(c);

    assert!(run(&["integrity", "--db", path.to_str().unwrap(), "--json"]).0);
    let (ok, exported) = run(&[
        "export",
        "--db",
        path.to_str().unwrap(),
        "--include-review-audit",
        "--json",
    ]);
    assert!(ok);
    assert_eq!(
        exported["entities"]["candidate_account_assignment_events"][0]["id"],
        "assignment-audit"
    );
    assert!(
        run(&[
            "backup",
            "--db",
            path.to_str().unwrap(),
            "--destination",
            backup.to_str().unwrap(),
            "--json",
        ])
        .0
    );
    let restored = Connection::open(&backup).unwrap();
    assert_eq!(
        restored
            .query_row::<i64, _, _>(
                "SELECT count(*) FROM candidate_account_assignment_events",
                [],
                |row| row.get(0),
            )
            .unwrap(),
        1
    );

    let c = Connection::open(&path).unwrap();
    c.execute("PRAGMA foreign_keys=OFF", []).unwrap();
    c.execute(
        "UPDATE candidate_transactions SET account_id=NULL WHERE id='candidate-audit'",
        [],
    )
    .unwrap();
    drop(c);
    let (ok, integrity) = run(&["integrity", "--db", path.to_str().unwrap(), "--json"]);
    assert!(!ok);
    assert!(serde_json::to_string(&integrity["findings"])
        .unwrap()
        .contains("candidate_account_assignment_head_mismatch"));
}

#[test]
fn backup_default_name_is_timestamped_adjacent_and_home_is_untouched() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let source = temp.path().join("ledger.sqlite3");
    drop(database(&source));
    let output = Command::new(env!("CARGO_BIN_EXE_tracky"))
        .env("HOME", &home)
        .args(["backup", "--db", source.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let destination = json["destination"].as_str().unwrap();
    assert!(destination.starts_with(temp.path().to_str().unwrap()));
    assert!(destination.contains("ledger-20"));
    assert!(destination.ends_with("Z.sqlite3"));
    assert_eq!(fs::read_dir(&home).unwrap().count(), 0);
}

#[test]
fn integrity_accepts_new_and_populated_databases() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("valid.sqlite3");
    drop(database(&path));
    assert!(run(&["integrity", "--db", path.to_str().unwrap(), "--json"]).0);
    let c = Connection::open(&path).unwrap();
    c.execute("INSERT INTO institutions(id,name) VALUES('i','Bank')", [])
        .unwrap();
    c.execute("INSERT INTO accounts(id,institution_id,label,currency,is_owned) VALUES('a','i','Wallet','COP',1)",[]).unwrap();
    c.execute("INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency) VALUES('t','a','2026-01-01','income',10,'COP')",[]).unwrap();
    drop(c);
    let (ok, json) = run(&["integrity", "--db", path.to_str().unwrap(), "--json"]);
    assert!(ok);
    assert!(serde_json::to_string(&json["counts"])
        .unwrap()
        .contains("canonical_transactions"));
}

#[test]
fn integrity_reports_imported_transfer_and_provenance_failures() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("broken-imported.sqlite3");
    let c = database(&path);
    c.execute("PRAGMA foreign_keys=OFF", []).unwrap();
    c.execute("INSERT INTO institutions(id,name) VALUES('i','Bank')", [])
        .unwrap();
    c.execute("INSERT INTO accounts(id,institution_id,label,currency,is_owned) VALUES('a','i','A','COP',1),('b','i','B','COP',1)",[]).unwrap();
    c.execute("INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency) VALUES('f','a','2026-01-01','from',100,'COP'),('t','b','2026-01-01','to',100,'COP')",[]).unwrap();
    c.execute("INSERT INTO canonical_transfer_pairs(id,transfer_kind,posted_date,amount_minor,currency,from_account_id,to_account_id,from_candidate_id,to_candidate_id,from_canonical_transaction_id,to_canonical_transaction_id) VALUES('pair','card_payment','2026-01-01',100,'COP','a','b','fc','tc','f','t')",[]).unwrap();
    c.execute("INSERT INTO source_documents(id,input_name,content_sha256,mime_type,byte_size) VALUES('d','redacted',?1,'application/pdf',1)",params!["b".repeat(64)]).unwrap();
    c.execute("INSERT INTO provenance(id,canonical_transaction_id,source_document_id,extractor_name,parser_id,parser_version,evidence_redaction,evidence_text_redacted,raw_storage_policy,confidence) VALUES('prov-broken','missing','d','x','p','1','r','r','not_stored',1)",[]).unwrap();
    drop(c);
    let (ok, json) = run(&["integrity", "--db", path.to_str().unwrap(), "--json"]);
    assert!(!ok);
    let findings = serde_json::to_string(&json["findings"]).unwrap();
    assert!(findings.contains("transfer_leg_missing_or_incompatible"));
    assert!(findings.contains("provenance_target_missing"));
}

fn seed_portable_investment(c: &Connection) {
    c.execute(
        "INSERT INTO institutions(id,name) VALUES('inv-inst','Broker')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO accounts(id,institution_id,label,currency,kind,is_owned) VALUES('inv-account','inv-inst','Portfolio','COP','brokerage',1)", []).unwrap();
    c.execute("INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency,transaction_kind,investment_allocation_status) VALUES('contribution','inv-account','2026-01-02','Investment',-10000,'COP','investment_contribution','pending_allocation')", []).unwrap();
    c.execute("INSERT INTO investment_instruments(id,name,instrument_type,denomination_currency,provider,provider_identifier) VALUES('instrument','Exact asset','security','COP','Broker','ASSET'),('fixed-instrument','CDT','fixed_income','COP','Bank','CDT-1')", []).unwrap();
    c.execute("INSERT INTO investment_allocation_revisions(id,allocation_id,revision,contribution_transaction_id,instrument_id,cash_amount_minor,cash_currency,acquired_quantity,effective_date,provenance_source) VALUES('allocation-rev','allocation',1,'contribution','instrument',6000,'COP','0.123456789012345678','2026-01-02','manual_entry'),('fixed-allocation-rev','fixed-allocation',1,'contribution','fixed-instrument',4000,'COP','1','2026-01-02','manual_entry')", []).unwrap();
    c.execute("INSERT INTO investment_allocation_heads(allocation_id,current_revision_id) VALUES('allocation','allocation-rev'),('fixed-allocation','fixed-allocation-rev')", []).unwrap();
    c.execute("INSERT INTO cdt_positions(id,instrument_id,account_id,constituent_allocation_id) VALUES('cdt','fixed-instrument','inv-account','fixed-allocation')", []).unwrap();
    c.execute("INSERT INTO cdt_operation_revisions(id,operation_id,revision,cdt_position_id,operation_type,effective_date,currency,principal_before_minor,principal_after_minor,external_capital_minor,funding_allocation_id,maturity_date,provenance_source) VALUES('cdt-rev','cdt-operation',1,'cdt','constitution','2026-01-03','COP',0,4000,4000,'fixed-allocation','2026-12-31','manual_entry')", []).unwrap();
    c.execute("INSERT INTO cdt_operation_heads(operation_id,current_revision_id) VALUES('cdt-operation','cdt-rev')", []).unwrap();
    c.execute("INSERT INTO brokerage_accounts(account_id,opened_date,provenance_source) VALUES('inv-account','2026-01-01','manual_entry')", []).unwrap();
    c.execute("INSERT INTO brokerage_operation_revisions(id,operation_id,revision,account_id,operation_type,effective_date,currency,gross_amount_minor,net_cash_minor,funding_allocation_id,provenance_source) VALUES('deposit-rev','deposit-operation',1,'inv-account','deposit','2026-01-03','COP',6000,6000,'allocation','manual_entry')", []).unwrap();
    c.execute("INSERT INTO brokerage_operation_revisions(id,operation_id,revision,account_id,operation_type,effective_date,currency,instrument_id,quantity,gross_amount_minor,historical_cost_minor,net_cash_minor,provenance_source) VALUES('buy-rev','buy-operation',1,'inv-account','buy','2026-01-04','COP','instrument','1.1',5000,5000,-5000,'manual_entry')", []).unwrap();
    c.execute("INSERT INTO brokerage_operation_heads(operation_id,current_revision_id) VALUES('deposit-operation','deposit-rev'),('buy-operation','buy-rev')", []).unwrap();
    c.execute("INSERT INTO brokerage_buy_funding_attributions(operation_revision_id,external_capital_minor,existing_cash_minor,reinvested_minor,investment_income_minor,unattributed_minor) VALUES('buy-rev',5000,0,0,0,0)", []).unwrap();
    c.execute("INSERT INTO investment_allocation_consumptions(allocation_id,consumer_kind,cdt_position_id,consumer_operation_id) VALUES('fixed-allocation','cdt_constitution','cdt','cdt-operation'),('allocation','brokerage_deposit',NULL,'deposit-operation')", []).unwrap();
    c.execute("INSERT INTO investment_snapshots(id,observed_at,provider_effective_date,source,external_reference,provenance_source) VALUES('snapshot','2026-01-31T00:00:00Z','2026-01-31','manual statement','snapshot-ref','manual_entry')", []).unwrap();
    c.execute("INSERT INTO investment_snapshot_positions(snapshot_id,account_id,instrument_id,quantity,currency,observed_value_minor,valuation_currency,observed_price) VALUES('snapshot','inv-account','instrument','1.1','COP',5500,'COP','5000')", []).unwrap();
    c.execute("INSERT INTO investment_snapshot_baselines(snapshot_id,account_id,instrument_id,currency,status,quantity_difference,derived_historical_cost_minor,derived_value_minor,value_difference_minor) VALUES('snapshot','inv-account','instrument','COP','matched','0',5000,5500,0)", []).unwrap();
    c.execute("INSERT INTO investment_adjustment_revisions(id,adjustment_id,revision,snapshot_id,account_id,instrument_id,currency,quantity_delta,historical_cost_delta_minor,effective_date,reason,provenance_source) VALUES('adjustment-rev','adjustment',1,'snapshot','inv-account','instrument','COP','0.1',0,'2026-01-30','confirmed missing history','manual_entry')", []).unwrap();
    c.execute("INSERT INTO investment_adjustment_heads(adjustment_id,current_revision_id) VALUES('adjustment','adjustment-rev')", []).unwrap();
    c.execute("INSERT INTO source_documents(id,input_name,content_sha256,mime_type,byte_size) VALUES('provider-doc','redacted',?1,'application/pdf',1)", params!["d".repeat(64)]).unwrap();
    c.execute("INSERT INTO import_batches(id,source_document_id,started_at,status) VALUES('provider-batch','provider-doc','2026-01-01','completed')", []).unwrap();
    c.execute("INSERT INTO investment_document_events(id,source_document_id,import_batch_id,account_id,provider,parser_id,parser_version,event_type,provider_effective_date,currency,quantity,page_number,row_index,evidence_redaction,fingerprint,status,decision,reconciled_kind,reconciled_id,accepted_snapshot_id,reviewed_at) VALUES('provider-event','provider-doc','provider-batch','inv-account','wenia','parser','1','observed_position','2026-01-31','COP','1.1',1,1,'redacted','provider-fingerprint','accepted','accept_snapshot','investment_snapshot','snapshot','snapshot','2026-01-31')", []).unwrap();
    c.execute("INSERT INTO provenance(id,investment_document_event_id,investment_snapshot_id,source_document_id,import_batch_id,page_number,row_index,extractor_name,parser_id,parser_version,evidence_redaction,evidence_text_redacted,raw_storage_policy,confidence) VALUES('investment-prov','provider-event','snapshot','provider-doc','provider-batch',1,1,'extractor','parser','1','redacted','safe','not_stored',1.0)", []).unwrap();
}

#[test]
fn investment_export_preserves_exact_history_links_and_review_boundary() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("investments.sqlite3");
    let c = database(&path);
    seed_portable_investment(&c);
    c.execute("PRAGMA foreign_keys=OFF", []).unwrap();
    c.execute("INSERT INTO source_documents(id,input_name,content_sha256,mime_type,byte_size) VALUES('idoc','/secret/provider.pdf',?1,'application/pdf',1)", params!["c".repeat(64)]).unwrap();
    c.execute("INSERT INTO import_batches(id,source_document_id,started_at,status) VALUES('ibatch','idoc','2026-01-01','completed')", []).unwrap();
    for (id, status, decision, reviewed) in [
        (
            "accepted-event",
            "accepted",
            Some("reconcile_deposit"),
            Some("2026-01-03"),
        ),
        ("pending-event", "pending_review", None, None),
        (
            "rejected-event",
            "rejected",
            Some("reject"),
            Some("2026-01-03"),
        ),
    ] {
        c.execute("INSERT INTO investment_document_events(id,source_document_id,import_batch_id,provider,parser_id,parser_version,event_type,provider_effective_date,currency,amount_minor,page_number,row_index,evidence_redaction,fingerprint,status,decision,reviewed_at) VALUES(?1,'idoc','ibatch','wenia','parser','1','deposit','2026-01-03','COP',100,1,1,'safe-redacted',?2,?3,?4,?5)", params![id, format!("fingerprint-{id}"), status, decision, reviewed]).unwrap();
    }
    drop(c);
    let (ok, plain) = run(&["export", "--db", path.to_str().unwrap(), "--json"]);
    assert!(ok);
    let (_, repeated) = run(&["export", "--db", path.to_str().unwrap(), "--json"]);
    assert_eq!(plain, repeated);
    for entity in [
        "investment_instruments",
        "investment_allocation_revisions",
        "investment_allocation_heads",
        "investment_allocation_consumptions",
        "cdt_positions",
        "cdt_operation_revisions",
        "cdt_operation_heads",
        "brokerage_accounts",
        "brokerage_operation_revisions",
        "brokerage_operation_heads",
        "brokerage_buy_funding_attributions",
        "investment_snapshots",
        "investment_snapshot_positions",
        "investment_snapshot_baselines",
        "investment_adjustment_revisions",
        "investment_adjustment_heads",
        "investment_provenance",
    ] {
        assert!(
            !plain["entities"][entity].as_array().unwrap().is_empty(),
            "{entity}"
        );
    }
    assert_eq!(
        plain["entities"]["investment_allocation_revisions"][0]["acquired_quantity"],
        "0.123456789012345678"
    );
    assert_eq!(
        plain["entities"]["investment_allocation_heads"][0]["current_revision_id"],
        "allocation-rev"
    );
    assert_eq!(
        plain["entities"]["brokerage_buy_funding_attributions"][0]["external_capital_minor"],
        5000
    );
    assert_eq!(
        plain["entities"]["cdt_operation_revisions"][0]["currency"],
        "COP"
    );
    assert_eq!(
        plain["entities"]["investment_snapshot_positions"][0]["quantity"],
        "1.1"
    );
    assert_eq!(
        plain["entities"]["accepted_investment_document_events"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(plain["entities"]
        .get("investment_document_review_events")
        .is_none());
    let serialized = serde_json::to_string(&plain).unwrap();
    assert!(!serialized.contains("provider.pdf"));
    let (ok, audit) = run(&[
        "export",
        "--db",
        path.to_str().unwrap(),
        "--include-review-audit",
        "--json",
    ]);
    assert!(ok);
    assert_eq!(
        audit["entities"]["investment_document_review_events"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert!(!serde_json::to_string(&audit)
        .unwrap()
        .contains("provider.pdf"));
}

#[test]
fn investment_backup_restore_has_equivalent_reports_and_integrity() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source.sqlite3");
    let restored = temp.path().join("restore.sqlite3");
    let c = database(&source);
    seed_portable_investment(&c);
    drop(c);
    let (_, source_report) = run(&[
        "reports",
        "investments",
        "--db",
        source.to_str().unwrap(),
        "--from",
        "2026-01-01",
        "--to",
        "2026-01-31",
        "--json",
    ]);
    assert!(
        run(&[
            "backup",
            "--db",
            source.to_str().unwrap(),
            "--destination",
            restored.to_str().unwrap(),
            "--json"
        ])
        .0
    );
    assert!(run(&["integrity", "--db", restored.to_str().unwrap(), "--json"]).0);
    let (ok, restore_report) = run(&[
        "reports",
        "investments",
        "--db",
        restored.to_str().unwrap(),
        "--from",
        "2026-01-01",
        "--to",
        "2026-01-31",
        "--json",
    ]);
    assert!(ok);
    assert_eq!(source_report, restore_report);
    assert_eq!(
        Connection::open(&source)
            .unwrap()
            .query_row::<i64, _, _>(
                "SELECT count(*) FROM investment_allocation_revisions",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        2
    );
}

#[test]
fn investment_integrity_reports_representative_broken_links_and_invariants() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("broken-investments.sqlite3");
    let c = database(&path);
    c.execute("PRAGMA foreign_keys=OFF", []).unwrap();
    c.execute("INSERT INTO investment_allocation_revisions(id,allocation_id,revision,contribution_transaction_id,instrument_id,cash_amount_minor,cash_currency,acquired_quantity,provenance_source) VALUES('bad-rev','bad',1,'missing-contribution','missing-instrument',10,'COP','1e3','manual_entry')", []).unwrap();
    c.execute("INSERT INTO investment_allocation_heads(allocation_id,current_revision_id) VALUES('wrong','bad-rev')", []).unwrap();
    c.execute("INSERT INTO brokerage_buy_funding_attributions(operation_revision_id,external_capital_minor,existing_cash_minor,reinvested_minor,investment_income_minor,unattributed_minor) VALUES('missing-buy',1,0,0,0,0)", []).unwrap();
    drop(c);
    let (ok, json) = run(&["integrity", "--db", path.to_str().unwrap(), "--json"]);
    assert!(!ok);
    let findings = serde_json::to_string(&json["findings"]).unwrap();
    assert!(findings.contains("allocation_reference_missing"));
    assert!(findings.contains("allocation_head_missing_or_incorrect"));
    assert!(findings.contains("investment_exact_decimal_invalid"));
    assert!(findings.contains("brokerage_funding_attribution_inconsistent"));
}

#[test]
fn investment_integrity_detects_lifecycle_consumption_source_and_replay_failures() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("broken-active-investments.sqlite3");
    let c = database(&path);
    seed_portable_investment(&c);
    c.execute("PRAGMA foreign_keys=OFF", []).unwrap();
    c.execute("UPDATE investment_allocation_revisions SET cash_amount_minor=100 WHERE id='allocation-rev'", []).unwrap();
    c.execute(
        "UPDATE investment_snapshots SET source=' ' WHERE id='snapshot'",
        [],
    )
    .unwrap();
    c.execute(
        "UPDATE brokerage_operation_revisions SET net_cash_minor=-1 WHERE id='deposit-rev'",
        [],
    )
    .unwrap();
    c.execute(
        "UPDATE cdt_operation_revisions SET agreed_rate='1e3', funding_allocation_id='missing' WHERE id='cdt-rev'",
        [],
    )
    .unwrap();
    c.execute(
        "UPDATE investment_snapshot_baselines SET quantity_difference='01.0' WHERE snapshot_id='snapshot'",
        [],
    )
    .unwrap();
    c.execute(
        "UPDATE investment_adjustment_revisions SET quantity_delta='NaN' WHERE id='adjustment-rev'",
        [],
    )
    .unwrap();
    c.execute(
        "UPDATE provenance SET investment_document_event_id='missing-event' WHERE id='investment-prov'",
        [],
    )
    .unwrap();
    c.execute(
        "DELETE FROM brokerage_operation_heads WHERE operation_id='buy-operation'",
        [],
    )
    .unwrap();
    c.execute(
        "DELETE FROM investment_snapshot_positions WHERE snapshot_id='snapshot'",
        [],
    )
    .unwrap();
    for suffix in ["one", "two"] {
        c.execute("INSERT INTO cdt_operation_revisions(id,operation_id,revision,cdt_position_id,operation_type,effective_date,currency,principal_before_minor,principal_after_minor,principal_returned_minor,net_cash_received_minor,maturity_date,provenance_source) VALUES(?1,?2,1,'cdt','redemption','2026-12-31','COP',4000,0,4000,4000,'2026-12-31','manual_entry')", params![format!("redemption-rev-{suffix}"),format!("redemption-{suffix}")]).unwrap();
        c.execute(
            "INSERT INTO cdt_operation_heads(operation_id,current_revision_id) VALUES(?1,?2)",
            params![
                format!("redemption-{suffix}"),
                format!("redemption-rev-{suffix}")
            ],
        )
        .unwrap();
    }
    drop(c);
    let (ok, json) = run(&["integrity", "--db", path.to_str().unwrap(), "--json"]);
    assert!(!ok);
    let findings = serde_json::to_string(&json["findings"]).unwrap();
    for code in [
        "allocation_consumption_exceeds_available",
        "brokerage_operation_active_head_missing",
        "brokerage_replay_impossible",
        "cdt_duplicate_active_lifecycle",
        "cdt_cash_or_funding_link_missing",
        "investment_exact_decimal_invalid",
        "investment_provenance_orphaned",
        "snapshot_source_invalid",
        "snapshot_account_missing",
    ] {
        assert!(findings.contains(code), "missing {code}: {findings}");
    }
    let order = json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|finding| {
            (
                finding["category"].as_str().unwrap().to_owned(),
                finding["entity"].as_str().unwrap().to_owned(),
                finding["id"].as_str().unwrap().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(order, sorted);
}

#[test]
fn investment_export_and_integrity_do_not_touch_home_or_database() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let path = temp.path().join("portable.sqlite3");
    let c = database(&path);
    seed_portable_investment(&c);
    drop(c);
    let before = fs::read(&path).unwrap();
    for command in ["export", "integrity"] {
        let output = Command::new(env!("CARGO_BIN_EXE_tracky"))
            .env("HOME", &home)
            .args([command, "--db", path.to_str().unwrap(), "--json"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{command}");
    }
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(home).unwrap().count(), 0);
}
