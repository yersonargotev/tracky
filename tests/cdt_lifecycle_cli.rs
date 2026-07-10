use serde_json::Value;
use std::process::{Command, Output};

fn tracky() -> &'static str {
    env!("CARGO_BIN_EXE_tracky")
}

fn output(args: &[&str]) -> Output {
    Command::new(tracky())
        .args(args)
        .output()
        .expect("run tracky")
}

fn run(args: &[&str]) -> Value {
    let output = output(args);
    assert!(
        output.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).expect("JSON response")
}

fn run_error(args: &[&str]) -> Value {
    let output = output(args);
    assert!(!output.status.success(), "command unexpectedly succeeded");
    serde_json::from_slice(&output.stdout).expect("JSON error response")
}

fn register_account(db: &str) -> String {
    run(&[
        "accounts",
        "register",
        "--db",
        db,
        "--institution",
        "Synthetic bank",
        "--label",
        "Synthetic CDT funding",
        "--account-type",
        "savings",
        "--currency",
        "COP",
        "--json",
    ])["account"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn fixed_income_instrument(db: &str) -> String {
    run(&[
        "instruments",
        "create",
        "--db",
        db,
        "--name",
        "Synthetic 180-day CDT",
        "--type",
        "fixed_income",
        "--denomination-currency",
        "COP",
        "--provider",
        "Synthetic bank",
        "--provider-identifier",
        "CDT-SYNTHETIC-180",
        "--json",
    ])["instrument"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn allocated_contribution(db: &str, account_id: &str, instrument_id: &str, amount: i64) -> String {
    let amount_text = format!("-{amount}");
    let quantity = format!("{}.{:02}", amount / 100, amount % 100);
    let contribution = run(&[
        "transactions",
        "add-investment",
        "--db",
        db,
        "--account-id",
        account_id,
        "--posted-date",
        "2026-01-10",
        "--description",
        "Synthetic CDT contribution",
        "--amount-minor",
        &amount_text,
        "--currency",
        "COP",
        "--json",
    ])["canonical_transactions"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    run(&[
        "investments",
        "allocate",
        "--db",
        db,
        "--contribution-id",
        &contribution,
        "--instrument-id",
        instrument_id,
        "--cash-amount-minor",
        &amount.to_string(),
        "--cash-currency",
        "COP",
        "--quantity",
        &quantity,
        "--json",
    ])["allocations"][0]["allocation_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn constitute(
    db: &str,
    allocation_id: &str,
    principal_minor: i64,
    maturity_date: &str,
    allows_partial_redemption: bool,
) -> Value {
    let principal = principal_minor.to_string();
    let allows_partial = allows_partial_redemption.to_string();
    run(&[
        "cdts",
        "constitute",
        "--db",
        db,
        "--allocation-id",
        allocation_id,
        "--principal-minor",
        &principal,
        "--currency",
        "COP",
        "--constitution-date",
        "2026-01-10",
        "--maturity-date",
        maturity_date,
        "--agreed-rate",
        "10.5",
        "--allows-partial-redemption",
        &allows_partial,
        "--json",
    ])
}

#[test]
fn cdt_constitution_consumes_fixed_income_allocation_and_exposes_exact_terms() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();
    let account = register_account(db);
    let instrument = fixed_income_instrument(db);
    let allocation = allocated_contribution(db, &account, &instrument, 10_000_000);

    let constituted = run(&[
        "cdts",
        "constitute",
        "--db",
        db,
        "--allocation-id",
        &allocation,
        "--principal-minor",
        "10000000",
        "--currency",
        "COP",
        "--constitution-date",
        "2026-01-10",
        "--maturity-date",
        "2026-07-10",
        "--agreed-rate",
        "10.5000",
        "--payment-mode",
        "at_maturity",
        "--payment-periodicity",
        "once",
        "--renewal-terms",
        "manual",
        "--contract-identifier",
        "SYN-001",
        "--json",
    ]);

    assert_eq!(constituted["schema_version"], "tracky.cdts.v1");
    assert_eq!(constituted["command"], "cdts constitute");
    assert_eq!(constituted["position"]["instrument_id"], instrument);
    assert_eq!(constituted["position"]["account_id"], account);
    assert_eq!(
        constituted["position"]["current_principal_minor"],
        10_000_000
    );
    assert_eq!(constituted["position"]["currency"], "COP");
    assert_eq!(constituted["position"]["status"], "active");
    assert_eq!(
        constituted["position"]["constitution_allocation_id"],
        allocation
    );
    assert!(constituted["position"]["constitution_contribution_id"]
        .as_str()
        .is_some());
    assert_eq!(
        constituted["position"]["current_terms"]["agreed_rate"],
        "10.5"
    );
    assert_eq!(
        constituted["position"]["current_terms"]["contract_identifier"],
        "SYN-001"
    );
    assert_eq!(
        constituted["operation_history"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        constituted["operation_history"][0]["provenance_source"],
        "manual_entry"
    );

    let listed = run(&[
        "cdts",
        "list",
        "--db",
        db,
        "--as-of",
        "2026-07-09",
        "--json",
    ]);
    assert_eq!(listed["positions"].as_array().unwrap().len(), 1);
    let inspected = run(&[
        "cdts",
        "inspect",
        "--db",
        db,
        "--position-id",
        constituted["position"]["id"].as_str().unwrap(),
        "--as-of",
        "2026-07-10",
        "--json",
    ]);
    assert_eq!(inspected["position"]["status"], "matured");
}

#[test]
fn cdt_renewal_and_redemption_separate_capital_interest_deductions_and_cash() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();
    let account = register_account(db);
    let instrument = fixed_income_instrument(db);
    let initial_allocation = allocated_contribution(db, &account, &instrument, 10_000_000);
    let cdt = constitute(db, &initial_allocation, 10_000_000, "2026-07-10", true);
    let position_id = cdt["position"]["id"].as_str().unwrap();

    let added_allocation = allocated_contribution(db, &account, &instrument, 2_000_000);
    let renewed = run(&[
        "cdts",
        "renew",
        "--db",
        db,
        "--position-id",
        position_id,
        "--effective-date",
        "2026-07-10",
        "--additional-allocation-id",
        &added_allocation,
        "--external-capital-minor",
        "2000000",
        "--gross-interest-minor",
        "800000",
        "--capitalized-interest-minor",
        "500000",
        "--withholding-minor",
        "50000",
        "--other-deductions-minor",
        "10000",
        "--net-cash-received-minor",
        "240000",
        "--deduction-component-id",
        "fee_cdt_renewal_001",
        "--maturity-date",
        "2027-01-10",
        "--agreed-rate",
        "10.750000",
        "--allows-partial-redemption",
        "true",
        "--json",
    ]);
    assert_eq!(renewed["position"]["status"], "renewed");
    assert_eq!(renewed["position"]["current_principal_minor"], 12_500_000);
    assert_eq!(
        renewed["operations"][1]["external_capital_minor"],
        2_000_000
    );
    assert_eq!(
        renewed["operations"][1]["capitalized_interest_minor"],
        500_000
    );
    assert_eq!(renewed["operations"][1]["gross_interest_minor"], 800_000);
    assert_eq!(renewed["operations"][1]["net_cash_received_minor"], 240_000);
    assert_eq!(renewed["operations"][1]["terms"]["agreed_rate"], "10.75");
    let category = run(&[
        "categories",
        "create",
        "--db",
        db,
        "--name",
        "Synthetic CDT deductions",
        "--json",
    ])["category"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let duplicated_deduction = run_error(&[
        "transactions",
        "add-expense",
        "--db",
        db,
        "--account-id",
        &account,
        "--posted-date",
        "2026-07-10",
        "--description",
        "Attempted duplicate CDT deduction",
        "--amount-minor",
        "-10000",
        "--currency",
        "COP",
        "--category-id",
        &category,
        "--investment-fee-component-id",
        "fee_cdt_renewal_001",
        "--json",
    ]);
    assert_eq!(
        duplicated_deduction["errors"][0]["code"],
        "fee_double_count_conflict"
    );

    let redeemed = run(&[
        "cdts",
        "redeem",
        "--db",
        db,
        "--position-id",
        position_id,
        "--effective-date",
        "2027-01-10",
        "--principal-returned-minor",
        "5000000",
        "--gross-interest-minor",
        "600000",
        "--withholding-minor",
        "42000",
        "--other-deductions-minor",
        "8000",
        "--net-cash-received-minor",
        "5550000",
        "--deduction-component-id",
        "fee_cdt_redemption_001",
        "--json",
    ]);
    assert_eq!(redeemed["position"]["current_principal_minor"], 7_500_000);
    assert_eq!(redeemed["position"]["status"], "matured");
    assert_eq!(
        redeemed["operations"][2]["principal_returned_minor"],
        5_000_000
    );
    assert_eq!(redeemed["operations"][2]["gross_interest_minor"], 600_000);
    assert_eq!(redeemed["operations"][2]["withholding_minor"], 42_000);
    assert_eq!(redeemed["operations"][2]["other_deductions_minor"], 8_000);
    assert_eq!(
        redeemed["operations"][2]["net_cash_received_minor"],
        5_550_000
    );

    let report = run(&[
        "reports",
        "summary",
        "--db",
        db,
        "--start-date",
        "2026-01-01",
        "--end-date",
        "2027-12-31",
        "--json",
    ]);
    assert_eq!(
        report["investment_contribution_totals"][0]["total_contributed_minor"],
        12_000_000
    );
    assert_eq!(report["totals"].as_array().unwrap().len(), 0);
}

#[test]
fn unchanged_principal_renewal_is_not_a_new_contribution_and_corrections_are_append_only() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();
    let account = register_account(db);
    let instrument = fixed_income_instrument(db);
    let allocation = allocated_contribution(db, &account, &instrument, 10_000_000);
    let cdt = constitute(db, &allocation, 10_000_000, "2026-07-10", false);
    let position_id = cdt["position"]["id"].as_str().unwrap();
    let consumed_replacement = run_error(&[
        "investments",
        "replace-allocation",
        "--db",
        db,
        "--allocation-id",
        &allocation,
        "--instrument-id",
        &instrument,
        "--cash-amount-minor",
        "9000000",
        "--cash-currency",
        "COP",
        "--quantity",
        "90000",
        "--reason",
        "Attempt to rewrite consumed CDT funding",
        "--json",
    ]);
    assert_eq!(
        consumed_replacement["errors"][0]["code"],
        "allocation_consumed_by_cdt"
    );

    let renewed = run(&[
        "cdts",
        "renew",
        "--db",
        db,
        "--position-id",
        position_id,
        "--effective-date",
        "2026-07-10",
        "--gross-interest-minor",
        "200000",
        "--withholding-minor",
        "10000",
        "--net-cash-received-minor",
        "190000",
        "--maturity-date",
        "2027-01-10",
        "--agreed-rate",
        "11.00",
        "--json",
    ]);
    assert_eq!(renewed["position"]["current_principal_minor"], 10_000_000);
    assert_eq!(renewed["operations"][1]["external_capital_minor"], 0);
    let renewal_operation_id = renewed["operations"][1]["operation_id"].as_str().unwrap();
    let replacement = serde_json::json!({
        "effective_date": "2026-07-10",
        "principal_before_minor": 10_000_000,
        "principal_after_minor": 10_000_000,
        "principal_returned_minor": 0,
        "external_capital_minor": 0,
        "capitalized_interest_minor": 0,
        "gross_interest_minor": 250_000,
        "withholding_minor": 10_000,
        "other_deductions_minor": 0,
        "net_cash_received_minor": 240_000,
        "funding_allocation_id": null,
        "terms": {
            "maturity_date": "2027-01-10",
            "agreed_rate": "11.00",
            "payment_mode": null,
            "payment_periodicity": null,
            "renewal_terms": null,
            "contract_identifier": null,
            "allows_partial_redemption": false
        },
        "deduction_component_id": null,
        "deduction_expense_transaction_id": null
    })
    .to_string();
    let corrected = run(&[
        "cdts",
        "replace-operation",
        "--db",
        db,
        "--operation-id",
        renewal_operation_id,
        "--reason",
        "Correct synthetic certificate interest",
        "--replacement-json",
        &replacement,
        "--json",
    ]);
    assert_eq!(corrected["operations"][1]["revision"], 2);
    assert_eq!(corrected["operations"][1]["gross_interest_minor"], 250_000);
    assert_eq!(
        corrected["operations"][1]["correction_reason"],
        "Correct synthetic certificate interest"
    );
    assert!(corrected["operations"][1]["replaces_revision_id"]
        .as_str()
        .is_some());
    assert_eq!(corrected["operation_history"].as_array().unwrap().len(), 3);

    let report = run(&[
        "reports",
        "summary",
        "--db",
        db,
        "--start-date",
        "2026-01-01",
        "--end-date",
        "2027-12-31",
        "--json",
    ]);
    assert_eq!(
        report["investment_contribution_totals"][0]["total_contributed_minor"],
        10_000_000
    );
    assert_eq!(report["totals"].as_array().unwrap().len(), 0);
}

#[test]
fn invalid_cdt_events_and_duplicate_redemption_are_atomic() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();
    let account = register_account(db);
    let instrument = fixed_income_instrument(db);
    let allocation = allocated_contribution(db, &account, &instrument, 10_000_000);

    let invalid_date = run_error(&[
        "cdts",
        "constitute",
        "--db",
        db,
        "--allocation-id",
        &allocation,
        "--principal-minor",
        "10000000",
        "--currency",
        "COP",
        "--constitution-date",
        "2026-02-30",
        "--maturity-date",
        "2026-07-10",
        "--json",
    ]);
    assert_eq!(invalid_date["errors"][0]["code"], "invalid_effective_date");
    let mismatch = run_error(&[
        "cdts",
        "constitute",
        "--db",
        db,
        "--allocation-id",
        &allocation,
        "--principal-minor",
        "9999999",
        "--currency",
        "USD",
        "--constitution-date",
        "2026-01-10",
        "--maturity-date",
        "2026-07-10",
        "--json",
    ]);
    assert_eq!(mismatch["errors"][0]["code"], "currency_mismatch");
    assert_eq!(
        run(&[
            "cdts",
            "list",
            "--db",
            db,
            "--as-of",
            "2026-01-10",
            "--json"
        ])["positions"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let cdt = constitute(db, &allocation, 10_000_000, "2026-07-10", false);
    let position_id = cdt["position"]["id"].as_str().unwrap();
    let unreconciled = run_error(&[
        "cdts",
        "renew",
        "--db",
        db,
        "--position-id",
        position_id,
        "--effective-date",
        "2026-07-10",
        "--external-capital-minor",
        "1000000",
        "--maturity-date",
        "2027-01-10",
        "--json",
    ]);
    assert_eq!(
        unreconciled["errors"][0]["code"],
        "additional_capital_not_reconciled"
    );
    let bad_net = run_error(&[
        "cdts",
        "renew",
        "--db",
        db,
        "--position-id",
        position_id,
        "--effective-date",
        "2026-07-10",
        "--gross-interest-minor",
        "100000",
        "--withholding-minor",
        "10000",
        "--net-cash-received-minor",
        "89999",
        "--maturity-date",
        "2027-01-10",
        "--json",
    ]);
    assert_eq!(bad_net["errors"][0]["code"], "net_cash_mismatch");
    let after_failed_renewals = run(&[
        "cdts",
        "inspect",
        "--db",
        db,
        "--position-id",
        position_id,
        "--as-of",
        "2026-07-10",
        "--json",
    ]);
    assert_eq!(
        after_failed_renewals["operations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let partial = run_error(&[
        "cdts",
        "redeem",
        "--db",
        db,
        "--position-id",
        position_id,
        "--effective-date",
        "2026-07-10",
        "--principal-returned-minor",
        "5000000",
        "--net-cash-received-minor",
        "5000000",
        "--json",
    ]);
    assert_eq!(
        partial["errors"][0]["code"],
        "partial_redemption_not_allowed"
    );
    let over = run_error(&[
        "cdts",
        "redeem",
        "--db",
        db,
        "--position-id",
        position_id,
        "--effective-date",
        "2026-07-10",
        "--principal-returned-minor",
        "10000001",
        "--net-cash-received-minor",
        "10000001",
        "--json",
    ]);
    assert_eq!(over["errors"][0]["code"], "invalid_principal_returned");
    let redeemed = run(&[
        "cdts",
        "redeem",
        "--db",
        db,
        "--position-id",
        position_id,
        "--effective-date",
        "2026-07-10",
        "--principal-returned-minor",
        "10000000",
        "--gross-interest-minor",
        "500000",
        "--withholding-minor",
        "35000",
        "--net-cash-received-minor",
        "10465000",
        "--json",
    ]);
    assert_eq!(redeemed["position"]["status"], "redeemed");
    let duplicate = run_error(&[
        "cdts",
        "redeem",
        "--db",
        db,
        "--position-id",
        position_id,
        "--effective-date",
        "2026-07-10",
        "--principal-returned-minor",
        "10000000",
        "--gross-interest-minor",
        "500000",
        "--withholding-minor",
        "35000",
        "--net-cash-received-minor",
        "10465000",
        "--json",
    ]);
    assert_eq!(duplicate["errors"][0]["code"], "duplicate_redemption");
    let final_state = run(&[
        "cdts",
        "inspect",
        "--db",
        db,
        "--position-id",
        position_id,
        "--as-of",
        "2026-07-10",
        "--json",
    ]);
    assert_eq!(final_state["operations"].as_array().unwrap().len(), 2);
}

#[test]
fn cdt_rejects_incompatible_instruments_rates_and_reused_fee_identities() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();
    let account = register_account(db);
    let fixed_income = fixed_income_instrument(db);
    let allocation = allocated_contribution(db, &account, &fixed_income, 10_000_000);
    let invalid_rate = run_error(&[
        "cdts",
        "constitute",
        "--db",
        db,
        "--allocation-id",
        &allocation,
        "--principal-minor",
        "10000000",
        "--currency",
        "COP",
        "--constitution-date",
        "2026-01-10",
        "--maturity-date",
        "2026-07-10",
        "--agreed-rate",
        "1.05e1",
        "--json",
    ]);
    assert_eq!(invalid_rate["errors"][0]["code"], "invalid_agreed_rate");

    let generic = run(&[
        "instruments",
        "create",
        "--db",
        db,
        "--name",
        "Synthetic generic asset",
        "--type",
        "generic",
        "--denomination-currency",
        "COP",
        "--provider",
        "Synthetic bank",
        "--provider-identifier",
        "GENERIC-1",
        "--json",
    ])["instrument"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let generic_allocation = allocated_contribution(db, &account, &generic, 1_000_000);
    let incompatible = run_error(&[
        "cdts",
        "constitute",
        "--db",
        db,
        "--allocation-id",
        &generic_allocation,
        "--principal-minor",
        "1000000",
        "--currency",
        "COP",
        "--constitution-date",
        "2026-01-10",
        "--maturity-date",
        "2026-07-10",
        "--json",
    ]);
    assert_eq!(
        incompatible["errors"][0]["code"],
        "instrument_not_fixed_income"
    );

    let fee_contribution = run(&[
        "transactions",
        "add-investment",
        "--db",
        db,
        "--account-id",
        &account,
        "--posted-date",
        "2026-01-10",
        "--description",
        "Fee-funded synthetic CDT",
        "--amount-minor",
        "-2000000",
        "--currency",
        "COP",
        "--json",
    ])["canonical_transactions"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fee_allocation = run(&[
        "investments",
        "allocate",
        "--db",
        db,
        "--contribution-id",
        &fee_contribution,
        "--instrument-id",
        &fixed_income,
        "--cash-amount-minor",
        "2000000",
        "--cash-currency",
        "COP",
        "--quantity",
        "20000",
        "--fee-amount-minor",
        "10000",
        "--fee-currency",
        "COP",
        "--fee-treatment",
        "capitalized",
        "--fee-component-id",
        "component_shared_with_cdt",
        "--json",
    ])["allocations"][0]["allocation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let cdt = constitute(db, &fee_allocation, 2_000_000, "2026-07-10", false);
    let position_id = cdt["position"]["id"].as_str().unwrap();
    let reused = run_error(&[
        "cdts",
        "renew",
        "--db",
        db,
        "--position-id",
        position_id,
        "--effective-date",
        "2026-07-10",
        "--gross-interest-minor",
        "100000",
        "--other-deductions-minor",
        "10000",
        "--net-cash-received-minor",
        "90000",
        "--deduction-component-id",
        "component_shared_with_cdt",
        "--maturity-date",
        "2027-01-10",
        "--json",
    ]);
    assert_eq!(
        reused["errors"][0]["code"],
        "deduction_double_count_conflict"
    );
    let inspected = run(&[
        "cdts",
        "inspect",
        "--db",
        db,
        "--position-id",
        position_id,
        "--as-of",
        "2026-07-10",
        "--json",
    ]);
    assert_eq!(inspected["operations"].as_array().unwrap().len(), 1);
}
