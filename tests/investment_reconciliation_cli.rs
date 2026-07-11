use serde_json::{json, Value};
use std::process::{Command, Output};
fn tracky() -> &'static str {
    env!("CARGO_BIN_EXE_tracky")
}
fn output(args: &[&str]) -> Output {
    Command::new(tracky()).args(args).output().unwrap()
}
fn run(args: &[&str]) -> Value {
    let o = output(args);
    assert!(
        o.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&o.stderr),
        String::from_utf8_lossy(&o.stdout)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}
fn account(db: &str) -> String {
    run(&[
        "accounts",
        "register",
        "--db",
        db,
        "--institution",
        "Synthetic provider",
        "--label",
        "Synthetic custody",
        "--account-type",
        "broker",
        "--currency",
        "COP",
        "--json",
    ])["account"]["id"]
        .as_str()
        .unwrap()
        .into()
}
fn instrument(db: &str) -> String {
    run(&[
        "instruments",
        "create",
        "--db",
        db,
        "--name",
        "Synthetic security",
        "--type",
        "security",
        "--denomination-currency",
        "COP",
        "--provider",
        "Synthetic provider",
        "--provider-identifier",
        "SYN",
        "--json",
    ])["instrument"]["id"]
        .as_str()
        .unwrap()
        .into()
}
fn snapshot(
    db: &str,
    a: &str,
    i: &str,
    reference: &str,
    quantity: &str,
    cash: Option<i64>,
) -> Value {
    let mut positions = vec![
        json!({"account_id":a,"instrument_id":i,"quantity":quantity,"currency":"COP","observed_value_minor":120000,"valuation_currency":"COP","observed_price":"12000"}),
    ];
    if let Some(c) = cash {
        positions.push(json!({"account_id":a,"instrument_id":null,"quantity":null,"currency":"COP","observed_cash_minor":c,"observed_value_minor":c,"valuation_currency":"COP","observed_price":null}));
    }
    let payload=json!({"observed_at":"2026-07-10T15:00:00-05:00","provider_effective_date":"2026-07-10","source":"synthetic_statement","external_reference":reference,"provenance_source":"manual_entry","positions":positions}).to_string();
    run(&[
        "snapshots",
        "record",
        "--db",
        db,
        "--snapshot-json",
        &payload,
        "--json",
    ])
}
#[test]
fn dated_snapshot_is_exact_inspectable_and_comparison_is_read_only() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("x.sqlite");
    let db = db.to_str().unwrap();
    let a = account(db);
    let i = instrument(db);
    let s = snapshot(db, &a, &i, "statement-1", "10.000", Some(500));
    assert!(s["snapshot"]["positions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["quantity"] == "10"));
    let id = s["snapshot"]["id"].as_str().unwrap();
    let before = std::fs::metadata(db).unwrap().len();
    let x = run(&[
        "snapshots",
        "compare",
        "--db",
        db,
        "--snapshot-id",
        id,
        "--as-of",
        "2026-07-12",
        "--json",
    ]);
    assert!(x["reconciliations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["status"] == "missing_derived_position"));
    let after = std::fs::metadata(db).unwrap().len();
    assert_eq!(
        before, after,
        "comparison must not add canonical economic state"
    );
    let inspected = run(&[
        "snapshots",
        "inspect",
        "--db",
        db,
        "--snapshot-id",
        id,
        "--json",
    ]);
    assert_eq!(inspected["snapshot"]["source"], "synthetic_statement");
}
#[test]
fn freshness_and_missing_valuation_are_deterministic() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("x.sqlite");
    let db = db.to_str().unwrap();
    let a = account(db);
    let i = instrument(db);
    let s = snapshot(db, &a, &i, "statement-2", "2", None);
    let id = s["snapshot"]["id"].as_str().unwrap();
    let x = run(&[
        "snapshots",
        "compare",
        "--db",
        db,
        "--snapshot-id",
        id,
        "--as-of",
        "2026-07-20",
        "--json",
    ]);
    assert_eq!(x["reconciliations"][0]["status"], "stale");
    assert_eq!(x["reconciliations"][0]["age_days"], 10);
    assert!(x["freshness_policy"]
        .as_str()
        .unwrap()
        .contains("7 calendar days"));
}
#[test]
fn explicit_adjustment_reconciles_and_preserves_original_difference_and_history() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("x.sqlite");
    let db = db.to_str().unwrap();
    let a = account(db);
    let i = instrument(db);
    let s = snapshot(db, &a, &i, "statement-3", "3.25", None);
    let sid = s["snapshot"]["id"].as_str().unwrap();
    let first = run(&[
        "snapshots",
        "compare",
        "--db",
        db,
        "--snapshot-id",
        sid,
        "--as-of",
        "2026-07-10",
        "--json",
    ]);
    assert_eq!(
        first["reconciliations"][0]["original_status"],
        "missing_derived_position"
    );
    let payload=json!({"snapshot_id":sid,"account_id":a,"instrument_id":i,"currency":"COP","quantity_delta":"3.25","cash_delta_minor":null,"historical_cost_delta_minor":30000,"effective_date":"2026-07-10","reason":"confirmed missing opening history","provenance_source":"manual_review"}).to_string();
    let adjusted = run(&[
        "snapshots",
        "adjust",
        "--db",
        db,
        "--adjustment-json",
        &payload,
        "--json",
    ]);
    let aid = adjusted["adjustments"][0]["adjustment_id"]
        .as_str()
        .unwrap();
    let after = run(&[
        "snapshots",
        "compare",
        "--db",
        db,
        "--snapshot-id",
        sid,
        "--as-of",
        "2026-07-10",
        "--json",
    ]);
    assert_eq!(after["reconciliations"][0]["status"], "matched");
    assert_eq!(
        after["reconciliations"][0]["original_status"],
        "missing_derived_position"
    );
    let replacement=json!({"snapshot_id":sid,"account_id":a,"instrument_id":i,"currency":"COP","quantity_delta":"3.25","cash_delta_minor":null,"historical_cost_delta_minor":31000,"effective_date":"2026-07-10","reason":"confirmed corrected opening cost","provenance_source":"manual_review"}).to_string();
    let history = run(&[
        "snapshots",
        "replace-adjustment",
        "--db",
        db,
        "--adjustment-id",
        aid,
        "--reason",
        "correct cost evidence",
        "--replacement-json",
        &replacement,
        "--json",
    ]);
    assert_eq!(history["adjustments"].as_array().unwrap().len(), 2);
    assert_eq!(
        history["adjustments"][1]["replaces_revision_id"],
        history["adjustments"][0]["id"]
    );
}
#[test]
fn duplicate_provider_reference_and_invalid_batch_are_atomic() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("x.sqlite");
    let db = db.to_str().unwrap();
    let a = account(db);
    let i = instrument(db);
    let _ = snapshot(db, &a, &i, "duplicate", "1", None);
    let payload=json!({"observed_at":"2026-07-10T15:00:00Z","source":"synthetic_statement","external_reference":"duplicate","provenance_source":"manual_entry","positions":[{"account_id":a,"instrument_id":i,"quantity":"1","currency":"COP"}]}).to_string();
    let o = output(&[
        "snapshots",
        "record",
        "--db",
        db,
        "--snapshot-json",
        &payload,
        "--json",
    ]);
    assert!(!o.status.success());
    let e: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(e["errors"][0]["code"], "duplicate_provider_reference");
    let listed = run(&["snapshots", "list", "--db", db, "--json"]);
    assert_eq!(listed["snapshots"].as_array().unwrap().len(), 1);
}
