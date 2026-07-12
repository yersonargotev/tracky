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
        audit["entities"]["review_candidates"][0]["status"],
        "rejected"
    );
    assert!(!serde_json::to_string(&audit)
        .unwrap()
        .contains("private.pdf"));
}
