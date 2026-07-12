use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::process::{Command, Output};
use tracky::storage::apply_migrations;

fn output(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tracky"))
        .args(args)
        .output()
        .unwrap()
}

fn run(args: &[&str]) -> Value {
    let result = output(args);
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stderr),
        String::from_utf8_lossy(&result.stdout)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

fn seed(path: &str) -> Connection {
    let c = Connection::open(path).unwrap();
    apply_migrations(&c).unwrap();
    c.execute_batch("INSERT INTO institutions(id,name) VALUES('nu','Nu'),('bank','Synthetic bank');
      INSERT INTO accounts(id,institution_id,label,currency,is_owned) VALUES('nu-account','nu','Nu','COP',1),('funding','bank','Funding','COP',1);
      INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency,transaction_kind,investment_allocation_status) VALUES('contribution','funding','2026-06-01','Synthetic capital',-100000,'COP','investment_contribution','pending_allocation'),('other-contribution','funding','2026-06-01','Other capital',-100000,'COP','investment_contribution','pending_allocation');
      INSERT INTO investment_instruments(id,name,instrument_type,denomination_currency,provider,provider_identifier) VALUES('cdt-instrument','Synthetic CDT','fixed_income','COP','Nu','synthetic-cdt'),('other-instrument','Other CDT','fixed_income','COP','Other','other-cdt');
      INSERT INTO investment_allocation_revisions(id,allocation_id,revision,contribution_transaction_id,instrument_id,cash_amount_minor,cash_currency,acquired_quantity,effective_date,provenance_source) VALUES('allocation-rev','allocation',1,'contribution','cdt-instrument',100000,'COP','1','2026-06-01','manual_entry'),('other-allocation-rev','other-allocation',1,'other-contribution','other-instrument',100000,'COP','1','2026-06-01','manual_entry');
      INSERT INTO investment_allocation_heads(allocation_id,current_revision_id) VALUES('allocation','allocation-rev'),('other-allocation','other-allocation-rev');
      INSERT INTO source_documents(id,input_name,content_sha256,mime_type,byte_size,institution_hint) VALUES('document','synthetic.pdf','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','application/pdf',1,'nu');
      INSERT INTO import_batches(id,source_document_id,started_at,status) VALUES('batch','document','2026-06-01','completed');").unwrap();
    c
}

fn event(c: &Connection, id: &str, kind: &str, date: &str, amount: i64) {
    c.execute("INSERT INTO investment_document_events(id,source_document_id,import_batch_id,provider,parser_id,parser_version,event_type,provider_effective_date,currency,amount_minor,page_number,row_index,evidence_redaction,fingerprint,status) VALUES(?1,'document','batch','nu','nu.synthetic','1',?2,?3,'COP',?4,1,1,'Synthetic redacted CDT evidence',?1,'pending_review')", params![id,kind,date,amount]).unwrap();
    c.execute("INSERT INTO provenance(id,investment_document_event_id,source_document_id,import_batch_id,page_number,row_index,extractor_name,parser_id,parser_version,evidence_redaction,evidence_text_redacted,raw_storage_policy,confidence) VALUES('prov_'||?1,?1,'document','batch',1,1,'synthetic','nu.synthetic','1','Synthetic redacted CDT evidence','Synthetic redacted CDT evidence','redacted_only',1)", [id]).unwrap();
}

#[test]
fn opening_preview_and_dry_run_are_read_only_then_apply_is_atomic_and_audited() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("x.sqlite");
    let db = path.to_str().unwrap();
    let c = seed(db);
    event(&c, "opening", "cdt_opening", "2026-06-01", 100000);
    drop(c);
    let preview = run(&[
        "investment-documents",
        "cdt-actions",
        "opening",
        "--db",
        db,
        "--json",
    ]);
    assert_eq!(
        preview["event_evidence"],
        json!({"effective_date":"2026-06-01","currency":"COP","amount_minor":100000})
    );
    assert_eq!(preview["actions"][0]["action"], "constitution");
    assert_eq!(
        preview["actions"][0]["required_reviewer_fields"],
        json!([
            "allocation_id",
            "maturity_date",
            "allows_partial_redemption"
        ])
    );
    let request=json!({"action":"constitution","allocation_id":"allocation","maturity_date":"2026-12-01","agreed_rate":"10.5","payment_mode":"at_maturity","payment_periodicity":null,"renewal_terms":null,"contract_identifier":null,"allows_partial_redemption":false}).to_string();
    let dry = run(&[
        "investment-documents",
        "enrich-cdt",
        "opening",
        "--db",
        db,
        "--request-json",
        &request,
        "--dry-run",
        "--json",
    ]);
    assert_eq!(dry["mode"], "dry_run");
    assert!(dry["ok"].as_bool().unwrap());
    let c = Connection::open(db).unwrap();
    assert_eq!(
        c.query_row("SELECT count(*) FROM cdt_positions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        c.query_row(
            "SELECT status FROM investment_document_events WHERE id='opening'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "pending_review"
    );
    drop(c);
    let applied = run(&[
        "investment-documents",
        "enrich-cdt",
        "opening",
        "--db",
        db,
        "--request-json",
        &request,
        "--apply",
        "--json",
    ]);
    assert_eq!(applied["mode"], "apply");
    assert_eq!(applied["event"]["decision"], "enrich_cdt_constitution");
    assert_eq!(
        applied["operation"]["provenance_source"],
        "provider_document_contractual_enrichment"
    );
    assert_eq!(
        applied["enrichment"]["reviewer_terms"]["agreed_rate"],
        "10.5"
    );
    assert_eq!(
        applied["enrichment"]["provider_evidence"]["amount_minor"],
        100000
    );
    let report = run(&[
        "reports",
        "investments",
        "--db",
        db,
        "--from",
        "2026-06-01",
        "--to",
        "2026-06-30",
        "--json",
    ]);
    assert!(report["pending_and_reconciliation"]["provider_events"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        report["cdt_provider_enrichments"][0]["provider_evidence"]["amount_minor"],
        100000
    );
    assert_eq!(
        report["cdt_provider_enrichments"][0]["reviewer_terms"]["agreed_rate"],
        "10.5"
    );
    let exported = run(&["export", "--db", db, "--json"]);
    assert_eq!(
        exported["entities"]["cdt_provider_enrichments"][0]["event_id"],
        "opening"
    );
    assert_eq!(
        exported["entities"]["investment_provenance"][0]["cdt_operation_revision_id"],
        applied["operation"]["id"]
    );
    assert!(run(&["integrity", "--db", db, "--json"])["ok"]
        .as_bool()
        .unwrap());
    let restored = d.path().join("restored.sqlite");
    run(&[
        "backup",
        "--db",
        db,
        "--destination",
        restored.to_str().unwrap(),
        "--json",
    ]);
    let restored_c = Connection::open(restored).unwrap();
    assert_eq!(
        restored_c
            .query_row("SELECT count(*) FROM cdt_provider_enrichments", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        1
    );

    let reused = output(&[
        "investment-documents",
        "enrich-cdt",
        "opening",
        "--db",
        db,
        "--request-json",
        &request,
        "--apply",
        "--json",
    ]);
    assert!(!reused.status.success());
    let reused: Value = serde_json::from_slice(&reused.stdout).unwrap();
    assert_eq!(reused["errors"][0]["code"], "event_not_pending");
}

#[test]
fn ambiguous_returns_and_incomplete_or_reused_requests_stay_pending_without_writes() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("x.sqlite");
    let db = path.to_str().unwrap();
    let c = seed(db);
    event(&c, "return", "cdt_return", "2026-12-01", 110000);
    c.execute_batch("INSERT INTO cdt_positions(id,instrument_id,account_id,constituent_allocation_id) VALUES('other-position','other-instrument','nu-account','other-allocation'); INSERT INTO cdt_operation_revisions(id,operation_id,revision,cdt_position_id,operation_type,effective_date,currency,principal_before_minor,principal_after_minor,external_capital_minor,funding_allocation_id,maturity_date,provenance_source) VALUES('other-operation-rev','other-operation',1,'other-position','constitution','2026-06-01','COP',0,100000,100000,'other-allocation','2026-11-30','manual_entry'); INSERT INTO cdt_operation_heads(operation_id,current_revision_id) VALUES('other-operation','other-operation-rev');").unwrap();
    drop(c);
    let preview = run(&[
        "investment-documents",
        "cdt-actions",
        "return",
        "--db",
        db,
        "--json",
    ]);
    assert_eq!(
        preview["actions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["action"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["renewal", "redemption", "link_existing"]
    );
    let incomplete = json!({"action":"redemption","position_id":"missing"}).to_string();
    let failed = output(&[
        "investment-documents",
        "enrich-cdt",
        "return",
        "--db",
        db,
        "--request-json",
        &incomplete,
        "--apply",
        "--json",
    ]);
    assert!(!failed.status.success());
    let value: Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(value["errors"][0]["code"], "incomplete_reviewer_terms");
    let c = Connection::open(db).unwrap();
    assert_eq!(
        c.query_row(
            "SELECT status FROM investment_document_events WHERE id='return'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "pending_review"
    );
    assert_eq!(
        c.query_row("SELECT count(*) FROM cdt_operation_revisions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(c);
    let incompatible_position=json!({"action":"redemption","position_id":"other-position","principal_returned_minor":100000,"gross_interest_minor":10000,"withholding_minor":0,"other_deductions_minor":0,"deduction_component_id":null,"deduction_expense_transaction_id":null}).to_string();
    let failed = output(&[
        "investment-documents",
        "enrich-cdt",
        "return",
        "--db",
        db,
        "--request-json",
        &incompatible_position,
        "--apply",
        "--json",
    ]);
    assert!(!failed.status.success());
    let failed: Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(failed["errors"][0]["code"], "cdt_target_incompatible");
    let c = Connection::open(db).unwrap();
    assert_eq!(
        c.query_row(
            "SELECT status FROM investment_document_events WHERE id='return'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "pending_review"
    );
}

#[test]
fn incompatible_allocation_and_late_provenance_failure_roll_back_every_write() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("x.sqlite");
    let db = path.to_str().unwrap();
    let c = seed(db);
    event(&c, "opening", "cdt_opening", "2026-06-01", 100000);
    drop(c);
    let wrong=json!({"action":"constitution","allocation_id":"other-allocation","maturity_date":"2026-12-01","agreed_rate":null,"payment_mode":null,"payment_periodicity":null,"renewal_terms":null,"contract_identifier":null,"allows_partial_redemption":false}).to_string();
    let failed = output(&[
        "investment-documents",
        "enrich-cdt",
        "opening",
        "--db",
        db,
        "--request-json",
        &wrong,
        "--apply",
        "--json",
    ]);
    assert!(!failed.status.success());
    let failed: Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(failed["errors"][0]["code"], "cdt_target_incompatible");
    let c = Connection::open(db).unwrap();
    c.execute(
        "DELETE FROM provenance WHERE investment_document_event_id='opening'",
        [],
    )
    .unwrap();
    drop(c);
    let request=json!({"action":"constitution","allocation_id":"allocation","maturity_date":"2026-12-01","agreed_rate":null,"payment_mode":null,"payment_periodicity":null,"renewal_terms":null,"contract_identifier":null,"allows_partial_redemption":false}).to_string();
    let failed = output(&[
        "investment-documents",
        "enrich-cdt",
        "opening",
        "--db",
        db,
        "--request-json",
        &request,
        "--apply",
        "--json",
    ]);
    assert!(!failed.status.success());
    let c = Connection::open(db).unwrap();
    for query in [
        "SELECT count(*) FROM cdt_positions",
        "SELECT count(*) FROM cdt_operation_revisions",
        "SELECT count(*) FROM investment_allocation_consumptions",
        "SELECT count(*) FROM cdt_provider_enrichments",
    ] {
        assert_eq!(
            c.query_row(query, [], |r| r.get::<_, i64>(0)).unwrap(),
            0,
            "{query}"
        );
    }
    assert_eq!(
        c.query_row(
            "SELECT status FROM investment_document_events WHERE id='opening'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "pending_review"
    );
}
