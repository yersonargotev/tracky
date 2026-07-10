use rusqlite::Connection;
use std::process::Command;
use tracky::storage::{apply_migrations, register_owned_account, AccountRegisterInput};

fn tracky() -> &'static str {
    env!("CARGO_BIN_EXE_tracky")
}
fn run(args: &[&str]) -> serde_json::Value {
    let output = Command::new(tracky())
        .args(args)
        .output()
        .expect("run tracky");
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("stderr: {}", String::from_utf8_lossy(&output.stderr)))
}
fn account(db: &str) -> String {
    let connection = Connection::open(db).unwrap();
    apply_migrations(&connection).unwrap();
    register_owned_account(
        &connection,
        AccountRegisterInput {
            institution: "synthetic".into(),
            label: "Synthetic wallet".into(),
            account_type: "wallet".into(),
            currency: "COP".into(),
            masked_identifier: None,
        },
    )
    .unwrap()
    .account
    .unwrap()
    .id
}
#[test]
fn ledger_lists_inspects_and_safely_updates_manual_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tracky.sqlite");
    let db = path.to_str().unwrap();
    let account_id = account(db);
    let food = run(&[
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
    let home = run(&[
        "categories",
        "create",
        "--db",
        db,
        "--name",
        "Synthetic home",
        "--json",
    ])["category"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let source = run(&[
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
        &account_id,
        "--posted-date",
        "2026-07-09",
        "--description",
        "Synthetic groceries",
        "--amount-minor",
        "-150000",
        "--currency",
        "COP",
        "--category-id",
        &food,
        "--json",
    ]);
    let expense_id = expense["canonical_transactions"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let income = run(&[
        "transactions",
        "add-income",
        "--db",
        db,
        "--account-id",
        &account_id,
        "--posted-date",
        "2026-07-10",
        "--description",
        "Synthetic salary",
        "--amount-minor",
        "500000",
        "--currency",
        "COP",
        "--income-source-id",
        &source,
        "--income-kind",
        "salary",
        "--json",
    ]);
    let income_id = income["canonical_transactions"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let listed = run(&[
        "transactions",
        "list",
        "--db",
        db,
        "--start-date",
        "2026-07-09",
        "--end-date",
        "2026-07-09",
        "--account-id",
        &account_id,
        "--category-id",
        &food,
        "--type",
        "expense",
        "--json",
    ]);
    assert_eq!(listed["schema_version"], "tracky.transactions.v1");
    assert_eq!(
        listed["canonical_transactions"].as_array().unwrap().len(),
        1
    );
    assert_eq!(listed["canonical_transactions"][0]["id"], expense_id);
    let inspected = run(&["transactions", "inspect", &expense_id, "--db", db, "--json"]);
    assert_eq!(inspected["provenance"][0]["source"], "manual_entry");
    assert_eq!(inspected["transaction_lines"][0]["category_id"], food);
    let updated = run(&[
        "transactions",
        "update",
        &expense_id,
        "--db",
        db,
        "--description",
        "Synthetic corrected groceries",
        "--line",
        &format!("{food}:-100000:COP"),
        "--line",
        &format!("{home}:-50000:COP"),
        "--json",
    ]);
    assert!(updated["ok"].as_bool().unwrap());
    assert_eq!(
        updated["canonical_transaction"]["description"],
        "Synthetic corrected groceries"
    );
    assert_eq!(updated["transaction_lines"].as_array().unwrap().len(), 2);
    assert_eq!(updated["edits"].as_array().unwrap().len(), 1);
    assert_eq!(
        updated["edits"][0]["changed_fields"]["before"]["description"],
        "Synthetic groceries"
    );
    let income_update = run(&[
        "transactions",
        "update",
        &income_id,
        "--db",
        db,
        "--income-kind",
        "freelance",
        "--json",
    ]);
    assert_eq!(
        income_update["canonical_transaction"]["income_kind"],
        "freelance"
    );
}
#[test]
fn ledger_refuses_unbalanced_updates_and_transfer_reclassification() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tracky.sqlite");
    let db = path.to_str().unwrap();
    let first = account(db);
    let second = {
        let c = Connection::open(db).unwrap();
        register_owned_account(
            &c,
            AccountRegisterInput {
                institution: "synthetic2".into(),
                label: "Synthetic card".into(),
                account_type: "card".into(),
                currency: "COP".into(),
                masked_identifier: None,
            },
        )
        .unwrap()
        .account
        .unwrap()
        .id
    };
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
    let expense = run(&[
        "transactions",
        "add-expense",
        "--db",
        db,
        "--account-id",
        &first,
        "--posted-date",
        "2026-07-09",
        "--description",
        "Synthetic",
        "--amount-minor",
        "-100",
        "--currency",
        "COP",
        "--category-id",
        &category,
        "--json",
    ]);
    let expense_id = expense["canonical_transactions"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rejected = run(&[
        "transactions",
        "update",
        &expense_id,
        "--db",
        db,
        "--line",
        &format!("{category}:-99:COP"),
        "--json",
    ]);
    assert_eq!(rejected["errors"][0]["code"], "expense_lines_unbalanced");
    let transfer = run(&[
        "transactions",
        "add-transfer",
        "--db",
        db,
        "--from-account-id",
        &first,
        "--to-account-id",
        &second,
        "--posted-date",
        "2026-07-09",
        "--description",
        "Synthetic transfer",
        "--amount-minor",
        "100",
        "--currency",
        "COP",
        "--json",
    ]);
    let transfer_id = transfer["canonical_transactions"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let blocked = run(&[
        "transactions",
        "update",
        &transfer_id,
        "--db",
        db,
        "--category-id",
        &category,
        "--json",
    ]);
    assert_eq!(
        blocked["errors"][0]["code"],
        "transfer_classification_immutable"
    );
}
