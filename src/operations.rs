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
    pub category: OperationErrorCategory,
    pub code: &'static str,
    pub message: String,
    pub path: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationErrorCategory {
    Operational,
    SqliteCorruption,
    SchemaIncompatibility,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingCategory {
    SqliteCorruption,
    SchemaIncompatibility,
    BrokenReference,
    InvariantViolation,
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
    pub category: FindingCategory,
    pub code: &'static str,
    pub entity: &'static str,
    pub id: String,
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
    category: OperationErrorCategory,
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
            OperationErrorCategory::Operational,
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
        // Publication is the commit point. Failure to remove the hidden second link does not
        // invalidate the already verified, atomically published backup.
        let _ = fs::remove_file(&temporary);
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
                OperationErrorCategory::Operational,
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
                OperationErrorCategory::Operational,
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
                    category: FindingCategory::SqliteCorruption,
                    code: "sqlite_integrity_failed",
                    entity: "database",
                    id: "database:sqlite_integrity_failed".into(),
                    count: 1,
                });
            }
        }
        Err(e) => {
            let category = match &e {
                rusqlite::Error::SqliteFailure(failure, _)
                    if failure.code == rusqlite::ErrorCode::DatabaseCorrupt =>
                {
                    OperationErrorCategory::SqliteCorruption
                }
                _ => OperationErrorCategory::Operational,
            };
            r.errors.push(error(
                category,
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
            category: FindingCategory::SchemaIncompatibility,
            code: "unsupported_schema_version",
            entity: "schema",
            id: "schema:unsupported_schema_version".into(),
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
        ("investment_instruments", "investment_instruments"),
        (
            "investment_allocation_revisions",
            "investment_allocation_revisions",
        ),
        ("investment_allocation_heads", "investment_allocation_heads"),
        (
            "investment_allocation_consumptions",
            "investment_allocation_consumptions",
        ),
        ("cdt_positions", "cdt_positions"),
        ("cdt_operation_revisions", "cdt_operation_revisions"),
        ("cdt_operation_heads", "cdt_operation_heads"),
        ("brokerage_accounts", "brokerage_accounts"),
        (
            "brokerage_operation_revisions",
            "brokerage_operation_revisions",
        ),
        ("brokerage_operation_heads", "brokerage_operation_heads"),
        (
            "brokerage_buy_funding_attributions",
            "brokerage_buy_funding_attributions",
        ),
        ("investment_snapshots", "investment_snapshots"),
        (
            "investment_snapshot_positions",
            "investment_snapshot_positions",
        ),
        (
            "investment_snapshot_baselines",
            "investment_snapshot_baselines",
        ),
        (
            "investment_adjustment_revisions",
            "investment_adjustment_revisions",
        ),
        ("investment_adjustment_heads", "investment_adjustment_heads"),
        ("investment_document_events", "investment_document_events"),
        (
            "candidate_account_assignment_events",
            "candidate_account_assignment_events",
        ),
        (
            "candidate_transfer_decisions",
            "candidate_transfer_decisions",
        ),
    ];
    for (entity, table) in tables {
        match c.query_row(&format!("SELECT count(*) FROM {table}"), [], |x| {
            x.get::<_, i64>(0)
        }) {
            Ok(count) => r.counts.push(Count { entity, count }),
            Err(e) => {
                r.errors.push(error(
                    OperationErrorCategory::SchemaIncompatibility,
                    "required_table_unavailable",
                    e.to_string(),
                    "schema",
                ));
                return r;
            }
        }
    }
    struct IntegrityCheck {
        category: FindingCategory,
        code: &'static str,
        entity: &'static str,
        sql: &'static str,
    }
    let checks = [
      IntegrityCheck { category:FindingCategory::BrokenReference, code:"transaction_account_missing", entity:"canonical_transactions", sql:"SELECT count(*) FROM canonical_transactions t LEFT JOIN accounts a ON a.id=t.account_id WHERE t.account_id IS NOT NULL AND a.id IS NULL" },
      IntegrityCheck { category:FindingCategory::BrokenReference, code:"transaction_income_source_missing", entity:"canonical_transactions", sql:"SELECT count(*) FROM canonical_transactions t LEFT JOIN income_sources s ON s.id=t.income_source_id WHERE t.income_source_id IS NOT NULL AND s.id IS NULL" },
      IntegrityCheck { category:FindingCategory::BrokenReference, code:"line_transaction_missing", entity:"transaction_lines", sql:"SELECT count(*) FROM transaction_lines l LEFT JOIN canonical_transactions t ON t.id=l.canonical_transaction_id WHERE t.id IS NULL" },
      IntegrityCheck { category:FindingCategory::BrokenReference, code:"line_category_missing", entity:"transaction_lines", sql:"SELECT count(*) FROM transaction_lines l LEFT JOIN categories c ON c.id=l.category_id WHERE c.id IS NULL" },
      IntegrityCheck { category:FindingCategory::BrokenReference, code:"transfer_leg_missing_or_incompatible", entity:"canonical_transfer_pairs", sql:"SELECT count(*) FROM canonical_transfer_pairs p LEFT JOIN accounts fa ON fa.id=p.from_account_id LEFT JOIN accounts ta ON ta.id=p.to_account_id LEFT JOIN canonical_transactions f ON f.id=p.from_canonical_transaction_id LEFT JOIN canonical_transactions t ON t.id=p.to_canonical_transaction_id WHERE fa.id IS NULL OR ta.id IS NULL OR f.id IS NULL OR t.id IS NULL OR f.account_id<>p.from_account_id OR t.account_id<>p.to_account_id OR f.currency<>p.currency OR t.currency<>p.currency OR f.amount_minor<>-p.amount_minor OR t.amount_minor<>p.amount_minor" },
      IntegrityCheck { category:FindingCategory::BrokenReference, code:"manual_transfer_leg_missing_or_incompatible", entity:"manual_transfer_pairs", sql:"SELECT count(*) FROM manual_transfer_pairs p LEFT JOIN accounts fa ON fa.id=p.from_account_id LEFT JOIN accounts ta ON ta.id=p.to_account_id LEFT JOIN canonical_transactions f ON f.id=p.from_canonical_transaction_id LEFT JOIN canonical_transactions t ON t.id=p.to_canonical_transaction_id WHERE fa.id IS NULL OR ta.id IS NULL OR f.id IS NULL OR t.id IS NULL OR f.account_id<>p.from_account_id OR t.account_id<>p.to_account_id OR f.currency<>p.currency OR t.currency<>p.currency OR f.amount_minor<>-p.amount_minor OR t.amount_minor<>p.amount_minor" },
      IntegrityCheck { category:FindingCategory::BrokenReference, code:"provenance_target_missing", entity:"provenance", sql:"SELECT count(*) FROM provenance p LEFT JOIN candidate_transactions c ON c.id=p.candidate_transaction_id LEFT JOIN canonical_transactions t ON t.id=p.canonical_transaction_id WHERE (p.candidate_transaction_id IS NOT NULL AND c.id IS NULL) OR (p.canonical_transaction_id IS NOT NULL AND t.id IS NULL)" },
      IntegrityCheck { category:FindingCategory::BrokenReference, code:"manual_provenance_target_missing", entity:"manual_transaction_provenance", sql:"SELECT count(*) FROM manual_transaction_provenance p LEFT JOIN canonical_transactions t ON t.id=p.canonical_transaction_id WHERE t.id IS NULL" },
      IntegrityCheck { category:FindingCategory::BrokenReference, code:"candidate_account_assignment_invalid", entity:"candidate_account_assignment_events", sql:"SELECT count(*) FROM candidate_account_assignment_events e LEFT JOIN candidate_transactions c ON c.id=e.candidate_transaction_id LEFT JOIN accounts a ON a.id=e.account_id WHERE c.id IS NULL OR a.id IS NULL OR a.is_owned<>1 OR upper(a.currency)<>upper(c.currency)" },
      IntegrityCheck { category:FindingCategory::InvariantViolation, code:"candidate_account_assignment_history_invalid", entity:"candidate_account_assignment_events", sql:"SELECT count(*) FROM candidate_account_assignment_events e LEFT JOIN accounts previous_account ON previous_account.id=e.previous_account_id LEFT JOIN candidate_account_assignment_events previous_event ON previous_event.candidate_transaction_id=e.candidate_transaction_id AND previous_event.revision=e.revision-1 WHERE (e.previous_account_id IS NOT NULL AND previous_account.id IS NULL) OR (e.revision>1 AND (previous_event.id IS NULL OR e.previous_account_id IS NOT previous_event.account_id))" },
      IntegrityCheck { category:FindingCategory::InvariantViolation, code:"candidate_account_assignment_head_mismatch", entity:"candidate_account_assignment_events", sql:"SELECT count(*) FROM candidate_transactions c JOIN candidate_account_assignment_events e ON e.id=(SELECT latest.id FROM candidate_account_assignment_events latest WHERE latest.candidate_transaction_id=c.id ORDER BY latest.revision DESC LIMIT 1) WHERE c.account_id IS NOT e.account_id" },
      IntegrityCheck { category:FindingCategory::InvariantViolation, code:"candidate_transfer_decision_invalid", entity:"candidate_transfer_decisions", sql:"SELECT count(DISTINCT d.id) FROM candidate_transfer_decisions d LEFT JOIN candidate_transactions c ON c.id=d.candidate_transaction_id WHERE c.id IS NULL OR d.decision<>'not_transfer' OR length(trim(d.reviewer_reason))=0 OR json_valid(d.suspicion_evidence_json)=0 OR json_type(d.suspicion_evidence_json)<>'array' OR json_array_length(d.suspicion_evidence_json)=0 OR EXISTS (SELECT 1 FROM json_each(d.suspicion_evidence_json) e LEFT JOIN candidate_transactions counterpart ON counterpart.id=json_extract(e.value,'$.counterpart_candidate_id') LEFT JOIN accounts from_account ON from_account.id=json_extract(e.value,'$.from_account_id') LEFT JOIN accounts to_account ON to_account.id=json_extract(e.value,'$.to_account_id') WHERE json_type(e.value)<>'object' OR json_type(e.value,'$.transfer_kind')<>'text' OR json_extract(e.value,'$.transfer_kind') NOT IN ('card_payment','own_account_transfer') OR json_type(e.value,'$.role')<>'text' OR json_extract(e.value,'$.role') NOT IN ('from','to') OR counterpart.id IS NULL OR counterpart.id=d.candidate_transaction_id OR json_type(e.value,'$.posted_date')<>'text' OR json_extract(e.value,'$.posted_date')<>c.posted_date OR json_extract(e.value,'$.posted_date')<>counterpart.posted_date OR json_type(e.value,'$.amount_minor')<>'integer' OR json_extract(e.value,'$.amount_minor')<>abs(c.amount_minor) OR json_extract(e.value,'$.amount_minor')<>abs(counterpart.amount_minor) OR json_type(e.value,'$.currency')<>'text' OR upper(json_extract(e.value,'$.currency'))<>upper(c.currency) OR upper(json_extract(e.value,'$.currency'))<>upper(counterpart.currency) OR from_account.id IS NULL OR from_account.is_owned<>1 OR to_account.id IS NULL OR to_account.is_owned<>1 OR (json_extract(e.value,'$.role')='from' AND (c.amount_minor>=0 OR c.account_id<>from_account.id OR counterpart.account_id<>to_account.id OR (CASE WHEN counterpart.semantic_hint='card_payment' THEN 'card_payment' ELSE 'own_account_transfer' END)<>json_extract(e.value,'$.transfer_kind'))) OR (json_extract(e.value,'$.role')='to' AND (counterpart.amount_minor>=0 OR counterpart.account_id<>from_account.id OR c.account_id<>to_account.id OR (CASE WHEN c.semantic_hint='card_payment' THEN 'card_payment' ELSE 'own_account_transfer' END)<>json_extract(e.value,'$.transfer_kind'))))" },
      IntegrityCheck { category:FindingCategory::InvariantViolation, code:"candidate_transfer_decision_contradicts_pair", entity:"candidate_transfer_decisions", sql:"SELECT count(*) FROM candidate_transfer_decisions d JOIN canonical_transfer_pairs p ON p.from_candidate_id=d.candidate_transaction_id OR p.to_candidate_id=d.candidate_transaction_id" },
    ];
    for check in checks {
        match c.query_row(check.sql, [], |x| x.get::<_, i64>(0)) {
            Ok(count) if count > 0 => r.findings.push(IntegrityFinding {
                category: check.category,
                code: check.code,
                entity: check.entity,
                id: format!("{}:{}", check.entity, check.code),
                count,
            }),
            Ok(_) => {}
            Err(e) => {
                r.errors.push(error(
                    OperationErrorCategory::Operational,
                    "integrity_query_failed",
                    e.to_string(),
                    "db",
                ));
                return r;
            }
        }
    }
    match crate::investment_integrity::findings(&c) {
        Ok(findings) => r
            .findings
            .extend(findings.into_iter().map(|finding| IntegrityFinding {
                category: match finding.kind {
                    crate::investment_integrity::Kind::Broken => FindingCategory::BrokenReference,
                    crate::investment_integrity::Kind::Invariant => {
                        FindingCategory::InvariantViolation
                    }
                },
                code: finding.code,
                entity: finding.entity,
                id: format!("{}:{}", finding.entity, finding.code),
                count: finding.count,
            })),
        Err(e) => {
            r.errors.push(error(
                OperationErrorCategory::Operational,
                "investment_integrity_failed",
                e.to_string(),
                "db",
            ));
            return r;
        }
    }
    r.findings.sort_by(|left, right| {
        format!("{:?}", left.category)
            .cmp(&format!("{:?}", right.category))
            .then(left.entity.cmp(right.entity))
            .then(left.id.cmp(&right.id))
    });
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

#[derive(Clone, Copy)]
struct ExportQuery {
    name: &'static str,
    sql: &'static str,
}

fn export_queries(
    c: &Connection,
    response: &mut ExportResponse,
    queries: &[ExportQuery],
) -> Result<()> {
    for query in queries {
        response
            .entities
            .insert(query.name.into(), rows(c, query.sql)?);
    }
    Ok(())
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
                OperationErrorCategory::Operational,
                "database_open_failed",
                e.to_string(),
                "db",
            ));
            return r;
        }
    };
    let queries=[
      ExportQuery{name:"accounts",sql:"SELECT id,institution_id,label,currency,masked_identifier,kind,is_owned,created_at FROM accounts ORDER BY id"},
      ExportQuery{name:"categories",sql:"SELECT id,name,created_at FROM categories ORDER BY id"},
      ExportQuery{name:"income_sources",sql:"SELECT id,name,created_at FROM income_sources ORDER BY id"},
      ExportQuery{name:"canonical_transactions",sql:"SELECT id,account_id,posted_date,description,amount_minor,currency,balance_minor,transaction_kind,investment_allocation_status,income_source_id,income_kind,investment_fee_component_id,external_reference,created_from_candidate_id,created_at FROM canonical_transactions ORDER BY posted_date,id"},
      ExportQuery{name:"transaction_lines",sql:"SELECT id,canonical_transaction_id,category_id,amount_minor,currency,line_kind,created_at FROM transaction_lines ORDER BY canonical_transaction_id,id"},
      ExportQuery{name:"transfer_pairs",sql:"SELECT id,transfer_kind,posted_date,amount_minor,currency,from_account_id,to_account_id,from_candidate_id,to_candidate_id,from_canonical_transaction_id,to_canonical_transaction_id,accepted_at FROM canonical_transfer_pairs ORDER BY posted_date,id"},
      ExportQuery{name:"manual_transfer_pairs",sql:"SELECT id,posted_date,amount_minor,currency,from_account_id,to_account_id,from_canonical_transaction_id,to_canonical_transaction_id,created_at FROM manual_transfer_pairs ORDER BY posted_date,id"},
      ExportQuery{name:"provenance",sql:"SELECT id,canonical_transaction_id,source_document_id,import_batch_id,page_number,row_index,extractor_name,extractor_version,parser_id,parser_version,evidence_redaction,evidence_text_redacted,raw_storage_policy,confidence,created_at FROM provenance WHERE canonical_transaction_id IS NOT NULL ORDER BY canonical_transaction_id,id"},
      ExportQuery{name:"investment_instruments",sql:"SELECT id,name,instrument_type,denomination_currency,provider,provider_identifier,created_at FROM investment_instruments ORDER BY id"},
      ExportQuery{name:"investment_allocation_revisions",sql:"SELECT id,allocation_id,revision,contribution_transaction_id,instrument_id,cash_amount_minor,cash_currency,acquired_quantity,effective_date,fee_amount_minor,fee_currency,fee_treatment,fee_component_id,fee_expense_transaction_id,provenance_source,correction_reason,replaces_revision_id,created_at FROM investment_allocation_revisions ORDER BY allocation_id,revision,id"},
      ExportQuery{name:"investment_allocation_heads",sql:"SELECT allocation_id,current_revision_id FROM investment_allocation_heads ORDER BY allocation_id"},
      ExportQuery{name:"investment_allocation_consumptions",sql:"SELECT allocation_id,consumer_kind,cdt_position_id,consumer_operation_id,created_at FROM investment_allocation_consumptions ORDER BY allocation_id"},
      ExportQuery{name:"cdt_positions",sql:"SELECT id,instrument_id,account_id,constituent_allocation_id,created_at FROM cdt_positions ORDER BY id"},
      ExportQuery{name:"cdt_operation_revisions",sql:"SELECT id,operation_id,revision,cdt_position_id,operation_type,effective_date,currency,principal_before_minor,principal_after_minor,principal_returned_minor,external_capital_minor,capitalized_interest_minor,gross_interest_minor,withholding_minor,other_deductions_minor,net_cash_received_minor,funding_allocation_id,maturity_date,agreed_rate,payment_mode,payment_periodicity,renewal_terms,contract_identifier,allows_partial_redemption,deduction_component_id,deduction_expense_transaction_id,provenance_source,correction_reason,replaces_revision_id,created_at FROM cdt_operation_revisions ORDER BY operation_id,revision,id"},
      ExportQuery{name:"cdt_operation_heads",sql:"SELECT operation_id,current_revision_id FROM cdt_operation_heads ORDER BY operation_id"},
      ExportQuery{name:"brokerage_accounts",sql:"SELECT account_id,opened_date,provenance_source,created_at FROM brokerage_accounts ORDER BY account_id"},
      ExportQuery{name:"brokerage_operation_revisions",sql:"SELECT id,operation_id,revision,account_id,operation_type,effective_date,currency,instrument_id,quantity,gross_amount_minor,historical_cost_minor,realized_result_minor,fee_minor,fee_treatment,withholding_minor,other_deductions_minor,net_cash_minor,funding_allocation_id,destination_account_id,linked_transaction_id,component_id,provenance_source,correction_reason,replaces_revision_id,created_at FROM brokerage_operation_revisions ORDER BY operation_id,revision,id"},
      ExportQuery{name:"brokerage_operation_heads",sql:"SELECT operation_id,current_revision_id FROM brokerage_operation_heads ORDER BY operation_id"},
      ExportQuery{name:"brokerage_buy_funding_attributions",sql:"SELECT operation_revision_id,external_capital_minor,existing_cash_minor,reinvested_minor,investment_income_minor,unattributed_minor FROM brokerage_buy_funding_attributions ORDER BY operation_revision_id"},
      ExportQuery{name:"investment_snapshots",sql:"SELECT id,observed_at,provider_effective_date,source,external_reference,provenance_source,created_at FROM investment_snapshots ORDER BY observed_at,id"},
      ExportQuery{name:"investment_snapshot_positions",sql:"SELECT snapshot_id,account_id,instrument_id,quantity,currency,observed_cash_minor,observed_value_minor,valuation_currency,observed_price FROM investment_snapshot_positions ORDER BY snapshot_id,account_id,instrument_id,currency"},
      ExportQuery{name:"investment_snapshot_baselines",sql:"SELECT snapshot_id,account_id,instrument_id,currency,status,quantity_difference,cash_difference_minor,derived_historical_cost_minor,derived_value_minor,value_difference_minor FROM investment_snapshot_baselines ORDER BY snapshot_id,account_id,instrument_id,currency"},
      ExportQuery{name:"investment_adjustment_revisions",sql:"SELECT id,adjustment_id,revision,snapshot_id,account_id,instrument_id,currency,quantity_delta,cash_delta_minor,historical_cost_delta_minor,effective_date,reason,provenance_source,correction_reason,replaces_revision_id,created_at FROM investment_adjustment_revisions ORDER BY adjustment_id,revision,id"},
      ExportQuery{name:"investment_adjustment_heads",sql:"SELECT adjustment_id,current_revision_id FROM investment_adjustment_heads ORDER BY adjustment_id"},
      ExportQuery{name:"accepted_investment_document_events",sql:"SELECT id,source_document_id,import_batch_id,account_id,provider,parser_id,parser_version,event_type,provider_effective_date,currency,amount_minor,instrument_hint,quantity,external_reference,page_number,row_index,evidence_redaction,fingerprint,status,decision,reconciled_kind,reconciled_id,accepted_snapshot_id,reviewed_at,created_at FROM investment_document_events WHERE status='accepted' ORDER BY provider_effective_date,id"},
      ExportQuery{name:"investment_provenance",sql:"SELECT p.id,p.investment_document_event_id,p.investment_snapshot_id,p.source_document_id,p.import_batch_id,p.page_number,p.row_index,p.extractor_name,p.extractor_version,p.parser_id,p.parser_version,p.evidence_redaction,p.evidence_text_redacted,p.raw_storage_policy,p.confidence,p.created_at FROM provenance p LEFT JOIN investment_document_events e ON e.id=p.investment_document_event_id WHERE p.investment_snapshot_id IS NOT NULL OR e.status='accepted' ORDER BY COALESCE(p.investment_document_event_id,p.investment_snapshot_id),p.id"},
      ExportQuery{name:"manual_provenance",sql:"SELECT canonical_transaction_id,entry_id,source,created_at FROM manual_transaction_provenance ORDER BY canonical_transaction_id"}];
    if let Err(e) = export_queries(&c, &mut r, &queries) {
        r.errors.push(error(
            OperationErrorCategory::SchemaIncompatibility,
            "export_query_failed",
            e.to_string(),
            "schema",
        ));
        return r;
    }
    if include_review_audit {
        let extra=[
      ExportQuery{name:"review_candidates",sql:"SELECT id,import_batch_id,source_document_id,institution_id,institution_hint,account_id,account_label_hint,account_currency_hint,account_masked_identifier_hint,posted_date,description,amount_minor,currency,balance_minor,direction_hint,semantic_hint,confidence,status,duplicate_status,fingerprint,validation_warnings_json,canonical_transaction_id,created_at FROM candidate_transactions ORDER BY import_batch_id,id"},
      ExportQuery{name:"import_batches",sql:"SELECT id,source_document_id,started_at,completed_at,status,candidate_count,error_count,duplicate_count FROM import_batches ORDER BY started_at,id"},
      ExportQuery{name:"source_documents",sql:"SELECT id,mime_type,byte_size,institution_id,institution_hint,account_id,account_label_hint,account_currency_hint,account_masked_identifier_hint,imported_at,duplicate_of_source_document_id FROM source_documents ORDER BY imported_at,id"},
      ExportQuery{name:"review_provenance",sql:"SELECT id,candidate_transaction_id,canonical_transaction_id,source_document_id,import_batch_id,page_number,row_index,extractor_name,extractor_version,parser_id,parser_version,evidence_redaction,evidence_text_redacted,raw_storage_policy,confidence,created_at FROM provenance WHERE candidate_transaction_id IS NOT NULL ORDER BY candidate_transaction_id,id"},
      ExportQuery{name:"investment_document_review_events",sql:"SELECT id,source_document_id,import_batch_id,account_id,provider,parser_id,parser_version,event_type,provider_effective_date,currency,amount_minor,instrument_hint,quantity,external_reference,page_number,row_index,evidence_redaction,fingerprint,status,decision,reconciled_kind,reconciled_id,accepted_snapshot_id,reviewed_at,created_at FROM investment_document_events ORDER BY provider_effective_date,id"},
      ExportQuery{name:"candidate_account_assignment_events",sql:"SELECT id,candidate_transaction_id,revision,previous_account_id,account_id,decision,reviewed_at FROM candidate_account_assignment_events ORDER BY candidate_transaction_id,revision"},
      ExportQuery{name:"candidate_transfer_decisions",sql:"SELECT id,candidate_transaction_id,decision,reviewer_reason,suspicion_evidence_json,reviewed_at FROM candidate_transfer_decisions ORDER BY candidate_transaction_id,id"}];
        if let Err(e) = export_queries(&c, &mut r, &extra) {
            r.errors.push(error(
                OperationErrorCategory::SchemaIncompatibility,
                "export_query_failed",
                e.to_string(),
                "schema",
            ));
            return r;
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
