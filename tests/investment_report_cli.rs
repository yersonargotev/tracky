use rusqlite::Connection;
use std::{fs, process::Command};
use tracky::storage::apply_migrations;

fn run(args: &[&str]) -> (bool, serde_json::Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_tracky"))
        .args(args)
        .output()
        .unwrap();
    (
        output.status.success(),
        serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stderr))),
    )
}

#[test]
fn consolidated_report_separates_capital_acquisitions_returns_currencies_and_pending() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tracky.sqlite");
    let c = Connection::open(&path).unwrap();
    apply_migrations(&c).unwrap();
    c.execute_batch("INSERT INTO institutions(id,name) VALUES('inst_bank','bank'),('inst_broker','broker');
      INSERT INTO accounts(id,institution_id,label,kind,currency,is_owned) VALUES('bank_cop','inst_bank','bank','checking','COP',1),('broker_cop','inst_broker','broker','brokerage','COP',1);
      INSERT INTO investment_instruments(id,name,instrument_type,denomination_currency,provider) VALUES('stock','stock','security','COP','broker'),('usdc','USDC','dollar_referenced_digital_asset','USD','wallet');
      INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency,transaction_kind,investment_allocation_status) VALUES('contrib','bank_cop','2026-06-01','investment',-100000,'COP','investment_contribution','pending_allocation'),('contrib_usd','bank_cop','2026-06-02','digital dollar',-5000,'USD','investment_contribution','pending_allocation');
      INSERT INTO investment_allocation_revisions(id,allocation_id,revision,contribution_transaction_id,instrument_id,cash_amount_minor,cash_currency,acquired_quantity,provenance_source) VALUES('ar1','a1',1,'contrib','stock',60000,'COP','3.000000001','manual_entry'),('ar2','a2',1,'contrib_usd','usdc',5000,'USD','5000.000000000000000001','manual_entry');
      INSERT INTO investment_allocation_heads(allocation_id,current_revision_id) VALUES('a1','ar1'),('a2','ar2');
      INSERT INTO brokerage_accounts(account_id,opened_date,provenance_source) VALUES('broker_cop','2026-01-01','manual_entry');
      INSERT INTO brokerage_operation_revisions(id,operation_id,revision,account_id,operation_type,effective_date,currency,gross_amount_minor,net_cash_minor,funding_allocation_id,provenance_source) VALUES('brd','deposit',1,'broker_cop','deposit','2026-06-03','COP',60000,60000,'a1','manual_entry'),('brb','buy',1,'broker_cop','buy','2026-06-04','COP',60000,-60000,NULL,'manual_entry'),('brv','dividend',1,'broker_cop','dividend','2026-06-10','COP',1000,850,NULL,'manual_entry'),('brs','sell',1,'broker_cop','sell','2026-06-15','COP',30000,28500,NULL,'manual_entry'),('brw','withdraw',1,'broker_cop','withdrawal','2026-06-20','COP',2000,-2000,NULL,'manual_entry');
      INSERT INTO brokerage_operation_heads(operation_id,current_revision_id) VALUES('deposit','brd'),('buy','brb'),('dividend','brv'),('sell','brs'),('withdraw','brw');
      INSERT INTO investment_allocation_consumptions(allocation_id,consumer_kind,consumer_operation_id) VALUES('a1','brokerage_deposit','deposit');").unwrap();
    c.execute("UPDATE brokerage_operation_revisions SET instrument_id='stock',quantity='1',historical_cost_minor=25000,realized_result_minor=5000,fee_minor=500,withholding_minor=600,other_deductions_minor=400 WHERE id='brs'",[]).unwrap();
    c.execute_batch("INSERT INTO source_documents(id,input_name,content_sha256,mime_type,byte_size,institution_hint) VALUES('doc','x.pdf','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','application/pdf',1,'wenia'); INSERT INTO import_batches(id,source_document_id,started_at,status,candidate_count,error_count,duplicate_count,error_details_json) VALUES('batch','doc','2026-06-01','completed',1,0,0,'[]'); INSERT INTO investment_document_events(id,source_document_id,import_batch_id,provider,parser_id,parser_version,event_type,provider_effective_date,currency,amount_minor,page_number,row_index,evidence_redaction,fingerprint,status) VALUES('evt','doc','batch','wenia','p','1','deposit','2026-06-05','COP',100,1,1,'x','fp','pending_review');").unwrap();
    drop(c);
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    let (ok, j) = run(&[
        "reports",
        "investments",
        "--db",
        path.to_str().unwrap(),
        "--from",
        "2026-06-01",
        "--to",
        "2026-06-30",
        "--json",
    ]);
    assert!(ok);
    assert_eq!(j["schema_version"], "tracky.investment-report.v1");
    assert_eq!(
        j["capital_external"]["external_capital_contributed"],
        serde_json::json!([{"currency":"COP","amount_minor":100000},{"currency":"USD","amount_minor":5000}])
    );
    assert_eq!(
        j["capital_external"]["capital_withdrawn"],
        serde_json::json!([])
    );
    assert_eq!(
        j["capital_external"]["net_external_contribution"],
        j["capital_external"]["external_capital_contributed"]
    );
    assert_eq!(
        j["acquisitions_and_reinvestment"]["gross_acquisitions"][0]["amount_minor"],
        60000
    );
    assert_eq!(
        j["acquisitions_and_reinvestment"]["funded_by_external_contribution"][0]["amount_minor"],
        60000
    );
    assert_eq!(
        j["acquisitions_and_reinvestment"]["reinvestment"],
        serde_json::json!([])
    );
    assert_eq!(
        j["returns_and_income"]["net_cash"][0]["amount_minor"],
        27350
    );
    assert!(j["acquisitions_and_reinvestment"]["by_instrument"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["instrument_id"] == "usdc" && x["currency"] == "USD"));
    assert_eq!(
        j["closing_positions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["instrument_id"] == "usdc")
            .unwrap()["quantity"],
        "5000.000000000000000001"
    );
    assert_eq!(
        j["returns_and_income"]["gross_dividends"][0]["amount_minor"],
        1000
    );
    assert_eq!(
        j["returns_and_income"]["realized_results"][0]["amount_minor"],
        5000
    );
    assert_eq!(
        j["costs_and_withholding"]["fees_and_commissions"][0]["amount_minor"],
        500
    );
    assert_eq!(
        j["costs_and_withholding"]["withholding"][0]["amount_minor"],
        600
    );
    assert_eq!(
        j["costs_and_withholding"]["other_deductions"][0]["amount_minor"],
        400
    );
    assert_eq!(
        j["pending_and_reconciliation"]["allocations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        j["pending_and_reconciliation"]["provider_events"][0]["event_id"],
        "evt"
    );
    assert_eq!(before, fs::metadata(&path).unwrap().modified().unwrap());
}

#[test]
fn invalid_range_fails_without_mutating_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tracky.sqlite");
    let c = Connection::open(&path).unwrap();
    apply_migrations(&c).unwrap();
    drop(c);
    let before = fs::read(&path).unwrap();
    let (ok, j) = run(&[
        "reports",
        "investments",
        "--db",
        path.to_str().unwrap(),
        "--from",
        "2026-07-01",
        "--to",
        "2026-06-01",
        "--json",
    ]);
    assert!(!ok);
    assert_eq!(j["errors"][0]["code"], "invalid_date_range");
    assert_eq!(before, fs::read(&path).unwrap());
}

#[test]
fn report_covers_cdt_renewal_and_uses_only_applicable_closing_snapshot() {
    use tracky::reconciliation::{record, SnapshotInput, SnapshotPositionInput};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tracky.sqlite");
    let mut c = Connection::open(&path).unwrap();
    apply_migrations(&c).unwrap();
    c.execute_batch("INSERT INTO institutions(id,name) VALUES('bank','bank');
      INSERT INTO accounts(id,institution_id,label,currency,kind,is_owned) VALUES('acct','bank','CDT','COP','investment',1);
      INSERT INTO investment_instruments(id,name,instrument_type,denomination_currency,provider) VALUES('cdt','CDT','fixed_income','COP','bank');
      INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency,transaction_kind,investment_allocation_status) VALUES('capital','acct','2026-06-01','capital',-100000,'COP','investment_contribution','pending_allocation');
      INSERT INTO investment_allocation_revisions(id,allocation_id,revision,contribution_transaction_id,instrument_id,cash_amount_minor,cash_currency,acquired_quantity,provenance_source) VALUES('rev','alloc',1,'capital','cdt',100000,'COP','1','manual_entry');
      INSERT INTO investment_allocation_heads(allocation_id,current_revision_id) VALUES('alloc','rev');
      INSERT INTO cdt_positions(id,instrument_id,account_id,constituent_allocation_id) VALUES('pos','cdt','acct','alloc');
      INSERT INTO cdt_operation_revisions(id,operation_id,revision,cdt_position_id,operation_type,effective_date,currency,principal_before_minor,principal_after_minor,external_capital_minor,capitalized_interest_minor,gross_interest_minor,maturity_date,provenance_source) VALUES
      ('op1r','op1',1,'pos','constitution','2026-06-01','COP',0,100000,100000,0,0,'2026-06-10','manual_entry'),
      ('op2r','op2',1,'pos','renewal','2026-06-10','COP',100000,110000,0,10000,10000,'2026-07-10','manual_entry');
      INSERT INTO cdt_operation_heads(operation_id,current_revision_id) VALUES('op1','op1r'),('op2','op2r');
      INSERT INTO investment_allocation_consumptions(allocation_id,consumer_kind,cdt_position_id,consumer_operation_id) VALUES('alloc','cdt_constitution','pos','op1');").unwrap();
    let first = record(
        &mut c,
        SnapshotInput {
            observed_at: "2026-06-10T12:00:00Z".into(),
            provider_effective_date: Some("2026-06-10".into()),
            source: "bank".into(),
            external_reference: Some("june".into()),
            provenance_source: "synthetic_test".into(),
            positions: vec![SnapshotPositionInput {
                account_id: "acct".into(),
                instrument_id: Some("cdt".into()),
                quantity: Some("1".into()),
                currency: "COP".into(),
                observed_cash_minor: None,
                observed_value_minor: Some(110000),
                valuation_currency: Some("COP".into()),
                observed_price: None,
            }],
        },
    )
    .unwrap();
    assert!(first.ok);
    let future = record(
        &mut c,
        SnapshotInput {
            observed_at: "2026-07-05T12:00:00Z".into(),
            provider_effective_date: Some("2026-07-05".into()),
            source: "bank".into(),
            external_reference: Some("july".into()),
            provenance_source: "synthetic_test".into(),
            positions: vec![SnapshotPositionInput {
                account_id: "acct".into(),
                instrument_id: Some("cdt".into()),
                quantity: Some("1".into()),
                currency: "COP".into(),
                observed_cash_minor: None,
                observed_value_minor: Some(999000),
                valuation_currency: Some("COP".into()),
                observed_price: None,
            }],
        },
    )
    .unwrap();
    assert!(future.ok);
    drop(c);
    let (ok, j) = run(&[
        "reports",
        "investments",
        "--db",
        path.to_str().unwrap(),
        "--from",
        "2026-06-01",
        "--to",
        "2026-06-30",
        "--json",
    ]);
    assert!(ok);
    assert_eq!(
        j["acquisitions_and_reinvestment"]["gross_acquisitions"][0]["amount_minor"],
        210000
    );
    assert_eq!(
        j["acquisitions_and_reinvestment"]["funded_by_maturities"][0]["amount_minor"],
        100000
    );
    assert_eq!(
        j["acquisitions_and_reinvestment"]["reinvestment"][0]["amount_minor"],
        110000
    );
    assert_eq!(
        j["returns_and_income"]["gross_interest"][0]["amount_minor"],
        10000
    );
    let p = &j["closing_positions"][0];
    assert_eq!(p["observed_value_minor"], 110000);
    assert_eq!(p["freshness"], "stale");
    assert_eq!(p["original_status"], "quantity_mismatch");
    assert_eq!(p["reconciliation_status"], "stale");
}
