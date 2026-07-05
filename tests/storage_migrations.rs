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
