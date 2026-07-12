use anyhow::{Context, Result};
use rusqlite::{backup::Backup, types::ValueRef, Connection, OpenFlags};
use serde::Serialize;
use serde_json::{Map, Value};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

pub const BACKUP_SCHEMA_VERSION: &str = "tracky.backup.v1";
pub const INTEGRITY_SCHEMA_VERSION: &str = "tracky.integrity.v1";
pub const EXPORT_SCHEMA_VERSION: &str = "tracky.export.v1";
const EXPECTED_USER_VERSION: i64 = 1;

#[derive(Debug, Serialize)]
pub struct OperationError {
    pub category: &'static str,
    pub code: &'static str,
    pub message: String,
    pub path: &'static str,
}

#[derive(Debug, Serialize)]
pub struct BackupResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    pub source: String,
    pub destination: Option<String>,
    pub errors: Vec<OperationError>,
}

#[derive(Debug, Serialize)]
pub struct Count {
    pub entity: &'static str,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct IntegrityFinding {
    pub category: &'static str,
    pub code: &'static str,
    pub entity: &'static str,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct IntegrityResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    pub sqlite_integrity: String,
    pub user_version: Option<i64>,
    pub counts: Vec<Count>,
    pub findings: Vec<IntegrityFinding>,
    pub errors: Vec<OperationError>,
}

#[derive(Debug, Serialize)]
pub struct ExportResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    pub include_review_audit: bool,
    pub entities: Map<String, Value>,
    pub errors: Vec<OperationError>,
}

fn error(
    category: &'static str,
    code: &'static str,
    message: impl Into<String>,
    path: &'static str,
) -> OperationError {
    OperationError {
        category,
        code,
        message: message.into(),
        path,
    }
}

fn readonly(path: &Path) -> Result<Connection> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .with_context(|| format!("opening read-only SQLite database {}", path.display()))
}

pub fn backup(source: &Path, destination: &Path) -> BackupResponse {
    let mut response = BackupResponse {
        schema_version: BACKUP_SCHEMA_VERSION,
        command: "backup",
        ok: false,
        source: source.display().to_string(),
        destination: None,
        errors: vec![],
    };
    if destination.exists() {
        response.errors.push(error(
            "operational",
            "destination_exists",
            "Backup destination already exists.",
            "destination",
        ));
        return response;
    }
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .and_then(|x| x.to_str())
        .unwrap_or("backup.sqlite3");
    let temporary = parent.join(format!(".{name}.tracky-tmp-{}", std::process::id()));
    let outcome = (|| -> Result<()> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("creating temporary backup {}", temporary.display()))?;
        let source_db = readonly(source)?;
        let mut destination_db = Connection::open(&temporary)?;
        {
            let backup = Backup::new(&source_db, &mut destination_db)?;
            backup.run_to_completion(64, std::time::Duration::from_millis(1), None)?;
        }
        let check: String =
            destination_db.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if check != "ok" {
            anyhow::bail!("backup integrity_check returned {check}");
        }
        drop(destination_db);
        fs::hard_link(&temporary, destination)
            .with_context(|| format!("publishing backup {}", destination.display()))?;
        fs::remove_file(&temporary).context("removing temporary backup link")?;
        Ok(())
    })();
    match outcome {
        Ok(()) => {
            response.ok = true;
            response.destination = Some(destination.display().to_string());
        }
        Err(e) => {
            let _ = fs::remove_file(&temporary);
            response.errors.push(error(
                "operational",
                "backup_failed",
                e.to_string(),
                "backup",
            ));
        }
    }
    response
}

pub fn integrity(path: &Path) -> IntegrityResponse {
    let mut r = IntegrityResponse {
        schema_version: INTEGRITY_SCHEMA_VERSION,
        command: "integrity",
        ok: false,
        sqlite_integrity: "not_run".into(),
        user_version: None,
        counts: vec![],
        findings: vec![],
        errors: vec![],
    };
    let c = match readonly(path) {
        Ok(c) => c,
        Err(e) => {
            r.errors.push(error(
                "operational",
                "database_open_failed",
                e.to_string(),
                "db",
            ));
            return r;
        }
    };
    match c.query_row::<String, _, _>("PRAGMA integrity_check", [], |x| x.get(0)) {
        Ok(v) => {
            r.sqlite_integrity = v;
            if r.sqlite_integrity != "ok" {
                r.findings.push(IntegrityFinding {
                    category: "sqlite_corruption",
                    code: "sqlite_integrity_failed",
                    entity: "database",
                    count: 1,
                });
            }
        }
        Err(e) => {
            r.errors.push(error(
                "sqlite_corruption",
                "integrity_check_failed",
                e.to_string(),
                "db",
            ));
            return r;
        }
    }
    r.user_version = c.query_row("PRAGMA user_version", [], |x| x.get(0)).ok();
    if r.user_version != Some(EXPECTED_USER_VERSION) {
        r.findings.push(IntegrityFinding {
            category: "schema_incompatibility",
            code: "unsupported_schema_version",
            entity: "schema",
            count: 1,
        });
    }
    let tables = [
        ("accounts", "accounts"),
        ("categories", "categories"),
        ("income_sources", "income_sources"),
        ("canonical_transactions", "canonical_transactions"),
        ("transaction_lines", "transaction_lines"),
        ("canonical_transfer_pairs", "canonical_transfer_pairs"),
        ("provenance", "provenance"),
    ];
    for (entity, table) in tables {
        match c.query_row(&format!("SELECT count(*) FROM {table}"), [], |x| {
            x.get::<_, i64>(0)
        }) {
            Ok(count) => r.counts.push(Count { entity, count }),
            Err(e) => {
                r.errors.push(error(
                    "schema_incompatibility",
                    "required_table_unavailable",
                    e.to_string(),
                    "schema",
                ));
                return r;
            }
        }
    }
    let checks = [
      ("broken_reference","transaction_account_missing","canonical_transactions", "SELECT count(*) FROM canonical_transactions t LEFT JOIN accounts a ON a.id=t.account_id WHERE t.account_id IS NOT NULL AND a.id IS NULL"),
      ("broken_reference","transaction_income_source_missing","canonical_transactions", "SELECT count(*) FROM canonical_transactions t LEFT JOIN income_sources s ON s.id=t.income_source_id WHERE t.income_source_id IS NOT NULL AND s.id IS NULL"),
      ("broken_reference","line_transaction_missing","transaction_lines", "SELECT count(*) FROM transaction_lines l LEFT JOIN canonical_transactions t ON t.id=l.canonical_transaction_id WHERE t.id IS NULL"),
      ("broken_reference","line_category_missing","transaction_lines", "SELECT count(*) FROM transaction_lines l LEFT JOIN categories c ON c.id=l.category_id WHERE c.id IS NULL"),
      ("broken_reference","transfer_leg_missing_or_incompatible","canonical_transfer_pairs", "SELECT count(*) FROM canonical_transfer_pairs p LEFT JOIN accounts fa ON fa.id=p.from_account_id LEFT JOIN accounts ta ON ta.id=p.to_account_id LEFT JOIN canonical_transactions f ON f.id=p.from_canonical_transaction_id LEFT JOIN canonical_transactions t ON t.id=p.to_canonical_transaction_id WHERE fa.id IS NULL OR ta.id IS NULL OR f.id IS NULL OR t.id IS NULL OR f.account_id<>p.from_account_id OR t.account_id<>p.to_account_id OR f.currency<>p.currency OR t.currency<>p.currency OR abs(f.amount_minor)<>p.amount_minor OR abs(t.amount_minor)<>p.amount_minor"),
      ("broken_reference","provenance_target_missing","provenance", "SELECT count(*) FROM provenance p LEFT JOIN candidate_transactions c ON c.id=p.candidate_transaction_id LEFT JOIN canonical_transactions t ON t.id=p.canonical_transaction_id WHERE (p.candidate_transaction_id IS NOT NULL AND c.id IS NULL) OR (p.canonical_transaction_id IS NOT NULL AND t.id IS NULL)"),
    ];
    for (category, code, entity, sql) in checks {
        match c.query_row(sql, [], |x| x.get::<_, i64>(0)) {
            Ok(count) if count > 0 => r.findings.push(IntegrityFinding {
                category,
                code,
                entity,
                count,
            }),
            Ok(_) => {}
            Err(e) => {
                r.errors.push(error(
                    "operational",
                    "integrity_query_failed",
                    e.to_string(),
                    "db",
                ));
                return r;
            }
        }
    }
    r.ok = r.sqlite_integrity == "ok" && r.findings.is_empty() && r.errors.is_empty();
    r
}

fn rows(c: &Connection, sql: &str) -> Result<Value> {
    let mut statement = c.prepare(sql)?;
    let names: Vec<String> = statement
        .column_names()
        .iter()
        .map(|x| (*x).to_owned())
        .collect();
    let values = statement
        .query_map([], |row| {
            let mut object = Map::new();
            for (i, name) in names.iter().enumerate() {
                let value = match row.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(v) => Value::from(v),
                    ValueRef::Real(v) => Value::from(v),
                    ValueRef::Text(v) => Value::String(String::from_utf8_lossy(v).into_owned()),
                    ValueRef::Blob(_) => Value::String("<redacted-binary>".into()),
                };
                object.insert(name.clone(), value);
            }
            Ok(Value::Object(object))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Value::Array(values))
}

pub fn export(path: &Path, include_review_audit: bool) -> ExportResponse {
    let mut r = ExportResponse {
        schema_version: EXPORT_SCHEMA_VERSION,
        command: "export",
        ok: false,
        include_review_audit,
        entities: Map::new(),
        errors: vec![],
    };
    let c = match readonly(path) {
        Ok(c) => c,
        Err(e) => {
            r.errors.push(error(
                "operational",
                "database_open_failed",
                e.to_string(),
                "db",
            ));
            return r;
        }
    };
    let queries=[
      ("accounts", "SELECT id,institution_id,label,currency,masked_identifier,kind,is_owned,created_at FROM accounts ORDER BY id"),
      ("categories", "SELECT id,name,created_at FROM categories ORDER BY id"),
      ("income_sources", "SELECT id,name,created_at FROM income_sources ORDER BY id"),
      ("canonical_transactions", "SELECT id,account_id,posted_date,description,amount_minor,currency,balance_minor,transaction_kind,investment_allocation_status,income_source_id,income_kind,investment_fee_component_id,external_reference,created_from_candidate_id,created_at FROM canonical_transactions ORDER BY posted_date,id"),
      ("transaction_lines", "SELECT id,canonical_transaction_id,category_id,amount_minor,currency,line_kind,created_at FROM transaction_lines ORDER BY canonical_transaction_id,id"),
      ("transfer_pairs", "SELECT id,transfer_kind,posted_date,amount_minor,currency,from_account_id,to_account_id,from_candidate_id,to_candidate_id,from_canonical_transaction_id,to_canonical_transaction_id,accepted_at FROM canonical_transfer_pairs ORDER BY posted_date,id"),
      ("provenance", "SELECT id,canonical_transaction_id,source_document_id,import_batch_id,page_number,row_index,extractor_name,extractor_version,parser_id,parser_version,evidence_redaction,evidence_text_redacted,raw_storage_policy,confidence,created_at FROM provenance WHERE canonical_transaction_id IS NOT NULL ORDER BY canonical_transaction_id,id")];
    for (name, sql) in queries {
        match rows(&c, sql) {
            Ok(v) => {
                r.entities.insert(name.into(), v);
            }
            Err(e) => {
                r.errors.push(error(
                    "schema_incompatibility",
                    "export_query_failed",
                    e.to_string(),
                    "schema",
                ));
                return r;
            }
        }
    }
    if include_review_audit {
        let extra=[
      ("review_candidates", "SELECT id,import_batch_id,source_document_id,institution_id,institution_hint,account_id,account_label_hint,account_currency_hint,account_masked_identifier_hint,posted_date,description,amount_minor,currency,balance_minor,direction_hint,semantic_hint,confidence,status,duplicate_status,fingerprint,validation_warnings_json,canonical_transaction_id,created_at FROM candidate_transactions ORDER BY import_batch_id,id"),
      ("import_batches", "SELECT id,source_document_id,started_at,completed_at,status,candidate_count,error_count,duplicate_count,error_details_json FROM import_batches ORDER BY started_at,id"),
      ("source_documents", "SELECT id,mime_type,byte_size,institution_id,institution_hint,account_id,account_label_hint,account_currency_hint,account_masked_identifier_hint,imported_at,duplicate_of_source_document_id FROM source_documents ORDER BY imported_at,id"),
      ("review_provenance", "SELECT id,candidate_transaction_id,canonical_transaction_id,source_document_id,import_batch_id,page_number,row_index,extractor_name,extractor_version,parser_id,parser_version,evidence_redaction,evidence_text_redacted,raw_storage_policy,confidence,created_at FROM provenance WHERE candidate_transaction_id IS NOT NULL ORDER BY candidate_transaction_id,id")];
        for (name, sql) in extra {
            match rows(&c, sql) {
                Ok(v) => {
                    r.entities.insert(name.into(), v);
                }
                Err(e) => {
                    r.errors.push(error(
                        "schema_incompatibility",
                        "export_query_failed",
                        e.to_string(),
                        "schema",
                    ));
                    return r;
                }
            }
        }
    }
    r.ok = true;
    r
}

pub fn default_backup_path(source: &Path, timestamp: &str) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|x| x.to_str())
        .unwrap_or("tracky");
    source.with_file_name(format!("{stem}-{timestamp}.sqlite3"))
}
