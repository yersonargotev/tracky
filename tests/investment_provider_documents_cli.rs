use lopdf::content::{Content, Operation};
use lopdf::{
    dictionary, Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream,
};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::process::{Command, Output};
use tracky::storage::apply_migrations;

fn synthetic_encrypted_nu_pdf(path: &std::path::Path, password: &str) {
    let mut doc = Document::with_version("1.5");
    let pages = doc.new_object_id();
    let font =
        doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let resources = doc.add_object(dictionary! {"Font"=>dictionary!{"F1"=>font}});
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 12.into()]),
            Operation::new(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    50.into(),
                    700.into(),
                ],
            ),
            Operation::new(
                "Tj",
                vec![Object::string_literal(
                    "Llego tu extracto Junio 2026 CDT Nu",
                )],
            ),
            Operation::new(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    360.into(),
                    680.into(),
                ],
            ),
            Operation::new("Tj", vec![Object::string_literal("-$2.000,00")]),
            Operation::new(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    50.into(),
                    680.into(),
                ],
            ),
            Operation::new(
                "Tj",
                vec![Object::string_literal("16 jun Enviaste a Plenti")],
            ),
            Operation::new(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    50.into(),
                    660.into(),
                ],
            ),
            Operation::new(
                "Tj",
                vec![Object::string_literal("17 jun Recibiste transferencia")],
            ),
            Operation::new(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    360.into(),
                    660.into(),
                ],
            ),
            Operation::new("Tj", vec![Object::string_literal("+$3.500,00")]),
            Operation::new(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    50.into(),
                    640.into(),
                ],
            ),
            Operation::new("Tj", vec![Object::string_literal("18 jun Pago servicio")]),
            Operation::new(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    360.into(),
                    640.into(),
                ],
            ),
            Operation::new("Tj", vec![Object::string_literal("-$1.200,00")]),
            Operation::new("ET", vec![]),
        ],
    };
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
            Object::string_literal(vec![1u8; 16]),
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

fn output(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tracky"))
        .args(args)
        .output()
        .unwrap()
}
fn run(args: &[&str]) -> Value {
    let out = output(args);
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn nu_inspect_exposes_ordinary_candidates_with_provider_events() {
    let d = tempfile::tempdir().unwrap();
    let pdf = d.path().join("statement.pdf");
    synthetic_encrypted_nu_pdf(&pdf, "runtime-only");
    let out = Command::new(env!("CARGO_BIN_EXE_tracky"))
        .env("TRACKY_SYNTHETIC_PASSWORD", "runtime-only")
        .args([
            "investment-documents",
            "inspect",
            pdf.to_str().unwrap(),
            "--password-env",
            "TRACKY_SYNTHETIC_PASSWORD",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["schema_version"], "tracky.investment-documents.v2");
    assert_eq!(value["events"].as_array().unwrap().len(), 1);
    assert_eq!(value["ordinary_candidates"].as_array().unwrap().len(), 2);
    assert!(value["ordinary_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|candidate| candidate["amount_minor"] == 350_000
            && candidate["direction_hint"] == "inflow"
            && candidate["status"] == "pending_review"));
    assert!(value["ordinary_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|candidate| candidate["amount_minor"] == -120_000
            && candidate["direction_hint"] == "outflow"));
}
fn seed(db: &str) -> Connection {
    let c = Connection::open(db).unwrap();
    apply_migrations(&c).unwrap();
    c.execute("INSERT INTO institutions(id,name) VALUES('provider_nu_inst','Nu'),('provider_plenti_inst','Plenti'),('provider_wenia_inst','Wenia')",[]).unwrap();
    c.execute("INSERT INTO accounts(id,institution_id,label,currency,is_owned) VALUES('provider_nu','provider_nu_inst','Nu','COP',1),('provider_plenti','provider_plenti_inst','Plenti','COP',1),('provider_wenia','provider_wenia_inst','Wenia','USD',1)",[]).unwrap();
    c.execute("INSERT INTO source_documents(id,input_name,content_sha256,mime_type,byte_size,institution_hint) VALUES('src_provider','redacted.pdf',?1,'application/pdf',1,'plenti')",["ab".repeat(32)]).unwrap();
    c.execute("INSERT INTO import_batches(id,source_document_id,started_at,completed_at,status,candidate_count,error_count,duplicate_count,error_details_json) VALUES('batch_provider','src_provider','2026-06-16T00:00:00Z','2026-06-16T00:00:00Z','completed',1,0,0,'[]')",[]).unwrap();
    c
}
fn event(
    c: &Connection,
    id: &str,
    provider: &str,
    kind: &str,
    amount: i64,
    currency: &str,
    position: Option<(&str, &str)>,
) {
    let (instrument, quantity) = position.map_or((None, None), |(i, q)| (Some(i), Some(q)));
    c.execute("INSERT INTO investment_document_events(id,source_document_id,import_batch_id,provider,parser_id,parser_version,event_type,provider_effective_date,currency,amount_minor,instrument_hint,quantity,page_number,row_index,evidence_redaction,fingerprint,status) VALUES(?1,'src_provider','batch_provider',?2,?2||'.investment-document.v1','1',?3,'2026-06-16',?4,?5,?6,?7,1,1,'REDACTED',?1,'pending_review')",params![id,provider,kind,currency,amount,instrument,quantity]).unwrap();
    c.execute("INSERT INTO provenance(id,investment_document_event_id,source_document_id,import_batch_id,page_number,row_index,extractor_name,parser_id,parser_version,evidence_redaction,evidence_text_redacted,raw_storage_policy,confidence) VALUES('prov_'||?1,?1,'src_provider','batch_provider',1,1,'pdf_oxide',?2||'.investment-document.v1','1','REDACTED','REDACTED','redacted_only',1)",params![id,provider]).unwrap();
}
#[test]
fn candidates_are_direction_aware_read_only_and_explicitly_consumed() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("x.sqlite");
    let db = path.to_str().unwrap();
    let c = seed(db);
    event(&c, "evt_plenti", "plenti", "deposit", 200_000, "COP", None);
    c.execute(
        "INSERT INTO institutions(id,name) VALUES('inst','Synthetic bank')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO accounts(id,institution_id,label,currency,is_owned) VALUES('acct','inst','Source','COP',1)",[]).unwrap();
    c.execute("INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency,transaction_kind,investment_allocation_status) VALUES('txn','acct','2026-06-16','Synthetic contribution',-200000,'COP','investment_contribution','pending_allocation')",[]).unwrap();
    drop(c);
    let before = std::fs::metadata(db).unwrap().len();
    let candidates = run(&[
        "investment-documents",
        "candidates",
        "evt_plenti",
        "--db",
        db,
        "--event-account-id",
        "provider_plenti",
        "--counterpart-account-id",
        "acct",
        "--json",
    ]);
    assert_eq!(candidates["candidates"][0]["status"], "unique_match");
    assert_eq!(candidates["candidates"][0]["target_id"], "txn");
    assert_eq!(before, std::fs::metadata(db).unwrap().len());
    let reviewed = run(&[
        "investment-documents",
        "reconcile-deposit",
        "evt_plenti",
        "--db",
        db,
        "--event-account-id",
        "provider_plenti",
        "--counterpart-account-id",
        "acct",
        "--canonical-transaction-id",
        "txn",
        "--json",
    ]);
    assert_eq!(reviewed["events"][0]["decision"], "reconcile_deposit");
    let second = output(&[
        "investment-documents",
        "reconcile-deposit",
        "evt_plenti",
        "--db",
        db,
        "--event-account-id",
        "provider_plenti",
        "--counterpart-account-id",
        "acct",
        "--canonical-transaction-id",
        "txn",
        "--json",
    ]);
    assert!(!second.status.success());
}
#[test]
fn observed_position_accepts_into_0030_snapshot_with_canonical_provenance() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("x.sqlite");
    let db = path.to_str().unwrap();
    let c = seed(db);
    c.execute(
        "INSERT INTO institutions(id,name) VALUES('wenia_inst','Wenia')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO accounts(id,institution_id,label,currency,is_owned) VALUES('wenia_acct','wenia_inst','Wenia custody','USD',1)",[]).unwrap();
    c.execute("INSERT INTO investment_instruments(id,name,instrument_type,denomination_currency,provider,provider_identifier) VALUES('copw','COPW','dollar_referenced_digital_asset','USD','Wenia','COPW')",[]).unwrap();
    event(
        &c,
        "evt_position",
        "wenia",
        "observed_position",
        12345,
        "USD",
        Some(("COPW", "10.25")),
    );
    drop(c);
    let accepted = run(&[
        "investment-documents",
        "accept-snapshot",
        "evt_position",
        "--db",
        db,
        "--account-id",
        "wenia_acct",
        "--instrument-id",
        "copw",
        "--json",
    ]);
    assert_eq!(accepted["events"][0]["decision"], "accept_snapshot");
    let c = Connection::open(db).unwrap();
    let counts:(i64,i64,i64)=c.query_row("SELECT (SELECT count(*) FROM investment_snapshots),(SELECT count(*) FROM investment_snapshot_baselines),(SELECT count(*) FROM provenance WHERE investment_document_event_id='evt_position' AND investment_snapshot_id IS NOT NULL)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(counts, (1, 1, 1));
}
#[test]
fn observed_position_is_never_a_money_candidate() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("x.sqlite");
    let db = path.to_str().unwrap();
    let c = seed(db);
    event(
        &c,
        "evt_position",
        "wenia",
        "observed_position",
        12345,
        "USD",
        Some(("COPW", "10")),
    );
    drop(c);
    let value = run(&[
        "investment-documents",
        "candidates",
        "evt_position",
        "--db",
        db,
        "--event-account-id",
        "provider_wenia",
        "--counterpart-account-id",
        "unused",
        "--json",
    ]);
    assert_eq!(value["candidates"][0]["status"], "incompatible");
}

#[test]
fn ambiguity_and_wrong_direction_remain_pending() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("x.sqlite");
    let db = path.to_str().unwrap();
    let c = seed(db);
    event(&c, "evt_plenti", "plenti", "deposit", 200_000, "COP", None);
    c.execute(
        "INSERT INTO institutions(id,name) VALUES('inst','Synthetic bank')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO accounts(id,institution_id,label,currency,is_owned) VALUES('acct','inst','Source','COP',1)",[]).unwrap();
    for id in ["txn_a", "txn_b"] {
        c.execute("INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency,transaction_kind,investment_allocation_status) VALUES(?1,'acct','2026-06-16','Synthetic contribution',-200000,'COP','investment_contribution','pending_allocation')",[id]).unwrap();
    }
    c.execute("INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency,transaction_kind) VALUES('wrong','acct','2026-06-16','Wrong direction',200000,'COP','own_account_transfer')",[]).unwrap();
    drop(c);
    let value = run(&[
        "investment-documents",
        "candidates",
        "evt_plenti",
        "--db",
        db,
        "--event-account-id",
        "provider_plenti",
        "--counterpart-account-id",
        "acct",
        "--json",
    ]);
    assert!(value["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|x| x["status"] == "ambiguous_match"));
    let rejected = output(&[
        "investment-documents",
        "reconcile-deposit",
        "evt_plenti",
        "--db",
        db,
        "--event-account-id",
        "provider_plenti",
        "--counterpart-account-id",
        "acct",
        "--canonical-transaction-id",
        "txn_a",
        "--json",
    ]);
    assert!(!rejected.status.success());
    let c = Connection::open(db).unwrap();
    let status: String = c
        .query_row(
            "SELECT status FROM investment_document_events WHERE id='evt_plenti'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "pending_review");
}

#[test]
fn nu_to_plenti_provider_pair_is_selected_and_consumed_atomically() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("x.sqlite");
    let db = path.to_str().unwrap();
    let c = seed(db);
    c.execute(
        "INSERT INTO institutions(id,name) VALUES('inst','Synthetic bank')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO accounts(id,institution_id,label,currency,is_owned) VALUES('acct','inst','Source','COP',1)",[]).unwrap();
    event(&c, "evt_nu", "nu", "withdrawal", -200_000, "COP", None);
    event(&c, "evt_plenti", "plenti", "deposit", 200_000, "COP", None);
    drop(c);
    let value = run(&[
        "investment-documents",
        "candidates",
        "evt_nu",
        "--db",
        db,
        "--event-account-id",
        "provider_nu",
        "--counterpart-account-id",
        "provider_plenti",
        "--json",
    ]);
    assert_eq!(value["candidates"][0]["target_id"], "evt_plenti");
    run(&[
        "investment-documents",
        "reconcile-withdrawal",
        "evt_nu",
        "--db",
        db,
        "--event-account-id",
        "provider_nu",
        "--counterpart-account-id",
        "provider_plenti",
        "--provider-event-id",
        "evt_plenti",
        "--json",
    ]);
    let c = Connection::open(db).unwrap();
    let accepted: i64 = c
        .query_row(
            "SELECT count(*) FROM investment_document_events WHERE status='accepted'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(accepted, 2);
}

#[test]
fn typed_reconciliation_checks_account_reference_and_event_semantics() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("x.sqlite");
    let db = path.to_str().unwrap();
    let c = seed(db);
    c.execute(
        "INSERT INTO institutions(id,name) VALUES('inst','Synthetic bank')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO accounts(id,institution_id,label,currency,is_owned) VALUES('acct','inst','Right','COP',1),('other','inst','Wrong','COP',1)",[]).unwrap();
    c.execute("INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency,transaction_kind,investment_allocation_status) VALUES('txn','acct','2026-06-16','Synthetic contribution',-200000,'COP','investment_contribution','pending_allocation')",[]).unwrap();
    event(&c, "evt", "plenti", "deposit", 200_000, "COP", None);
    let wrong_account = run(&[
        "investment-documents",
        "candidates",
        "evt",
        "--db",
        db,
        "--event-account-id",
        "provider_plenti",
        "--counterpart-account-id",
        "other",
        "--json",
    ]);
    assert_eq!(wrong_account["candidates"][0]["status"], "unmatched");
    c.execute(
        "UPDATE investment_document_events SET external_reference='provider-ref' WHERE id='evt'",
        [],
    )
    .unwrap();
    c.execute(
        "UPDATE canonical_transactions SET external_reference='provider-ref' WHERE id='txn'",
        [],
    )
    .unwrap();
    drop(c);
    let referenced = run(&[
        "investment-documents",
        "candidates",
        "evt",
        "--db",
        db,
        "--event-account-id",
        "provider_plenti",
        "--counterpart-account-id",
        "acct",
        "--json",
    ]);
    assert_eq!(referenced["candidates"][0]["status"], "unique_match");
    let wrong_action = output(&[
        "investment-documents",
        "reconcile-withdrawal",
        "evt",
        "--db",
        db,
        "--event-account-id",
        "provider_plenti",
        "--counterpart-account-id",
        "acct",
        "--canonical-transaction-id",
        "txn",
        "--json",
    ]);
    assert!(!wrong_action.status.success());
}

#[test]
fn audit_inspection_expands_canonical_target_and_is_read_only() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("x.sqlite");
    let db = path.to_str().unwrap();
    let c = seed(db);
    c.execute(
        "INSERT INTO institutions(id,name) VALUES('inst','Synthetic bank')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO accounts(id,institution_id,label,currency,is_owned) VALUES('acct','inst','Source','COP',1)",[]).unwrap();
    c.execute("INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency,transaction_kind,investment_allocation_status) VALUES('txn','acct','2026-06-16','Synthetic contribution',-200000,'COP','investment_contribution','pending_allocation')",[]).unwrap();
    event(&c, "evt", "plenti", "deposit", 200_000, "COP", None);
    drop(c);
    run(&[
        "investment-documents",
        "reconcile-deposit",
        "evt",
        "--db",
        db,
        "--event-account-id",
        "provider_plenti",
        "--counterpart-account-id",
        "acct",
        "--canonical-transaction-id",
        "txn",
        "--json",
    ]);
    let before = std::fs::metadata(db).unwrap().len();
    let inspected = run(&[
        "investment-documents",
        "inspect-event",
        "evt",
        "--db",
        db,
        "--json",
    ]);
    assert_eq!(inspected["audit_chain"]["reconciled_target"]["id"], "txn");
    assert_eq!(
        inspected["audit_chain"]["reconciled_target"]["account_id"],
        "acct"
    );
    assert_eq!(before, std::fs::metadata(db).unwrap().len());
    let c = Connection::open(db).unwrap();
    let linked:String=c.query_row("SELECT canonical_transaction_id FROM provenance WHERE investment_document_event_id='evt'",[],|r|r.get(0)).unwrap();
    assert_eq!(linked, "txn");
}

#[test]
fn repeated_snapshot_acceptance_and_incomplete_cdt_evidence_do_not_mutate() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("x.sqlite");
    let db = path.to_str().unwrap();
    let c = seed(db);
    c.execute(
        "INSERT INTO institutions(id,name) VALUES('wenia_inst','Wenia')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO accounts(id,institution_id,label,currency,is_owned) VALUES('wenia_acct','wenia_inst','Custody','USD',1)",[]).unwrap();
    c.execute("INSERT INTO investment_instruments(id,name,instrument_type,denomination_currency,provider,provider_identifier) VALUES('copw','COPW','dollar_referenced_digital_asset','USD','Wenia','COPW')",[]).unwrap();
    event(
        &c,
        "position",
        "wenia",
        "observed_position",
        100,
        "USD",
        Some(("COPW", "1")),
    );
    event(&c, "cdt", "nu", "cdt_opening", -100, "COP", None);
    drop(c);
    run(&[
        "investment-documents",
        "accept-snapshot",
        "position",
        "--db",
        db,
        "--account-id",
        "wenia_acct",
        "--instrument-id",
        "copw",
        "--json",
    ]);
    let second = output(&[
        "investment-documents",
        "accept-snapshot",
        "position",
        "--db",
        db,
        "--account-id",
        "wenia_acct",
        "--instrument-id",
        "copw",
        "--json",
    ]);
    assert!(!second.status.success());
    let c = Connection::open(db).unwrap();
    assert_eq!(
        c.query_row("SELECT count(*) FROM investment_snapshots", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(c);
    let cdt = output(&[
        "investment-documents",
        "reconcile-withdrawal",
        "cdt",
        "--db",
        db,
        "--event-account-id",
        "provider_nu",
        "--counterpart-account-id",
        "wenia_acct",
        "--canonical-transaction-id",
        "missing",
        "--json",
    ]);
    assert!(!cdt.status.success());
    let c = Connection::open(db).unwrap();
    let status: String = c
        .query_row(
            "SELECT status FROM investment_document_events WHERE id='cdt'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "pending_review");
}

#[test]
fn extraction_failure_is_safe_json_and_never_leaks_runtime_secret() {
    let d = tempfile::tempdir().unwrap();
    let pdf = d.path().join("not-a-pdf.pdf");
    std::fs::write(&pdf, b"not a pdf").unwrap();
    let db = d.path().join("x.sqlite");
    let out = Command::new(env!("CARGO_BIN_EXE_tracky"))
        .args([
            "investment-documents",
            "import",
            pdf.to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--password-env",
            "TRACKY_TEST_SECRET",
            "--json",
        ])
        .env("TRACKY_TEST_SECRET", "do-not-print-this")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(!stdout.contains("do-not-print-this"));
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["errors"][0]["code"], "pdf_extraction_failed");
    let c = Connection::open(db).unwrap();
    assert_eq!(
        c.query_row("SELECT count(*) FROM source_documents", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn reconciliation_mismatch_dimensions_remain_unmatched() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("x.sqlite");
    let db = path.to_str().unwrap();
    let c = seed(db);
    c.execute(
        "INSERT INTO institutions(id,name) VALUES('inst','Synthetic bank')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO accounts(id,institution_id,label,currency,is_owned) VALUES('acct','inst','Source','COP',1)",[]).unwrap();
    c.execute("INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency,transaction_kind,investment_allocation_status) VALUES('txn','acct','2026-06-16','Synthetic contribution',-200000,'COP','investment_contribution','pending_allocation')",[]).unwrap();
    event(&c, "evt", "plenti", "deposit", 200_000, "COP", None);
    drop(c);
    for sql in ["UPDATE canonical_transactions SET posted_date='2026-05-01' WHERE id='txn'","UPDATE canonical_transactions SET posted_date='2026-06-16',amount_minor=-199999 WHERE id='txn'","UPDATE canonical_transactions SET amount_minor=-200000,currency='USD' WHERE id='txn'","UPDATE canonical_transactions SET currency='COP',transaction_kind='expense' WHERE id='txn'"] { let c=Connection::open(db).unwrap(); c.execute(sql,[]).unwrap(); drop(c); let result=run(&["investment-documents","candidates","evt","--db",db,"--event-account-id","provider_plenti","--counterpart-account-id","acct","--json"]); assert_eq!(result["candidates"][0]["status"],"unmatched", "{sql}"); }
}

#[test]
fn sqlite_deduplication_rejects_document_movement_and_reference_reuse() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("x.sqlite");
    let db = path.to_str().unwrap();
    let c = seed(db);
    event(&c, "one", "plenti", "deposit", 100, "COP", None);
    event(&c, "two", "plenti", "deposit", 200, "COP", None);
    assert!(c
        .execute(
            "UPDATE investment_document_events SET fingerprint='one' WHERE id='two'",
            []
        )
        .is_err());
    c.execute(
        "UPDATE investment_document_events SET external_reference='same-ref' WHERE id='one'",
        [],
    )
    .unwrap();
    assert!(c
        .execute(
            "UPDATE investment_document_events SET external_reference='same-ref' WHERE id='two'",
            []
        )
        .is_err());
    assert!(c.execute("INSERT INTO source_documents(id,input_name,content_sha256,mime_type,byte_size) VALUES('duplicate','redacted-2.pdf',?1,'application/pdf',1)",["ab".repeat(32)]).is_err());
}

#[test]
fn synthetic_nu_credential_paths_are_public_secret_safe_and_exactly_deduplicated() {
    let d = tempfile::tempdir().unwrap();
    let asset = d.path().join("nu-redacted-encrypted.pdf");
    let password = "synthetic-runtime-password";
    synthetic_encrypted_nu_pdf(&asset, password);
    for (name, value) in [
        ("TRACKY_TEST_MISSING", None),
        ("TRACKY_TEST_WRONG", Some("wrong-runtime-secret")),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tracky"));
        command.args([
            "investment-documents",
            "inspect",
            asset.to_str().unwrap(),
            "--password-env",
            name,
            "--json",
        ]);
        if let Some(value) = value {
            command.env(name, value);
        } else {
            command.env_remove(name);
        }
        let out = command.output().unwrap();
        assert!(!out.status.success());
        let stdout = String::from_utf8(out.stdout).unwrap();
        assert!(!stdout.contains("wrong-runtime-secret"));
        let json: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(json["errors"][0]["code"], "pdf_extraction_failed");
    }
    let db = d.path().join("x.sqlite");
    let first = Command::new(env!("CARGO_BIN_EXE_tracky"))
        .args([
            "investment-documents",
            "import",
            asset.to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--password-env",
            "TRACKY_TEST_CORRECT",
            "--json",
        ])
        .env("TRACKY_TEST_CORRECT", password)
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stdout)
    );
    let first_json: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first_json["events"].as_array().unwrap().len(), 1);
    assert_eq!(
        first_json["ordinary_candidates"].as_array().unwrap().len(),
        2
    );
    let c = Connection::open(&db).unwrap();
    let counts: (i64, i64, i64, i64, i64, i64) = c
        .query_row(
            "SELECT
                (SELECT count(*) FROM source_documents),
                (SELECT count(*) FROM import_batches),
                (SELECT count(*) FROM candidate_transactions),
                (SELECT count(*) FROM investment_document_events),
                (SELECT count(*) FROM provenance),
                (SELECT count(*) FROM canonical_transactions)",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(counts, (1, 1, 2, 1, 3, 0));
    drop(c);
    let second = Command::new(env!("CARGO_BIN_EXE_tracky"))
        .args([
            "investment-documents",
            "import",
            asset.to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--password-env",
            "TRACKY_TEST_CORRECT",
            "--json",
        ])
        .env("TRACKY_TEST_CORRECT", password)
        .output()
        .unwrap();
    assert!(!second.status.success());
    let json: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(json["errors"][0]["code"], "duplicate_source_document");
    assert!(!String::from_utf8_lossy(&second.stdout).contains(password));
}

#[test]
fn mixed_nu_import_rolls_back_candidates_when_provider_event_persistence_fails() {
    let d = tempfile::tempdir().unwrap();
    let asset = d.path().join("nu-redacted-encrypted.pdf");
    let db = d.path().join("x.sqlite");
    synthetic_encrypted_nu_pdf(&asset, "runtime-only");
    let c = Connection::open(&db).unwrap();
    apply_migrations(&c).unwrap();
    c.execute_batch(
        "CREATE TRIGGER reject_provider_event BEFORE INSERT ON investment_document_events
         BEGIN SELECT RAISE(ABORT, 'synthetic persistence failure'); END;",
    )
    .unwrap();
    drop(c);

    let out = Command::new(env!("CARGO_BIN_EXE_tracky"))
        .env("TRACKY_TEST_CORRECT", "runtime-only")
        .args([
            "investment-documents",
            "import",
            asset.to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--password-env",
            "TRACKY_TEST_CORRECT",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let c = Connection::open(&db).unwrap();
    let counts: (i64, i64, i64, i64, i64) = c
        .query_row(
            "SELECT
                (SELECT count(*) FROM source_documents),
                (SELECT count(*) FROM import_batches),
                (SELECT count(*) FROM candidate_transactions),
                (SELECT count(*) FROM investment_document_events),
                (SELECT count(*) FROM provenance)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0, 0, 0, 0));
}
