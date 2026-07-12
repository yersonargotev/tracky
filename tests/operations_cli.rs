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
    c.execute("INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency) VALUES('f','a','2026-01-01','from',-100,'COP'),('t','b','2026-01-01','to',99,'COP')",[]).unwrap();
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
