use std::process::Command;

fn tracky() -> &'static str {
    env!("CARGO_BIN_EXE_tracky")
}

#[test]
fn account_registry_cli_registers_and_lists_owned_accounts_as_json() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let db = db_path.to_str().unwrap();

    let register_nequi = Command::new(tracky())
        .args([
            "accounts",
            "register",
            "--db",
            db,
            "--institution",
            "nequi",
            "--label",
            "Nequi wallet",
            "--account-type",
            "wallet",
            "--currency",
            "COP",
            "--json",
        ])
        .output()
        .expect("run nequi register");
    assert!(
        register_nequi.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&register_nequi.stderr)
    );
    let nequi_json: serde_json::Value =
        serde_json::from_slice(&register_nequi.stdout).expect("register json");
    assert_eq!(nequi_json["schema_version"], "tracky.accounts.v1");
    assert_eq!(nequi_json["command"], "accounts register");
    assert_eq!(nequi_json["ok"], true);
    assert_eq!(nequi_json["account"]["institution"], "nequi");
    assert_eq!(nequi_json["account"]["label"], "Nequi wallet");

    let register_rappi = Command::new(tracky())
        .args([
            "accounts",
            "register",
            "--db",
            db,
            "--institution",
            "rappi",
            "--label",
            "RappiCard",
            "--account-type",
            "credit_card",
            "--currency",
            "COP",
            "--masked-identifier",
            "***1234",
            "--json",
        ])
        .output()
        .expect("run rappi register");
    assert!(
        register_rappi.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&register_rappi.stderr)
    );

    let list = Command::new(tracky())
        .args(["accounts", "list", "--db", db, "--json"])
        .output()
        .expect("run accounts list");
    assert!(
        list.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    let list_json: serde_json::Value = serde_json::from_slice(&list.stdout).expect("list json");
    assert_eq!(list_json["schema_version"], "tracky.accounts.v1");
    assert_eq!(list_json["command"], "accounts list");
    assert_eq!(list_json["accounts"].as_array().unwrap().len(), 2);
    assert!(list_json["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|account| account["label"] == "RappiCard"
            && account["account_type"] == "credit_card"
            && account["masked_identifier"] == "***1234"));
}
