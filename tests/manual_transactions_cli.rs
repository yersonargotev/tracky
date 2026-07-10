use rusqlite::Connection;
use std::process::Command;

fn tracky() -> &'static str {
    env!("CARGO_BIN_EXE_tracky")
}

fn run(args: &[&str]) -> serde_json::Value {
    let output = Command::new(tracky())
        .args(args)
        .output()
        .expect("run tracky");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON response")
}

fn register_account(db: &str, institution: &str, label: &str) -> String {
    run(&[
        "accounts",
        "register",
        "--db",
        db,
        "--institution",
        institution,
        "--label",
        label,
        "--account-type",
        "wallet",
        "--currency",
        "COP",
        "--json",
    ])["account"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn manual_transactions_cli_creates_expense_income_and_balanced_transfer_with_manual_audit() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();
    let cash = register_account(db, "nequi", "Synthetic wallet");
    let card = register_account(db, "rappi", "Synthetic card");
    let category = run(&[
        "categories",
        "create",
        "--db",
        db,
        "--name",
        "Synthetic food",
        "--json",
    ])["category"]["id"]
        .as_str()
        .unwrap()
        .to_string();
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
        .to_string();

    let expense = run(&[
        "transactions",
        "add-expense",
        "--db",
        db,
        "--account-id",
        &cash,
        "--posted-date",
        "2026-07-09",
        "--description",
        "Synthetic groceries",
        "--amount-minor",
        "-150000",
        "--currency",
        "COP",
        "--category-id",
        &category,
        "--json",
    ]);
    assert_eq!(expense["schema_version"], "tracky.manual-transactions.v1");
    assert_eq!(
        expense["canonical_transactions"][0]["transaction_kind"],
        "expense"
    );
    assert_eq!(expense["transaction_lines"][0]["amount_minor"], -150000);
    assert_eq!(expense["provenance"][0]["source"], "manual_entry");
    assert!(expense["canonical_transactions"][0]["created_from_candidate_id"].is_null());

    let income = run(&[
        "transactions",
        "add-income",
        "--db",
        db,
        "--account-id",
        &cash,
        "--posted-date",
        "2026-07-09",
        "--description",
        "Synthetic salary",
        "--amount-minor",
        "500000",
        "--currency",
        "COP",
        "--income-source-id",
        &income_source,
        "--income-kind",
        "salary",
        "--json",
    ]);
    assert_eq!(
        income["canonical_transactions"][0]["transaction_kind"],
        "income"
    );
    assert_eq!(
        income["canonical_transactions"][0]["income_source_id"],
        income_source
    );

    let transfer = run(&[
        "transactions",
        "add-transfer",
        "--db",
        db,
        "--from-account-id",
        &cash,
        "--to-account-id",
        &card,
        "--posted-date",
        "2026-07-09",
        "--description",
        "Synthetic card payment",
        "--amount-minor",
        "200000",
        "--currency",
        "COP",
        "--json",
    ]);
    assert_eq!(
        transfer["transfer_pair"]["transfer_kind"],
        "own_account_transfer"
    );
    assert_eq!(
        transfer["canonical_transactions"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        transfer["canonical_transactions"][0]["amount_minor"],
        -200000
    );
    assert_eq!(
        transfer["canonical_transactions"][1]["amount_minor"],
        200000
    );

    let connection = Connection::open(&db_path).expect("open database");
    let audit: (i64, i64, i64, i64) = connection.query_row(
        "SELECT
            (SELECT COUNT(*) FROM canonical_transactions WHERE transaction_kind IN ('expense', 'income', 'own_account_transfer')),
            (SELECT COUNT(*) FROM manual_transaction_provenance),
            (SELECT COUNT(*) FROM manual_transfer_pairs),
            (SELECT COUNT(*) FROM transaction_fingerprints WHERE canonical_transaction_id IS NOT NULL)", [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).expect("manual audit");
    assert_eq!(audit, (4, 4, 1, 4));
}

#[test]
fn manual_transactions_cli_refuses_invalid_signs_currency_categories_and_unbalanced_transfers() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();
    let cash = register_account(db, "nequi", "Synthetic wallet");
    let card = register_account(db, "rappi", "Synthetic card");
    let category = run(&[
        "categories",
        "create",
        "--db",
        db,
        "--name",
        "Synthetic food",
        "--json",
    ])["category"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    for (args, code) in [
        (
            vec![
                "transactions",
                "add-expense",
                "--db",
                db,
                "--account-id",
                &cash,
                "--posted-date",
                "2026-07-09",
                "--description",
                "Synthetic",
                "--amount-minor",
                "150000",
                "--currency",
                "COP",
                "--category-id",
                &category,
                "--json",
            ],
            "invalid_amount_sign",
        ),
        (
            vec![
                "transactions",
                "add-expense",
                "--db",
                db,
                "--account-id",
                &card,
                "--posted-date",
                "2026-02-30",
                "--description",
                "Synthetic",
                "--amount-minor",
                "-150000",
                "--currency",
                "COP",
                "--category-id",
                &category,
                "--json",
            ],
            "invalid_posted_date",
        ),
        (
            vec![
                "transactions",
                "add-expense",
                "--db",
                db,
                "--account-id",
                &cash,
                "--posted-date",
                "2026-07-09",
                "--description",
                "Synthetic",
                "--amount-minor",
                "-150000",
                "--currency",
                "CO",
                "--category-id",
                &category,
                "--json",
            ],
            "invalid_currency",
        ),
        (
            vec![
                "transactions",
                "add-expense",
                "--db",
                db,
                "--account-id",
                &cash,
                "--posted-date",
                "2026-07-09",
                "--description",
                "Synthetic",
                "--amount-minor",
                "-150000",
                "--currency",
                "USD",
                "--category-id",
                &category,
                "--json",
            ],
            "account_currency_mismatch",
        ),
        (
            vec![
                "transactions",
                "add-expense",
                "--db",
                db,
                "--account-id",
                &cash,
                "--posted-date",
                "2026-07-09",
                "--description",
                "Synthetic",
                "--amount-minor",
                "-150000",
                "--currency",
                "COP",
                "--line",
                "cat_missing:-150000:COP",
                "--json",
            ],
            "category_not_found",
        ),
        (
            vec![
                "transactions",
                "add-expense",
                "--db",
                db,
                "--account-id",
                &cash,
                "--posted-date",
                "2026-07-09",
                "--description",
                "Synthetic",
                "--amount-minor",
                "-150000",
                "--currency",
                "COP",
                "--line",
                &format!("{category}:-140000:COP"),
                "--json",
            ],
            "expense_lines_unbalanced",
        ),
        (
            vec![
                "transactions",
                "add-transfer",
                "--db",
                db,
                "--from-account-id",
                &cash,
                "--to-account-id",
                &cash,
                "--posted-date",
                "2026-07-09",
                "--description",
                "Synthetic",
                "--amount-minor",
                "150000",
                "--currency",
                "COP",
                "--json",
            ],
            "transfer_accounts_must_differ",
        ),
        (
            vec![
                "transactions",
                "add-income",
                "--db",
                db,
                "--account-id",
                &card,
                "--posted-date",
                "2026-07-09",
                "--description",
                "Synthetic",
                "--amount-minor",
                "-150000",
                "--currency",
                "COP",
                "--income-source-id",
                "incsrc_missing",
                "--income-kind",
                "salary",
                "--json",
            ],
            "invalid_amount_sign",
        ),
        (
            vec![
                "transactions",
                "add-transfer",
                "--db",
                db,
                "--from-account-id",
                &cash,
                "--to-account-id",
                &card,
                "--posted-date",
                "2026-07-09",
                "--description",
                "Synthetic",
                "--amount-minor",
                "-150000",
                "--currency",
                "COP",
                "--json",
            ],
            "invalid_amount_sign",
        ),
    ] {
        let output = Command::new(tracky())
            .args(&args)
            .output()
            .expect("run invalid manual command");
        assert!(!output.status.success());
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("invalid JSON");
        assert_eq!(json["schema_version"], "tracky.manual-transactions.v1");
        assert_eq!(json["errors"][0]["code"], code, "args: {args:?}");
    }
}
