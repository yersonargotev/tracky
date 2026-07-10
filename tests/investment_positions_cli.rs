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

fn register_account(db: &str, currency: &str) -> String {
    register_account_named(db, currency, "Synthetic investment source")
}

fn register_account_named(db: &str, currency: &str, label: &str) -> String {
    run(&[
        "accounts",
        "register",
        "--db",
        db,
        "--institution",
        "Synthetic bank",
        "--label",
        label,
        "--account-type",
        "wallet",
        "--currency",
        currency,
        "--json",
    ])["account"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn add_contribution(db: &str, account_id: &str, amount_minor: &str) -> String {
    add_contribution_named(db, account_id, amount_minor, "Synthetic contribution")
}

fn add_contribution_named(
    db: &str,
    account_id: &str,
    amount_minor: &str,
    description: &str,
) -> String {
    run(&[
        "transactions",
        "add-investment",
        "--db",
        db,
        "--account-id",
        account_id,
        "--posted-date",
        "2026-07-10",
        "--description",
        description,
        "--amount-minor",
        amount_minor,
        "--currency",
        "COP",
        "--json",
    ])["canonical_transactions"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn add_fee_expense(
    db: &str,
    account_id: &str,
    amount_minor: &str,
    fee_component_id: &str,
) -> String {
    let category = run(&[
        "categories",
        "create",
        "--db",
        db,
        "--name",
        "Synthetic investment fees",
        "--json",
    ])["category"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    run(&[
        "transactions",
        "add-expense",
        "--db",
        db,
        "--account-id",
        account_id,
        "--posted-date",
        "2026-07-10",
        "--description",
        "Synthetic investment fee",
        "--amount-minor",
        amount_minor,
        "--currency",
        "COP",
        "--category-id",
        &category,
        "--investment-fee-component-id",
        fee_component_id,
        "--json",
    ])["canonical_transactions"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn create_instrument(
    db: &str,
    name: &str,
    instrument_type: &str,
    denomination_currency: &str,
    provider_identifier: Option<&str>,
) -> Value {
    let mut args = vec![
        "instruments",
        "create",
        "--db",
        db,
        "--name",
        name,
        "--type",
        instrument_type,
        "--denomination-currency",
        denomination_currency,
        "--provider",
        "Synthetic provider",
    ];
    if let Some(identifier) = provider_identifier {
        args.extend(["--provider-identifier", identifier]);
    }
    args.push("--json");
    run(&args)
}

#[test]
fn instruments_create_list_and_inspect_keep_unlike_assets_distinct() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();

    let usd = create_instrument(db, "US dollar cash", "fiat_currency", "USD", Some("USD"));
    let usdc = create_instrument(
        db,
        "USD Coin",
        "dollar_referenced_digital_asset",
        "USD",
        Some("USDC"),
    );
    let copw = create_instrument(
        db,
        "COPW",
        "dollar_referenced_digital_asset",
        "COP",
        Some("COPW"),
    );
    let _cdt = create_instrument(
        db,
        "Synthetic fixed income",
        "fixed_income",
        "COP",
        Some("CDT-SYNTHETIC"),
    );

    assert_eq!(usd["schema_version"], "tracky.investments.v1");
    assert_ne!(usd["instrument"]["id"], usdc["instrument"]["id"]);
    assert_ne!(usdc["instrument"]["id"], copw["instrument"]["id"]);
    assert_eq!(usd["instrument"]["instrument_type"], "fiat_currency");
    assert_eq!(usdc["instrument"]["denomination_currency"], "USD");

    let listed = run(&["instruments", "list", "--db", db, "--json"]);
    assert_eq!(listed["instruments"].as_array().unwrap().len(), 4);

    let usd_id = usd["instrument"]["id"].as_str().unwrap();
    let inspected = run(&[
        "instruments",
        "inspect",
        "--db",
        db,
        "--instrument-id",
        usd_id,
        "--json",
    ]);
    assert_eq!(inspected["instrument"]["provider_identifier"], "USD");
}

#[test]
fn contribution_allocations_preserve_exact_cross_currency_quantities_fees_and_positions() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();
    let account = register_account(db, "COP");
    let contribution = add_contribution(db, &account, "-1000000");
    let separate_fee_expense = add_fee_expense(db, &account, "-500", "fee_component_usdc_purchase");
    let usd = create_instrument(db, "US dollar cash", "fiat_currency", "USD", Some("USD"));
    let usdc = create_instrument(
        db,
        "USD Coin",
        "dollar_referenced_digital_asset",
        "USD",
        Some("USDC"),
    );
    let usd_id = usd["instrument"]["id"].as_str().unwrap();
    let usdc_id = usdc["instrument"]["id"].as_str().unwrap();

    let partial = run(&[
        "investments",
        "allocate",
        "--db",
        db,
        "--contribution-id",
        &contribution,
        "--instrument-id",
        usd_id,
        "--cash-amount-minor",
        "600000",
        "--cash-currency",
        "COP",
        "--quantity",
        "150.1250",
        "--fee-amount-minor",
        "1000",
        "--fee-currency",
        "COP",
        "--fee-treatment",
        "capitalized",
        "--fee-component-id",
        "fee_component_usd_purchase",
        "--json",
    ]);
    assert_eq!(partial["allocation_status"], "partially_allocated");
    assert_eq!(partial["unallocated_amount_minor"], 400000);
    assert_eq!(partial["allocations"][0]["acquired_quantity"], "150.125");
    assert_eq!(partial["allocations"][0]["cash_currency"], "COP");
    assert_eq!(partial["allocations"][0]["fee_amount_minor"], 1000);
    assert_eq!(
        partial["allocations"][0]["provenance_source"],
        "manual_entry"
    );
    assert_eq!(
        partial["allocations"][0]["effective_rate"]["cost_minor_numerator"],
        600000
    );
    assert_eq!(
        partial["allocations"][0]["effective_rate"]["quantity_denominator"],
        "150.125"
    );
    let fee_category = run(&["categories", "list", "--db", db, "--json"])["categories"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let duplicate_capitalized_expense = output(&[
        "transactions",
        "add-expense",
        "--db",
        db,
        "--account-id",
        &account,
        "--posted-date",
        "2026-07-10",
        "--description",
        "Duplicate capitalized fee",
        "--amount-minor",
        "-1000",
        "--currency",
        "COP",
        "--category-id",
        &fee_category,
        "--investment-fee-component-id",
        "fee_component_usd_purchase",
        "--json",
    ]);
    assert!(!duplicate_capitalized_expense.status.success());
    let duplicate_capitalized_json: Value =
        serde_json::from_slice(&duplicate_capitalized_expense.stdout).unwrap();
    assert_eq!(
        duplicate_capitalized_json["errors"][0]["code"],
        "fee_double_count_conflict"
    );

    let complete = run(&[
        "investments",
        "allocate",
        "--db",
        db,
        "--contribution-id",
        &contribution,
        "--instrument-id",
        usdc_id,
        "--cash-amount-minor",
        "400000",
        "--cash-currency",
        "COP",
        "--quantity",
        "100.00000001",
        "--fee-amount-minor",
        "500",
        "--fee-currency",
        "COP",
        "--fee-treatment",
        "separate",
        "--fee-component-id",
        "fee_component_usdc_purchase",
        "--fee-expense-transaction-id",
        &separate_fee_expense,
        "--json",
    ]);
    assert_eq!(complete["allocation_status"], "fully_allocated");
    assert_eq!(complete["unallocated_amount_minor"], 0);
    assert_eq!(complete["allocations"].as_array().unwrap().len(), 2);
    let usdc_allocation_id = complete["allocations"][1]["allocation_id"]
        .as_str()
        .unwrap();
    let incompatible_fee_replacement = output(&[
        "investments",
        "replace-allocation",
        "--db",
        db,
        "--allocation-id",
        usdc_allocation_id,
        "--instrument-id",
        usdc_id,
        "--cash-amount-minor",
        "400000",
        "--cash-currency",
        "COP",
        "--quantity",
        "100.00000001",
        "--fee-amount-minor",
        "500",
        "--fee-currency",
        "COP",
        "--fee-treatment",
        "capitalized",
        "--fee-component-id",
        "fee_component_usdc_purchase",
        "--reason",
        "Invalid fee treatment change",
        "--json",
    ]);
    assert!(!incompatible_fee_replacement.status.success());
    let incompatible_fee_json: Value =
        serde_json::from_slice(&incompatible_fee_replacement.stdout).unwrap();
    assert_eq!(
        incompatible_fee_json["errors"][0]["code"],
        "fee_double_count_conflict"
    );

    let positions = run(&[
        "investments",
        "positions",
        "--db",
        db,
        "--account-id",
        &account,
        "--json",
    ]);
    let positions = positions["positions"].as_array().unwrap();
    assert_eq!(positions.len(), 2);
    let usd_position = positions
        .iter()
        .find(|position| position["instrument_id"] == usd_id)
        .unwrap();
    assert_eq!(usd_position["quantity"], "150.125");
    assert_eq!(usd_position["accumulated_cost_minor"], 601000);
    assert_eq!(usd_position["cost_currency"], "COP");
    assert!(usd_position["latest_contributing_operation_id"]
        .as_str()
        .unwrap()
        .starts_with("allocrev_"));
    let usdc_position = positions
        .iter()
        .find(|position| position["instrument_id"] == usdc_id)
        .unwrap();
    assert_eq!(usdc_position["accumulated_cost_minor"], 400000);

    let inspected = run(&[
        "transactions",
        "inspect",
        "--db",
        db,
        &contribution,
        "--json",
    ]);
    assert_eq!(
        inspected["canonical_transaction"]["investment_allocation_status"],
        "fully_allocated"
    );
    assert_eq!(inspected["provenance"][0]["source"], "manual_entry");

    let duplicate_fee_contribution = add_contribution(db, &account, "-100");
    let duplicate_fee = output(&[
        "investments",
        "allocate",
        "--db",
        db,
        "--contribution-id",
        &duplicate_fee_contribution,
        "--instrument-id",
        usd_id,
        "--cash-amount-minor",
        "100",
        "--cash-currency",
        "COP",
        "--quantity",
        "0.01",
        "--fee-amount-minor",
        "500",
        "--fee-currency",
        "COP",
        "--fee-treatment",
        "separate",
        "--fee-component-id",
        "fee_component_usdc_purchase",
        "--fee-expense-transaction-id",
        &separate_fee_expense,
        "--json",
    ]);
    assert!(!duplicate_fee.status.success());
    let duplicate_fee_json: Value = serde_json::from_slice(&duplicate_fee.stdout).unwrap();
    assert_eq!(
        duplicate_fee_json["errors"][0]["code"],
        "fee_double_count_conflict"
    );
}

#[test]
fn allocation_replacements_are_append_only_and_invalid_changes_are_atomic() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();
    let account = register_account(db, "COP");
    let contribution = add_contribution(db, &account, "-1000");
    let instrument = create_instrument(db, "Generic unit", "generic", "USD", None);
    let instrument_id = instrument["instrument"]["id"].as_str().unwrap();

    let first = run(&[
        "investments",
        "allocate",
        "--db",
        db,
        "--contribution-id",
        &contribution,
        "--instrument-id",
        instrument_id,
        "--cash-amount-minor",
        "600",
        "--cash-currency",
        "COP",
        "--quantity",
        "1.25",
        "--json",
    ]);
    let allocation_id = first["allocations"][0]["allocation_id"].as_str().unwrap();

    let replaced = run(&[
        "investments",
        "replace-allocation",
        "--db",
        db,
        "--allocation-id",
        allocation_id,
        "--instrument-id",
        instrument_id,
        "--cash-amount-minor",
        "700",
        "--cash-currency",
        "COP",
        "--quantity",
        "1.5",
        "--reason",
        "Synthetic correction",
        "--json",
    ]);
    assert_eq!(replaced["unallocated_amount_minor"], 300);
    assert_eq!(replaced["allocations"][0]["revision"], 2);
    assert_eq!(replaced["allocation_history"].as_array().unwrap().len(), 2);
    assert_eq!(
        replaced["allocation_history"][1]["correction_reason"],
        "Synthetic correction"
    );
    assert!(replaced["allocation_history"]
        .as_array()
        .unwrap()
        .iter()
        .all(|revision| revision["provenance_source"] == "manual_entry"));

    let before_count = replaced["allocation_history"].as_array().unwrap().len();
    let invalid = output(&[
        "investments",
        "replace-allocation",
        "--db",
        db,
        "--allocation-id",
        allocation_id,
        "--instrument-id",
        instrument_id,
        "--cash-amount-minor",
        "1001",
        "--cash-currency",
        "COP",
        "--quantity",
        "2",
        "--reason",
        "Invalid over-allocation",
        "--json",
    ]);
    assert!(!invalid.status.success());
    let invalid_json: Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert_eq!(
        invalid_json["errors"][0]["code"],
        "contribution_overallocated"
    );

    let inspected = run(&[
        "investments",
        "inspect-contribution",
        "--db",
        db,
        "--contribution-id",
        &contribution,
        "--json",
    ]);
    assert_eq!(
        inspected["allocation_history"].as_array().unwrap().len(),
        before_count
    );
    assert_eq!(inspected["unallocated_amount_minor"], 300);

    for (quantity, instrument_id, code) in [
        ("0", instrument_id, "invalid_quantity"),
        ("1e3", instrument_id, "invalid_quantity"),
        ("1", "instr_missing", "instrument_not_found"),
    ] {
        let invalid = output(&[
            "investments",
            "allocate",
            "--db",
            db,
            "--contribution-id",
            &contribution,
            "--instrument-id",
            instrument_id,
            "--cash-amount-minor",
            "1",
            "--cash-currency",
            "COP",
            "--quantity",
            quantity,
            "--json",
        ]);
        assert!(!invalid.status.success());
        let json: Value = serde_json::from_slice(&invalid.stdout).unwrap();
        assert_eq!(json["errors"][0]["code"], code);
    }

    for (extra_args, code) in [
        (
            vec!["--cash-currency", "USD", "--quantity", "1"],
            "cash_currency_mismatch",
        ),
        (
            vec![
                "--cash-currency",
                "COP",
                "--quantity",
                "1",
                "--fee-amount-minor",
                "1",
            ],
            "invalid_fee",
        ),
    ] {
        let mut args = vec![
            "investments",
            "allocate",
            "--db",
            db,
            "--contribution-id",
            &contribution,
            "--instrument-id",
            instrument_id,
            "--cash-amount-minor",
            "1",
        ];
        args.extend(extra_args);
        args.push("--json");
        let invalid = output(&args);
        assert!(!invalid.status.success());
        let json: Value = serde_json::from_slice(&invalid.stdout).unwrap();
        assert_eq!(json["errors"][0]["code"], code);
    }

    let final_state = run(&[
        "investments",
        "inspect-contribution",
        "--db",
        db,
        "--contribution-id",
        &contribution,
        "--json",
    ]);
    assert_eq!(
        final_state["allocation_history"].as_array().unwrap().len(),
        2
    );
    assert_eq!(final_state["unallocated_amount_minor"], 300);
}

#[test]
fn multi_instrument_action_is_atomic_and_positions_aggregate_active_allocations() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();
    let account = register_account(db, "COP");
    let contribution = add_contribution(db, &account, "-1000");
    let usd = create_instrument(db, "US dollar cash", "fiat_currency", "USD", Some("USD"));
    let stock = create_instrument(db, "Synthetic stock", "security", "COP", Some("SYN"));
    let usd_id = usd["instrument"]["id"].as_str().unwrap();
    let stock_id = stock["instrument"]["id"].as_str().unwrap();
    let allocations = serde_json::json!([
        {
            "instrument_id": usd_id,
            "cash_amount_minor": 400,
            "cash_currency": "COP",
            "acquired_quantity": "1.25"
        },
        {
            "instrument_id": stock_id,
            "cash_amount_minor": 600,
            "cash_currency": "COP",
            "acquired_quantity": "2"
        }
    ])
    .to_string();
    let allocated = run(&[
        "investments",
        "allocate",
        "--db",
        db,
        "--contribution-id",
        &contribution,
        "--allocations-json",
        &allocations,
        "--json",
    ]);
    assert_eq!(allocated["allocation_status"], "fully_allocated");
    assert_eq!(allocated["allocations"].as_array().unwrap().len(), 2);

    let second_contribution = add_contribution(db, &account, "-500");
    run(&[
        "investments",
        "allocate",
        "--db",
        db,
        "--contribution-id",
        &second_contribution,
        "--instrument-id",
        usd_id,
        "--cash-amount-minor",
        "500",
        "--cash-currency",
        "COP",
        "--quantity",
        "0.75",
        "--json",
    ]);
    let positions = run(&[
        "investments",
        "positions",
        "--db",
        db,
        "--account-id",
        &account,
        "--json",
    ]);
    let usd_position = positions["positions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|position| position["instrument_id"] == usd_id)
        .unwrap();
    assert_eq!(usd_position["quantity"], "2");
    assert_eq!(usd_position["accumulated_cost_minor"], 900);

    let rejected_contribution = add_contribution_named(
        db,
        &account,
        "-1000",
        "Synthetic rejected batch contribution",
    );
    let invalid_allocations = serde_json::json!([
        {
            "instrument_id": usd_id,
            "cash_amount_minor": 400,
            "cash_currency": "COP",
            "acquired_quantity": "1"
        },
        {
            "instrument_id": "instr_missing",
            "cash_amount_minor": 600,
            "cash_currency": "COP",
            "acquired_quantity": "1"
        }
    ])
    .to_string();
    let invalid = output(&[
        "investments",
        "allocate",
        "--db",
        db,
        "--contribution-id",
        &rejected_contribution,
        "--allocations-json",
        &invalid_allocations,
        "--json",
    ]);
    assert!(!invalid.status.success());
    let state = run(&[
        "investments",
        "inspect-contribution",
        "--db",
        db,
        "--contribution-id",
        &rejected_contribution,
        "--json",
    ]);
    assert_eq!(state["allocation_status"], "pending_allocation");
    assert!(state["allocation_history"].as_array().unwrap().is_empty());
}

#[test]
fn investment_allocations_keep_expense_income_transfer_and_contribution_reports_compatible() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();
    let source = register_account(db, "COP");
    let destination = register_account_named(db, "COP", "Synthetic transfer destination");
    add_fee_expense(db, &source, "-100", "fee_component_report_expense");
    let income_source = run(&[
        "income-sources",
        "create",
        "--db",
        db,
        "--name",
        "Synthetic employer",
        "--json",
    ])["income_source"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    run(&[
        "transactions",
        "add-income",
        "--db",
        db,
        "--account-id",
        &source,
        "--posted-date",
        "2026-07-10",
        "--description",
        "Synthetic income",
        "--amount-minor",
        "2000",
        "--currency",
        "COP",
        "--income-source-id",
        &income_source,
        "--income-kind",
        "salary",
        "--json",
    ]);
    run(&[
        "transactions",
        "add-transfer",
        "--db",
        db,
        "--from-account-id",
        &source,
        "--to-account-id",
        &destination,
        "--posted-date",
        "2026-07-10",
        "--description",
        "Synthetic own transfer",
        "--amount-minor",
        "50",
        "--currency",
        "COP",
        "--json",
    ]);
    let contribution = add_contribution(db, &source, "-1000");
    let instrument = create_instrument(db, "Generic holding", "generic", "USD", None);
    let instrument_id = instrument["instrument"]["id"].as_str().unwrap();
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
        "1000",
        "--cash-currency",
        "COP",
        "--quantity",
        "1",
        "--json",
    ]);

    let report = run(&[
        "reports",
        "summary",
        "--db",
        db,
        "--start-date",
        "2026-07-10",
        "--end-date",
        "2026-07-10",
        "--json",
    ]);
    assert_eq!(report["totals"][0]["total_income_minor"], 2000);
    assert_eq!(report["totals"][0]["total_expenses_minor"], 100);
    assert_eq!(report["totals"][0]["excluded_transfer_count"], 1);
    assert_eq!(
        report["investment_contribution_totals"][0]["total_contributed_minor"],
        1000
    );
}
