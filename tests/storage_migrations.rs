use rusqlite::{params, Connection, Error};
use tracky::storage::apply_migrations;

fn temporary_database() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("tracky.sqlite");
    let connection = Connection::open(db_path).expect("open temp sqlite db");
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .expect("enable foreign keys");
    (dir, connection)
}

#[test]
fn migrations_create_review_first_tables() {
    let (_dir, connection) = temporary_database();

    apply_migrations(&connection).expect("apply migrations");

    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN (
                'institutions',
                'accounts',
                'source_documents',
                'import_batches',
                'candidate_transactions',
                'provenance',
                'canonical_transactions',
                'transaction_fingerprints',
                'transaction_duplicate_markers'
            )",
            [],
            |row| row.get(0),
        )
        .expect("count tables");
    let user_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read user_version");

    assert_eq!(table_count, 9);
    assert_eq!(user_version, 1);
}

#[test]
fn migrations_expand_legacy_transfer_pair_kind_without_losing_indexes() {
    let (_dir, connection) = temporary_database();
    connection
        .execute_batch(
            "CREATE TABLE canonical_transfer_pairs (
               id TEXT PRIMARY KEY,
               transfer_kind TEXT NOT NULL CHECK (transfer_kind IN ('card_payment')),
               posted_date TEXT NOT NULL,
               amount_minor INTEGER NOT NULL CHECK (amount_minor > 0),
               currency TEXT NOT NULL,
               from_account_id TEXT NOT NULL REFERENCES accounts(id),
               to_account_id TEXT NOT NULL REFERENCES accounts(id),
               from_candidate_id TEXT NOT NULL UNIQUE REFERENCES candidate_transactions(id),
               to_candidate_id TEXT NOT NULL UNIQUE REFERENCES candidate_transactions(id),
               from_canonical_transaction_id TEXT NOT NULL UNIQUE REFERENCES canonical_transactions(id),
               to_canonical_transaction_id TEXT NOT NULL UNIQUE REFERENCES canonical_transactions(id),
               accepted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             );",
        )
        .expect("create legacy transfer-pair table");

    apply_migrations(&connection).expect("migrate transfer-pair kind");

    let sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='canonical_transfer_pairs'",
            [],
            |row| row.get(0),
        )
        .expect("read migrated table SQL");
    let index_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN (
               'idx_canonical_transfer_pairs_from_candidate',
               'idx_canonical_transfer_pairs_to_candidate'
             )",
            [],
            |row| row.get(0),
        )
        .expect("count rebuilt indexes");
    assert!(sql.contains("'own_account_transfer'"));
    assert_eq!(index_count, 2);
}

#[test]
fn migrations_create_investment_instrument_and_append_only_allocation_tables() {
    let (_dir, connection) = temporary_database();
    apply_migrations(&connection).expect("apply migrations");

    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN (
                'investment_instruments',
                'investment_allocation_revisions',
                'investment_allocation_heads'
            )",
            [],
            |row| row.get(0),
        )
        .expect("count investment tables");
    assert_eq!(count, 3);
}

#[test]
fn migrations_create_append_only_cdt_lifecycle_tables() {
    let (_dir, connection) = temporary_database();
    apply_migrations(&connection).expect("apply migrations");

    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN (
                'cdt_positions',
                'cdt_operation_revisions',
                'cdt_operation_heads',
                'investment_allocation_consumptions'
            )",
            [],
            |row| row.get(0),
        )
        .expect("count CDT lifecycle tables");
    assert_eq!(count, 4);
    let allocation_claim_is_primary_key: i64 = connection
        .query_row(
            "SELECT pk FROM pragma_table_info('investment_allocation_consumptions')
             WHERE name = 'allocation_id'",
            [],
            |row| row.get(0),
        )
        .expect("read durable allocation claim key");
    assert_eq!(allocation_claim_is_primary_key, 1);
}

#[test]
fn migrations_add_duplicate_count_to_existing_import_batches() {
    let (_dir, connection) = temporary_database();
    connection
        .execute_batch(
            "CREATE TABLE import_batches (
                id TEXT PRIMARY KEY,
                source_document_id TEXT NOT NULL,
                started_at TEXT NOT NULL,
                completed_at TEXT,
                status TEXT NOT NULL CHECK (status IN ('completed', 'completed_with_errors', 'failed')),
                candidate_count INTEGER NOT NULL DEFAULT 0 CHECK (candidate_count >= 0),
                error_count INTEGER NOT NULL DEFAULT 0 CHECK (error_count >= 0),
                error_details_json TEXT NOT NULL DEFAULT '[]'
            );
            INSERT INTO import_batches (
                id, source_document_id, started_at, status, candidate_count, error_count
            ) VALUES ('batch_legacy', 'srcdoc_legacy', '2026-07-05T00:00:00Z', 'completed', 1, 0);",
        )
        .expect("seed legacy import_batches table");

    apply_migrations(&connection).expect("apply migrations");

    let duplicate_count: i64 = connection
        .query_row(
            "SELECT duplicate_count FROM import_batches WHERE id = 'batch_legacy'",
            [],
            |row| row.get(0),
        )
        .expect("read backfilled duplicate_count");
    assert_eq!(duplicate_count, 0);
}

#[test]
fn migrations_add_semantic_hint_to_existing_candidate_transactions() {
    let (_dir, connection) = temporary_database();
    connection
        .execute_batch(
            "CREATE TABLE candidate_transactions (
                id TEXT PRIMARY KEY,
                import_batch_id TEXT NOT NULL,
                source_document_id TEXT NOT NULL,
                posted_date TEXT NOT NULL,
                description TEXT NOT NULL,
                amount_minor INTEGER NOT NULL,
                currency TEXT NOT NULL,
                direction_hint TEXT CHECK (direction_hint IN ('inflow', 'outflow')),
                confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
                status TEXT NOT NULL CHECK (status IN ('pending_review', 'possible_duplicate', 'accepted', 'rejected')),
                duplicate_status TEXT NOT NULL DEFAULT 'not_checked' CHECK (duplicate_status IN ('not_checked', 'unique', 'possible_duplicate', 'exact_duplicate')),
                fingerprint TEXT,
                validation_warnings_json TEXT NOT NULL DEFAULT '[]'
            );
            INSERT INTO candidate_transactions (
                id, import_batch_id, source_document_id, posted_date, description,
                amount_minor, currency, direction_hint, confidence, status, duplicate_status
            ) VALUES (
                'cand_legacy', 'batch_legacy', 'srcdoc_legacy', '2026-07-05', 'Redacted',
                1000, 'COP', 'inflow', 0.9, 'pending_review', 'not_checked'
            );",
        )
        .expect("seed legacy candidate_transactions table");

    apply_migrations(&connection).expect("apply migrations");

    let semantic_hint: Option<String> = connection
        .query_row(
            "SELECT semantic_hint FROM candidate_transactions WHERE id = 'cand_legacy'",
            [],
            |row| row.get(0),
        )
        .expect("read nullable semantic_hint");
    assert_eq!(semantic_hint, None);
    let resolution: String = connection
        .query_row(
            "SELECT account_resolution_json FROM candidate_transactions WHERE id = 'cand_legacy'",
            [],
            |row| row.get(0),
        )
        .expect("read backfilled account resolution");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&resolution).unwrap()["reason"],
        "not_evaluated"
    );
}

#[test]
fn migrations_add_pending_investment_allocation_to_existing_canonical_ledger() {
    let (_dir, connection) = temporary_database();
    connection
        .execute_batch(
            "CREATE TABLE canonical_transactions (
                id TEXT PRIMARY KEY,
                posted_date TEXT NOT NULL,
                description TEXT NOT NULL,
                amount_minor INTEGER NOT NULL,
                currency TEXT NOT NULL,
                transaction_kind TEXT
            );
            INSERT INTO canonical_transactions (
                id, posted_date, description, amount_minor, currency, transaction_kind
            ) VALUES ('txn_legacy', '2026-07-01', 'Legacy expense', -1000, 'COP', 'expense');",
        )
        .expect("seed legacy canonical ledger");

    apply_migrations(&connection).expect("apply migrations");

    let allocation_status: Option<String> = connection
        .query_row(
            "SELECT investment_allocation_status FROM canonical_transactions WHERE id = 'txn_legacy'",
            [],
            |row| row.get(0),
        )
        .expect("read nullable allocation status");
    assert_eq!(allocation_status, None);
    let fee_component_id: Option<String> = connection
        .query_row(
            "SELECT investment_fee_component_id FROM canonical_transactions WHERE id = 'txn_legacy'",
            [],
            |row| row.get(0),
        )
        .expect("read nullable investment fee component");
    assert_eq!(fee_component_id, None);
}

#[test]
fn can_insert_and_read_core_review_first_records_without_canonical_promotion() {
    let (_dir, connection) = temporary_database();
    apply_migrations(&connection).expect("apply migrations");

    connection
        .execute(
            "INSERT INTO institutions (id, name) VALUES (?1, ?2)",
            params!["inst_nequi", "Nequi"],
        )
        .expect("insert institution");
    connection
        .execute(
            "INSERT INTO accounts (id, institution_id, label, currency, masked_identifier, kind)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                "acct_nequi_wallet",
                "inst_nequi",
                "Nequi wallet",
                "COP",
                "***1234",
                "wallet"
            ],
        )
        .expect("insert account");
    connection
        .execute(
            "INSERT INTO source_documents (
                id, input_name, content_sha256, mime_type, byte_size,
                institution_id, institution_hint, account_id,
                account_label_hint, account_currency_hint, account_masked_identifier_hint
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                "srcdoc_test",
                "nequi-redacted.pdf",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "application/pdf",
                1234_i64,
                "inst_nequi",
                "nequi",
                "acct_nequi_wallet",
                "Nequi wallet",
                "COP",
                "***1234"
            ],
        )
        .expect("insert source document");
    connection
        .execute(
            "INSERT INTO import_batches (
                id, source_document_id, started_at, completed_at,
                status, candidate_count, error_count
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                "batch_test",
                "srcdoc_test",
                "2026-07-05T00:00:00Z",
                "2026-07-05T00:00:01Z",
                "completed",
                1_i64,
                0_i64
            ],
        )
        .expect("insert import batch");
    connection
        .execute(
            "INSERT INTO candidate_transactions (
                id, import_batch_id, source_document_id, institution_id, institution_hint,
                account_id, account_label_hint, account_currency_hint, account_masked_identifier_hint,
                posted_date, description, amount_minor, currency, balance_minor,
                direction_hint, confidence, status, duplicate_status, fingerprint,
                validation_warnings_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                "cand_test",
                "batch_test",
                "srcdoc_test",
                "inst_nequi",
                "nequi",
                "acct_nequi_wallet",
                "Nequi wallet",
                "COP",
                "***1234",
                "2026-05-31",
                "Redacted merchant",
                -4590000_i64,
                "COP",
                12500000_i64,
                "outflow",
                0.91_f64,
                "pending_review",
                "not_checked",
                "fp_test",
                "[]"
            ],
        )
        .expect("insert candidate transaction");
    connection
        .execute(
            "INSERT INTO provenance (
                id, candidate_transaction_id, source_document_id, import_batch_id,
                page_number, row_index, bbox_x, bbox_y, bbox_width, bbox_height, bbox_unit,
                extractor_name, extractor_version, parser_id, parser_version,
                evidence_redaction, evidence_text_redacted, raw_storage_policy, confidence
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                "prov_test",
                "cand_test",
                "srcdoc_test",
                "batch_test",
                2_i64,
                17_i64,
                42.1_f64,
                510.4_f64,
                496.0_f64,
                12.0_f64,
                "pdf_point",
                "pdf_oxide",
                Option::<String>::None,
                "nequi.statement.v1",
                "1",
                "redacted",
                "2026-05-31 REDACTED_COUNTERPARTY -$REDACTED balance $REDACTED",
                "local_only_optional",
                0.91_f64
            ],
        )
        .expect("insert provenance");
    connection
        .execute(
            "INSERT INTO transaction_fingerprints (
                id, fingerprint, candidate_transaction_id, duplicate_status,
                normalized_account_key, normalized_posted_date, normalized_amount_minor,
                normalized_currency, normalized_description
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                "fp_row_test",
                "fp_test",
                "cand_test",
                "not_checked",
                "acct_nequi_wallet",
                "2026-05-31",
                -4590000_i64,
                "COP",
                "redacted merchant"
            ],
        )
        .expect("insert fingerprint");

    let row: (String, String, i64, String, i64, String, String) = connection
        .query_row(
            "SELECT c.status, c.duplicate_status, c.amount_minor, c.currency,
                    p.page_number, p.raw_storage_policy, p.evidence_text_redacted
             FROM candidate_transactions c
             JOIN provenance p ON p.candidate_transaction_id = c.id
             WHERE c.id = 'cand_test'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .expect("read candidate with provenance");
    let canonical_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM canonical_transactions", [], |row| {
            row.get(0)
        })
        .expect("count canonical transactions");

    assert_eq!(
        row,
        (
            "pending_review".to_string(),
            "not_checked".to_string(),
            -4590000,
            "COP".to_string(),
            2,
            "local_only_optional".to_string(),
            "2026-05-31 REDACTED_COUNTERPARTY -$REDACTED balance $REDACTED".to_string(),
        )
    );
    assert_eq!(canonical_count, 0);
}

#[test]
fn migrations_constrain_contract_statuses_and_source_hash_uniqueness() {
    let (_dir, connection) = temporary_database();
    apply_migrations(&connection).expect("apply migrations");

    connection
        .execute(
            "INSERT INTO source_documents (id, input_name, content_sha256, mime_type, byte_size)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                "srcdoc_one",
                "one.pdf",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "application/pdf",
                10_i64
            ],
        )
        .expect("insert source document");
    connection
        .execute(
            "INSERT INTO import_batches (id, source_document_id, started_at, status)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                "batch_one",
                "srcdoc_one",
                "2026-07-05T00:00:00Z",
                "completed"
            ],
        )
        .expect("insert import batch");

    let invalid_batch_status = connection.execute(
        "INSERT INTO import_batches (id, source_document_id, started_at, status)
         VALUES (?1, ?2, ?3, ?4)",
        params!["batch_bad", "srcdoc_one", "2026-07-05T00:00:00Z", "running"],
    );
    assert!(matches!(
        invalid_batch_status,
        Err(Error::SqliteFailure(_, _))
    ));

    let duplicate_hash = connection.execute(
        "INSERT INTO source_documents (id, input_name, content_sha256, mime_type, byte_size)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            "srcdoc_duplicate",
            "duplicate.pdf",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "application/pdf",
            10_i64
        ],
    );
    assert!(matches!(duplicate_hash, Err(Error::SqliteFailure(_, _))));

    let invalid_candidate_status = connection.execute(
        "INSERT INTO candidate_transactions (
            id, import_batch_id, source_document_id, posted_date, description,
            amount_minor, currency, confidence, status, duplicate_status
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            "cand_bad_status",
            "batch_one",
            "srcdoc_one",
            "2026-05-31",
            "Redacted merchant",
            -100_i64,
            "COP",
            0.5_f64,
            "imported",
            "not_checked"
        ],
    );
    assert!(matches!(
        invalid_candidate_status,
        Err(Error::SqliteFailure(_, _))
    ));

    let invalid_duplicate_status = connection.execute(
        "INSERT INTO candidate_transactions (
            id, import_batch_id, source_document_id, posted_date, description,
            amount_minor, currency, confidence, status, duplicate_status
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            "cand_bad_duplicate",
            "batch_one",
            "srcdoc_one",
            "2026-05-31",
            "Redacted merchant",
            -100_i64,
            "COP",
            0.5_f64,
            "pending_review",
            "duplicate"
        ],
    );
    assert!(matches!(
        invalid_duplicate_status,
        Err(Error::SqliteFailure(_, _))
    ));
}

#[test]
fn migrations_rebuild_legacy_provenance_for_provider_events() {
    let (_dir, connection) = temporary_database();
    apply_migrations(&connection).unwrap();
    connection.execute_batch("DROP TABLE provenance; CREATE TABLE provenance (
      id TEXT PRIMARY KEY, candidate_transaction_id TEXT UNIQUE, canonical_transaction_id TEXT,
      source_document_id TEXT NOT NULL, import_batch_id TEXT, page_number INTEGER, row_index INTEGER,
      bbox_x REAL,bbox_y REAL,bbox_width REAL,bbox_height REAL,bbox_unit TEXT,
      extractor_name TEXT NOT NULL,extractor_version TEXT,parser_id TEXT NOT NULL,parser_version TEXT NOT NULL,
      evidence_redaction TEXT NOT NULL,evidence_text_redacted TEXT NOT NULL,raw_storage_policy TEXT NOT NULL,
      raw_evidence_ref TEXT,confidence REAL NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
      CHECK(candidate_transaction_id IS NOT NULL OR canonical_transaction_id IS NOT NULL));").unwrap();
    connection.execute("INSERT INTO provenance(id,canonical_transaction_id,source_document_id,extractor_name,parser_id,parser_version,evidence_redaction,evidence_text_redacted,raw_storage_policy,confidence) VALUES('legacy_prov','legacy_txn','legacy_source','legacy_extractor','legacy_parser','1','REDACTED','REDACTED','redacted_only',1)",[]).unwrap();
    apply_migrations(&connection).unwrap();
    let sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='provenance'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(sql.contains("investment_document_event_id IS NOT NULL"));
    let index_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='idx_provenance_candidate_transaction'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(index_count, 1);
    let evidence: String = connection
        .query_row(
            "SELECT evidence_redaction FROM provenance WHERE id='legacy_prov'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(evidence, "REDACTED");
}

#[test]
fn migrations_reject_invalid_provider_document_vocabulary() {
    let (_dir, connection) = temporary_database();
    apply_migrations(&connection).unwrap();
    connection.execute("INSERT INTO source_documents(id,input_name,content_sha256,mime_type,byte_size) VALUES('src_vocab','redacted.pdf',?1,'application/pdf',1)",["cd".repeat(32)]).unwrap();
    connection.execute("INSERT INTO import_batches(id,source_document_id,started_at,status,error_details_json) VALUES('batch_vocab','src_vocab','2026-06-01T00:00:00Z','completed','[]')",[]).unwrap();
    let invalid=connection.execute("INSERT INTO investment_document_events(id,source_document_id,import_batch_id,provider,parser_id,parser_version,event_type,provider_effective_date,currency,amount_minor,page_number,row_index,evidence_redaction,fingerprint,status) VALUES('event_vocab','src_vocab','batch_vocab','invented','parser','1','deposit','2026-06-01','COP',1,1,1,'REDACTED','fp_vocab','pending_review')",[]);
    assert!(invalid.is_err());
}
