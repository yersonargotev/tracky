use crate::investment_document_parsers::{detect_and_parse, extract};
use crate::investments::canonical_exact_decimal;
use crate::pdf::{hex_sha256, source_document_id, ExtractedLine};
use crate::storage::apply_migrations;
use anyhow::Result;
use regex::Regex;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub const SCHEMA_VERSION: &str = "tracky.investment-documents.v1";
pub(crate) const PARSER_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProviderEvent {
    pub id: String,
    pub provider: Provider,
    pub parser_id: String,
    pub parser_version: String,
    pub event_type: EventType,
    pub provider_effective_date: String,
    pub currency: String,
    pub amount_minor: Option<i64>,
    pub instrument_hint: Option<String>,
    pub quantity: Option<String>,
    pub external_reference: Option<String>,
    pub page_number: usize,
    pub row_index: usize,
    pub evidence_redaction: String,
    pub fingerprint: String,
    pub status: ReviewStatus,
    pub decision: Option<ReviewDecision>,
    pub reconciled_kind: Option<ReconciliationKind>,
    pub reconciled_id: Option<String>,
    pub source_document_id: String,
    pub import_batch_id: Option<String>,
    pub account_id: Option<String>,
    pub provenance_id: Option<String>,
    pub accepted_snapshot_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReconciliationCandidate {
    pub kind: ReconciliationKind,
    pub target_kind: Option<ReconciliationKind>,
    pub target_id: Option<String>,
    pub status: MatchStatus,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuditChain {
    pub source_document_id: String,
    pub import_batch_id: String,
    pub provenance_id: String,
    pub event_account_id: Option<String>,
    pub parser_id: String,
    pub parser_version: String,
    pub page_number: usize,
    pub row_index: usize,
    pub evidence_redaction: String,
    pub decision: Option<ReviewDecision>,
    pub reconciled_target: Option<AuditTarget>,
    pub accepted_snapshot: Option<AcceptedSnapshotAudit>,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuditTarget {
    pub kind: ReconciliationKind,
    pub id: String,
    pub account_id: Option<String>,
    pub effective_date: String,
    pub amount_minor: Option<i64>,
    pub currency: String,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AcceptedSnapshotAudit {
    pub id: String,
    pub provider_effective_date: Option<String>,
    pub position_count: usize,
    pub baseline_count: usize,
    pub positions: Vec<AcceptedSnapshotPositionAudit>,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AcceptedSnapshotPositionAudit {
    pub account_id: String,
    pub instrument_id: Option<String>,
    pub quantity: Option<String>,
    pub currency: String,
    pub observed_value_minor: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    pub provider: Option<Provider>,
    pub source_document_id: Option<String>,
    pub import_batch_id: Option<String>,
    pub events: Vec<ProviderEvent>,
    pub candidates: Vec<ReconciliationCandidate>,
    pub audit_chain: Option<AuditChain>,
    pub errors: Vec<Error>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Error {
    pub code: &'static str,
    pub path: &'static str,
    pub message: String,
}

pub fn inspect(path: &Path, credential: Option<&str>) -> Result<Response> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(err("investment-documents inspect", extraction_error())),
    };
    let source = source_document_id(&hex_sha256(&bytes));
    let lines = match extract(path, credential.unwrap_or("")) {
        Ok(lines) => lines,
        Err(_) => return Ok(err("investment-documents inspect", extraction_error())),
    };
    Ok(match detect_and_parse(&source, &lines) {
        Ok((provider, events)) => ok(
            "investment-documents inspect",
            Some(provider),
            Some(source),
            None,
            events,
        ),
        Err(e) => err("investment-documents inspect", e),
    })
}

pub fn import(
    connection: &mut Connection,
    path: &Path,
    credential: Option<&str>,
) -> Result<Response> {
    apply_migrations(connection)?;
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(err("investment-documents import", extraction_error())),
    };
    let hash = hex_sha256(&bytes);
    let source = source_document_id(&hash);
    if connection
        .query_row(
            "SELECT id FROM source_documents WHERE content_sha256=?1",
            [&hash],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .is_some()
    {
        return Ok(err(
            "investment-documents import",
            Error {
                code: "duplicate_source_document",
                path: "source_document.content_sha256",
                message: "The exact document was already imported.".into(),
            },
        ));
    }
    let lines = match extract(path, credential.unwrap_or("")) {
        Ok(lines) => lines,
        Err(_) => return Ok(err("investment-documents import", extraction_error())),
    };
    let (provider, mut events) = match detect_and_parse(&source, &lines) {
        Ok(v) => v,
        Err(e) => return Ok(err("investment-documents import", e)),
    };
    let batch = unique("batch", &hash);
    let tx = connection.transaction()?;
    tx.execute("INSERT INTO source_documents(id,input_name,content_sha256,mime_type,byte_size,institution_hint) VALUES(?1,?2,?3,'application/pdf',?4,?5)", params![source,path.file_name().and_then(|x|x.to_str()).unwrap_or("document.pdf"),hash,bytes.len() as i64,provider])?;
    tx.execute("INSERT INTO import_batches(id,source_document_id,started_at,completed_at,status,candidate_count,error_count,duplicate_count,error_details_json) VALUES(?1,?2,?3,?3,'completed',?4,0,0,'[]')",params![batch,source,now(),events.len() as i64])?;
    for e in &mut events {
        e.import_batch_id = Some(batch.clone());
        if tx.execute("INSERT INTO investment_document_events(id,source_document_id,import_batch_id,provider,parser_id,parser_version,event_type,provider_effective_date,currency,amount_minor,instrument_hint,quantity,external_reference,page_number,row_index,evidence_redaction,fingerprint,status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,'pending_review')",params![e.id,source,batch,e.provider,e.parser_id,e.parser_version,e.event_type,e.provider_effective_date,e.currency,e.amount_minor,e.instrument_hint,e.quantity,e.external_reference,e.page_number as i64,e.row_index as i64,e.evidence_redaction,e.fingerprint]).is_err() {
            return Ok(err("investment-documents import", Error { code:"duplicate_provider_movement", path:"events.fingerprint", message:"A normalized provider movement from this document was already imported.".into() }));
        }
        let provenance_id = format!("prov_{}", &digest(&e.id)[..24]);
        tx.execute("INSERT INTO provenance(id,investment_document_event_id,source_document_id,import_batch_id,page_number,row_index,extractor_name,extractor_version,parser_id,parser_version,evidence_redaction,evidence_text_redacted,raw_storage_policy,confidence) VALUES(?1,?2,?3,?4,?5,?6,'pdf_oxide',NULL,?7,?8,?9,?9,'redacted_only',1.0)",params![provenance_id,e.id,source,batch,e.page_number as i64,e.row_index as i64,e.parser_id,e.parser_version,e.evidence_redaction])?;
        e.provenance_id = Some(provenance_id);
    }
    tx.commit()?;
    Ok(ok(
        "investment-documents import",
        Some(provider),
        Some(source),
        Some(batch),
        events,
    ))
}

pub use crate::investment_document_review::{
    accept_snapshot, inspect_event, list, reconcile_deposit, reconcile_withdrawal,
    reconciliation_candidates, reject,
};

pub(crate) fn money(s: &str) -> Option<i64> {
    let raw = s.trim();
    let negative = raw.starts_with('-');
    let raw = raw.trim_start_matches(['+', '-']).trim_start_matches('$');
    let (whole, fraction) = match (raw.rfind('.'), raw.rfind(',')) {
        (Some(dot), Some(comma)) if dot > comma => (&raw[..dot], &raw[dot + 1..]),
        (Some(_dot), Some(comma)) => (&raw[..comma], &raw[comma + 1..]),
        (Some(pos), None) if raw.len() - pos - 1 == 2 => (&raw[..pos], &raw[pos + 1..]),
        (None, Some(pos)) if raw.len() - pos - 1 == 2 => (&raw[..pos], &raw[pos + 1..]),
        _ => (raw, "00"),
    };
    let units: i64 = whole.replace(['.', ','], "").parse().ok()?;
    let cents: i64 = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().ok()?.checked_mul(10)?,
        2 => fraction.parse().ok()?,
        _ => return None,
    };
    let value = units.checked_mul(100)?.checked_add(cents)?;
    Some(if negative { -value } else { value })
}
pub(crate) fn decimal(s: &str) -> Option<String> {
    let x = if s.contains(',') {
        s.replace('.', "").replace(',', ".")
    } else {
        s.to_string()
    };
    canonical_exact_decimal(&x, false)
}
pub(crate) fn date_slash(s: &str) -> Option<String> {
    let x = s.split('/').collect::<Vec<_>>();
    Some(format!("{}-{}-{}", x.get(2)?, x.get(1)?, x.first()?))
}
pub(crate) fn month(s: &str) -> &'static str {
    match s.to_lowercase().as_str() {
        "ene" => "01",
        "feb" => "02",
        "mar" => "03",
        "abr" => "04",
        "may" => "05",
        "jun" => "06",
        "jul" => "07",
        "ago" => "08",
        "sep" => "09",
        "oct" => "10",
        "nov" => "11",
        _ => "12",
    }
}
pub(crate) fn year_from(lines: &[ExtractedLine]) -> Option<String> {
    Regex::new(r"20\d{2}")
        .ok()?
        .find(
            &lines
                .iter()
                .map(|x| x.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        )
        .map(|x| x.as_str().into())
}
pub(crate) fn redact(s: &str) -> String {
    Regex::new(r"\d").unwrap().replace_all(s, "#").into_owned()
}
pub(crate) fn digest(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
fn unique(prefix: &str, seed: &str) -> String {
    format!(
        "{prefix}_{}_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        &digest(seed)[..8]
    )
}
pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
pub(crate) fn ok(
    command: &'static str,
    provider: Option<Provider>,
    source_document_id: Option<String>,
    import_batch_id: Option<String>,
    events: Vec<ProviderEvent>,
) -> Response {
    Response {
        schema_version: SCHEMA_VERSION,
        command,
        ok: true,
        provider,
        source_document_id,
        import_batch_id,
        events,
        candidates: vec![],
        audit_chain: None,
        errors: vec![],
    }
}
pub(crate) fn err(command: &'static str, e: Error) -> Response {
    Response {
        schema_version: SCHEMA_VERSION,
        command,
        ok: false,
        provider: None,
        source_document_id: None,
        import_batch_id: None,
        events: vec![],
        candidates: vec![],
        audit_chain: None,
        errors: vec![e],
    }
}

fn extraction_error() -> Error {
    Error {
        code: "pdf_extraction_failed",
        path: "document",
        message: "The document could not be extracted with the supplied runtime credential.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(page: usize, text: &str) -> ExtractedLine {
        ExtractedLine {
            page,
            text: text.into(),
            bbox: None,
        }
    }

    #[test]
    fn nu_content_proposes_only_supported_cdt_and_plenti_movements() {
        let lines = vec![
            line(1, "Llegó tu extracto de Junio 2026 CDT Nu"),
            line(2, "16 jun Enviaste a Plenti -$2.000.000,00"),
            line(2, "19 jun Recibiste dinero de un CDT +$1.050.000,00"),
            line(2, "24 jun Abriste un CDT -$900.000,00"),
        ];
        let (provider, events) = detect_and_parse("src", &lines).unwrap();
        assert_eq!(provider, Provider::Nu);
        assert_eq!(
            events
                .iter()
                .map(|x| x.event_type.as_str())
                .collect::<Vec<_>>(),
            ["withdrawal", "cdt_return", "cdt_opening"]
        );
        assert_eq!(events[0].amount_minor, Some(-200_000_000));
        assert!(events[0]
            .evidence_redaction
            .chars()
            .all(|c| !c.is_ascii_digit()));
    }

    #[test]
    fn plenti_content_proposes_exact_deposit_without_inventing_instrument() {
        let lines = vec![
            line(1, "Extracto Transaccional SOMOS PLENTI S.A.S."),
            line(
                1,
                "16/06/2026 Recarga Bre-B PERSONA REDACTADA 580.21 2,000,000.00",
            ),
        ];
        let (_, events) = detect_and_parse("src", &lines).unwrap();
        assert_eq!(events[0].amount_minor, Some(200_000_000));
        assert_eq!(events[0].instrument_hint, None);
    }

    #[test]
    fn plenti_duplicate_visual_and_text_rows_are_normalized_once() {
        let lines = vec![
            line(1, "Extracto Transaccional SOMOS PLENTI S.A.S."),
            line(1, "16/06/2026 Recarga Bre-B REDACTADA 1.00 2,000.00"),
            line(1, "16/06/2026 Recarga Bre-B REDACTADA 1.00 2,000.00"),
        ];
        let (_, events) = detect_and_parse("src", &lines).unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn wenia_portfolio_position_keeps_exact_quantity_and_dated_value() {
        let lines = vec![
            line(1, "Comportamiento de tus criptos Wenia"),
            line(1, "Periodo del informe: del 01/06/2026 al 30/06/2026"),
            line(1, "COPW activo 10,2500 COPW valor 123.45 USD"),
        ];
        let (_, events) = detect_and_parse("src", &lines).unwrap();
        assert_eq!(events[0].event_type, EventType::ObservedPosition);
        assert_eq!(events[0].provider_effective_date, "2026-06-30");
        assert_eq!(events[0].quantity.as_deref(), Some("10.25"));
        assert_eq!(events[0].amount_minor, Some(12_345));
    }

    #[test]
    fn recognized_provider_without_complete_rows_stays_safe() {
        let error =
            detect_and_parse("src", &[line(1, "Comportamiento de tus criptos Wenia")]).unwrap_err();
        assert_eq!(error.code, "partially_recognized_document");
    }

    #[test]
    fn unsupported_content_fails_without_candidates() {
        let error = detect_and_parse("src", &[line(1, "generic bank description")]).unwrap_err();
        assert_eq!(error.code, "unsupported_document");
    }

    #[test]
    fn exact_money_parser_never_rounds_binary_floats() {
        assert_eq!(money("$1.234,56"), Some(123_456));
        assert_eq!(money("2,000,000.00"), Some(200_000_000));
        assert_eq!(money("-$0,01"), Some(-1));
    }
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Nu,
    Wenia,
    Plenti,
}
impl Provider {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Nu => "nu",
            Self::Wenia => "wenia",
            Self::Plenti => "plenti",
        }
    }
}
impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
impl Provider {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "nu" => Some(Self::Nu),
            "wenia" => Some(Self::Wenia),
            "plenti" => Some(Self::Plenti),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Deposit,
    Withdrawal,
    CdtOpening,
    CdtReturn,
    ObservedCash,
    ObservedPosition,
}
impl EventType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Deposit => "deposit",
            Self::Withdrawal => "withdrawal",
            Self::CdtOpening => "cdt_opening",
            Self::CdtReturn => "cdt_return",
            Self::ObservedCash => "observed_cash",
            Self::ObservedPosition => "observed_position",
        }
    }
}
impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
impl EventType {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "deposit" => Some(Self::Deposit),
            "withdrawal" => Some(Self::Withdrawal),
            "cdt_opening" => Some(Self::CdtOpening),
            "cdt_return" => Some(Self::CdtReturn),
            "observed_cash" => Some(Self::ObservedCash),
            "observed_position" => Some(Self::ObservedPosition),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    PendingReview,
    Accepted,
    Rejected,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    ReconcileDeposit,
    ReconcileWithdrawal,
    AcceptSnapshot,
    Reject,
}
impl ReviewDecision {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ReconcileDeposit => "reconcile_deposit",
            Self::ReconcileWithdrawal => "reconcile_withdrawal",
            Self::AcceptSnapshot => "accept_snapshot",
            Self::Reject => "reject",
        }
    }
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "reconcile_deposit" => Some(Self::ReconcileDeposit),
            "reconcile_withdrawal" => Some(Self::ReconcileWithdrawal),
            "accept_snapshot" => Some(Self::AcceptSnapshot),
            "reject" => Some(Self::Reject),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationKind {
    Reconciliation,
    CanonicalTransaction,
    ProviderEvent,
    InvestmentSnapshot,
}
impl ReconciliationKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Reconciliation => "reconciliation",
            Self::CanonicalTransaction => "canonical_transaction",
            Self::ProviderEvent => "provider_event",
            Self::InvestmentSnapshot => "investment_snapshot",
        }
    }
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "reconciliation" => Some(Self::Reconciliation),
            "canonical_transaction" => Some(Self::CanonicalTransaction),
            "provider_event" => Some(Self::ProviderEvent),
            "investment_snapshot" => Some(Self::InvestmentSnapshot),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchStatus {
    UniqueMatch,
    AmbiguousMatch,
    Unmatched,
    AlreadyReconciled,
    Incompatible,
}
impl ReviewStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PendingReview => "pending_review",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }
}
impl ReviewStatus {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "pending_review" => Some(Self::PendingReview),
            "accepted" => Some(Self::Accepted),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}
macro_rules! sqlite_enum {
    ($t:ty) => {
        impl ToSql for $t {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(self.as_str().into())
            }
        }
        impl FromSql for $t {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                let raw = value.as_str()?;
                Self::parse(raw).ok_or_else(|| {
                    FromSqlError::Other(
                        format!("invalid persisted provider-document value: {raw}").into(),
                    )
                })
            }
        }
    };
}
sqlite_enum!(Provider);
sqlite_enum!(EventType);
sqlite_enum!(ReviewStatus);
sqlite_enum!(ReviewDecision);
sqlite_enum!(ReconciliationKind);
