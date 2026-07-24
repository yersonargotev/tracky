use lopdf::{
    content::{Content, Operation},
    dictionary,
    encryption::{EncryptionState, EncryptionVersion, Permissions},
    Document, Object, Stream,
};
use rusqlite::Connection;
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

fn synthetic_encrypted_nu_card_pdf(path: &std::path::Path, password: &str, marker: &str) {
    let mut doc = Document::with_version("1.5");
    let pages = doc.new_object_id();
    let font =
        doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let resources = doc.add_object(dictionary! {"Font"=>dictionary!{"F1"=>font}});
    let mut operations = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec!["F1".into(), 12.into()]),
    ];
    for (y, text) in [
        (750, "Nu"),
        (730, "Resumen de tu extracto"),
        (710, "Tus tarjetas"),
        (
            690,
            "Fecha Descripcion Valor Cuotas Valor interes Monto total",
        ),
        (670, "Fecha de corte 07 mayo 2026"),
        (650, marker),
        (610, "05 may COMPRA REDACTADA $ 120.000"),
        (590, "05 may COMISION POR CONVERSION INTERNACIONAL"),
        (580, "$ 12.000"),
        (550, "06 may PAGO RECIBIDO $ 120.000"),
        (510, "07 may CREDITO REDACTADO -$ 20.000"),
        (470, "08 may REVERSION REDACTADA -$ 30.000"),
        (430, "09 may DEVOLUCION REDACTADA -$ 40.000"),
    ] {
        operations.push(Operation::new(
            "Tm",
            vec![1.into(), 0.into(), 0.into(), 1.into(), 50.into(), y.into()],
        ));
        operations.push(Operation::new("Tj", vec![Object::string_literal(text)]));
    }
    operations.push(Operation::new("ET", vec![]));
    let content = Content { operations };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"Contents"=>content_id,"Resources"=>resources,"MediaBox"=>vec![0.into(),0.into(),612.into(),792.into()]});
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", catalog);
    doc.trailer.set(
        "ID",
        Object::Array(vec![
            Object::string_literal(marker.as_bytes()),
            Object::string_literal(vec![2u8; 16]),
        ]),
    );
    let state = EncryptionState::try_from(EncryptionVersion::V2 {
        document: &doc,
        owner_password: "owner-redacted",
        user_password: password,
        key_length: 128,
        permissions: Permissions::all(),
    })
    .unwrap();
    doc.encrypt(&state).unwrap();
    doc.save(path).unwrap();
}

#[test]
fn protected_nu_card_pdf_is_content_detected_and_imported_review_first() {
    let dir = tempfile::tempdir().expect("temp dir");
    let first_pdf = dir.path().join("statement-redacted-a.pdf");
    let second_pdf = dir.path().join("statement-redacted-b.pdf");
    let db_path = dir.path().join("tracky.sqlite");
    let password = "runtime-only-redacted";
    synthetic_encrypted_nu_card_pdf(&first_pdf, password, "SYNTHETIC SOURCE A");
    synthetic_encrypted_nu_card_pdf(&second_pdf, password, "SYNTHETIC SOURCE B");

    let inspect = tracky()
        .args([
            "pdf",
            "inspect",
            first_pdf.to_str().unwrap(),
            "--password-env",
            "TRACKY_TEST_NU_CARD_PASSWORD",
            "--json",
        ])
        .env("TRACKY_TEST_NU_CARD_PASSWORD", password)
        .output()
        .expect("inspect synthetic Nu card PDF");
    assert!(
        inspect.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&inspect.stderr),
        String::from_utf8_lossy(&inspect.stdout)
    );
    let inspect_json: serde_json::Value =
        serde_json::from_slice(&inspect.stdout).expect("inspect JSON");
    assert_eq!(inspect_json["source_document"]["institution_hint"], "nu");
    assert_eq!(
        inspect_json["source_document"]["account_hint"]["label"],
        "Nu credit card"
    );
    assert_eq!(
        inspect_json["parser_status"]["parser_id"],
        "nu.credit-card.statement.v1"
    );
    assert_eq!(inspect_json["parser_status"]["parser_version"], "1");
    assert_eq!(inspect_json["candidates"].as_array().unwrap().len(), 6);
    let semantics = inspect_json["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|candidate| candidate["semantic_hint"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        semantics,
        vec![
            "card_charge",
            "card_charge",
            "card_payment",
            "card_credit",
            "card_reversal",
            "card_refund"
        ]
    );
    let conversion_fee = inspect_json["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| {
            candidate["description"]
                .as_str()
                .is_some_and(|description| description.contains("COMISION"))
        })
        .expect("conversion fee candidate");
    assert_eq!(conversion_fee["semantic_hint"], "card_charge");
    assert_eq!(conversion_fee["direction_hint"], "outflow");
    assert_eq!(conversion_fee["amount_minor"], 1_200_000);
    assert!(conversion_fee["provenance"]["evidence"]["text"]
        .as_str()
        .unwrap()
        .contains("COMISION"));
    assert!(inspect_json["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|candidate| {
            !candidate["description"]
                .as_str()
                .unwrap()
                .contains("REDACTADA")
                && !candidate["provenance"]["evidence"]["text"]
                    .as_str()
                    .unwrap()
                    .contains("REDACTADA")
        }));
    assert!(!String::from_utf8_lossy(&inspect.stdout).contains(password));
    assert!(
        !db_path.exists(),
        "read-only inspection must not create SQLite"
    );

    for pdf in [&first_pdf, &second_pdf] {
        let imported = tracky()
            .args([
                "import",
                "pdf",
                pdf.to_str().unwrap(),
                "--db",
                db_path.to_str().unwrap(),
                "--password-env",
                "TRACKY_TEST_NU_CARD_PASSWORD",
                "--json",
            ])
            .env("TRACKY_TEST_NU_CARD_PASSWORD", password)
            .output()
            .expect("import synthetic Nu card PDF");
        assert!(
            imported.status.success(),
            "stderr: {}\nstdout: {}",
            String::from_utf8_lossy(&imported.stderr),
            String::from_utf8_lossy(&imported.stdout)
        );
        assert!(!String::from_utf8_lossy(&imported.stdout).contains(password));
    }

    let connection = Connection::open(&db_path).expect("open imported database");
    let counts: (i64, i64, i64, i64) = connection
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM source_documents),
                (SELECT COUNT(*) FROM import_batches),
                (SELECT COUNT(*) FROM candidate_transactions),
                (SELECT COUNT(*) FROM canonical_transactions)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(counts, (2, 2, 12, 0));
    let duplicate_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM candidate_transactions
             WHERE status = 'possible_duplicate'
               AND duplicate_status IN ('possible_duplicate', 'exact_duplicate')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(duplicate_count, 6);
    let provenance_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM provenance
             WHERE parser_id = 'nu.credit-card.statement.v1'
               AND raw_storage_policy = 'redacted_only'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(provenance_count, 12);
}
