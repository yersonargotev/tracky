use std::process::Command;

fn tracky() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tracky"))
}

#[test]
fn missing_password_env_is_stable_json_from_real_binary() {
    let output = tracky()
        .args([
            "pdf",
            "inspect",
            "assets/nequi-redacted.pdf",
            "--password-env",
            "TRACKY_TEST_MISSING_PASSWORD",
            "--json",
        ])
        .env_remove("TRACKY_TEST_MISSING_PASSWORD")
        .output()
        .expect("run tracky binary");

    assert!(!output.status.success());
    assert!(
        output.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is stable JSON");
    assert_eq!(json["schema_version"], "tracky.pdf-inspect.v1");
    assert_eq!(json["command"], "pdf inspect");
    assert_eq!(json["ok"], false);
    assert!(json.get("source_document").is_some());
    assert!(json.get("extractor_status").is_some());
    assert!(json.get("parser_status").is_some());
    assert_eq!(json["candidates"].as_array().unwrap().len(), 0);
    assert_eq!(json["extractor_status"]["status"], "not_run");
    assert_eq!(json["parser_status"]["status"], "not_run");
    assert_eq!(json["errors"][0]["category"], "validation_failure");
    assert_eq!(json["errors"][0]["code"], "missing_document_credential");
    assert_eq!(
        json["errors"][0]["path"],
        "extractor_status.credential_source"
    );
    assert_eq!(
        json["errors"][0]["details"]["env_var"],
        "TRACKY_TEST_MISSING_PASSWORD"
    );
}

#[test]
fn unreadable_pdf_is_stable_json_from_real_binary() {
    let output = tracky()
        .args(["pdf", "inspect", "assets/does-not-exist.pdf", "--json"])
        .output()
        .expect("run tracky binary");

    assert!(!output.status.success());
    assert!(
        output.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is stable JSON");
    assert_eq!(json["schema_version"], "tracky.pdf-inspect.v1");
    assert_eq!(json["command"], "pdf inspect");
    assert_eq!(json["ok"], false);
    assert!(json.get("source_document").is_some());
    assert!(json.get("extractor_status").is_some());
    assert!(json.get("parser_status").is_some());
    assert_eq!(json["candidates"].as_array().unwrap().len(), 0);
    assert_eq!(json["extractor_status"]["status"], "failed");
    assert_eq!(json["parser_status"]["status"], "not_run");
    assert_eq!(json["errors"][0]["category"], "extractor_failure");
    assert_eq!(json["errors"][0]["code"], "pdf_open_failed");
    assert_eq!(json["errors"][0]["path"], "extractor_status");
}
