use crate::investment_document_parsers::{detect_and_parse_with_ordinary, extract};
use crate::investments::canonical_exact_decimal;
use crate::pdf::{hex_sha256, source_document_id, CandidateTransaction, ExtractedLine};
use crate::storage::{apply_migrations, persist_mixed_review_first};
use anyhow::Result;
use regex::Regex;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub const SCHEMA_VERSION: &str = "tracky.investment-documents.v1";
pub const MIXED_NU_SCHEMA_VERSION: &str = "tracky.investment-documents.v2";
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ordinary_candidates: Vec<CandidateTransaction>,
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
    Ok(match detect_and_parse_with_ordinary(&source, &lines) {
        Ok((provider, events, ordinary_candidates)) => response_with_ordinary(
            "investment-documents inspect",
            Some(provider),
            Some(source),
            None,
            events,
            ordinary_candidates,
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
    let (provider, mut events, ordinary_candidates) =
        match detect_and_parse_with_ordinary(&source, &lines) {
            Ok(v) => v,
            Err(e) => return Ok(err("investment-documents import", e)),
        };
    let (account_label, account_currency) = match provider {
        Provider::Nu => ("Nu account", "COP"),
        Provider::Plenti => ("Plenti account", "COP"),
        Provider::Wenia => ("Wenia account", "USD"),
    };
    let source_document = crate::pdf::SourceDocument {
        id: source.clone(),
        input_name: path
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("document.pdf")
            .into(),
        content_sha256: hash.clone(),
        mime_type: "application/pdf",
        byte_size: bytes.len() as u64,
        institution_hint: provider.to_string(),
        account_hint: crate::pdf::AccountHint {
            label: account_label.into(),
            currency: account_currency,
            masked_identifier: None,
        },
        document_duplicate_status: crate::pdf::DocumentDuplicateStatus {
            status: crate::pdf::DocumentDuplicateState::New,
            matched_source_document_id: None,
            reason: None,
        },
    };
    let (batch, ordinary_candidates) = persist_mixed_review_first(
        connection,
        source_document,
        ordinary_candidates,
        |tx, batch| {
            for event in &mut events {
                event.import_batch_id = Some(batch.to_string());
                tx.execute("INSERT INTO investment_document_events(id,source_document_id,import_batch_id,provider,parser_id,parser_version,event_type,provider_effective_date,currency,amount_minor,instrument_hint,quantity,external_reference,page_number,row_index,evidence_redaction,fingerprint,status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,'pending_review')",params![event.id,source,batch,event.provider,event.parser_id,event.parser_version,event.event_type,event.provider_effective_date,event.currency,event.amount_minor,event.instrument_hint,event.quantity,event.external_reference,event.page_number as i64,event.row_index as i64,event.evidence_redaction,event.fingerprint])?;
                let provenance_id = format!("prov_{}", &digest(&event.id)[..24]);
                tx.execute("INSERT INTO provenance(id,investment_document_event_id,source_document_id,import_batch_id,page_number,row_index,extractor_name,extractor_version,parser_id,parser_version,evidence_redaction,evidence_text_redacted,raw_storage_policy,confidence) VALUES(?1,?2,?3,?4,?5,?6,'pdf_oxide',NULL,?7,?8,?9,?9,'redacted_only',1.0)",params![provenance_id,event.id,source,batch,event.page_number as i64,event.row_index as i64,event.parser_id,event.parser_version,event.evidence_redaction])?;
                event.provenance_id = Some(provenance_id);
            }
            Ok(())
        },
    )?;
    Ok(response_with_ordinary(
        "investment-documents import",
        Some(provider),
        Some(source),
        Some(batch),
        events,
        ordinary_candidates,
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
        ordinary_candidates: vec![],
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
        ordinary_candidates: vec![],
        candidates: vec![],
        audit_chain: None,
        errors: vec![e],
    }
}

fn response_with_ordinary(
    command: &'static str,
    provider: Option<Provider>,
    source_document_id: Option<String>,
    import_batch_id: Option<String>,
    events: Vec<ProviderEvent>,
    ordinary_candidates: Vec<CandidateTransaction>,
) -> Response {
    Response {
        schema_version: if provider == Some(Provider::Nu) {
            MIXED_NU_SCHEMA_VERSION
        } else {
            SCHEMA_VERSION
        },
        command,
        ok: true,
        provider,
        source_document_id,
        import_batch_id,
        events,
        ordinary_candidates,
        candidates: vec![],
        audit_chain: None,
        errors: vec![],
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
    use crate::investment_document_parsers::detect_and_parse;
    use crate::pdf::BBox;

    fn line(page: usize, text: &str) -> ExtractedLine {
        ExtractedLine {
            page,
            text: text.into(),
            bbox: None,
        }
    }

    fn cell(page: usize, text: &str, x: f32, y: f32) -> ExtractedLine {
        ExtractedLine {
            page,
            text: text.into(),
            bbox: Some(BBox {
                x,
                y,
                width: 40.0,
                height: 10.0,
                unit: "pdf_point",
            }),
        }
    }

    #[test]
    fn nu_amount_cell_fragment_is_parsed_from_its_visual_row() {
        let lines = vec![
            line(1, "Llegó tu extracto de Abril 2026 CDT Nu"),
            cell(2, "07 abr Abriste un CDT", 40.0, 300.0),
            cell(2, "-$1.234.567,89", 360.0, 301.0),
        ];

        let (_, events) = detect_and_parse("src", &lines).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::CdtOpening);
        assert_eq!(events[0].provider_effective_date, "2026-04-07");
        assert_eq!(events[0].amount_minor, Some(-123_456_789));
        assert_eq!(events[0].page_number, 2);
        assert!(events[0]
            .evidence_redaction
            .chars()
            .all(|c| !c.is_ascii_digit()));
    }

    #[test]
    fn nu_date_description_and_amount_cells_form_one_stable_event() {
        let lines = vec![
            line(1, "Llegó tu extracto de Mayo 2026 CDT Nu"),
            cell(3, "19 may", 40.0, 420.0),
            cell(3, "Recibiste dinero de un CDT", 120.0, 420.0),
            cell(3, "+$1.050.000,00", 370.0, 420.0),
        ];

        let (_, first) = detect_and_parse("src", &lines).unwrap();
        let (_, second) = detect_and_parse("src", &lines).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].event_type, EventType::CdtReturn);
        assert_eq!(first[0].provider_effective_date, "2026-05-19");
        assert_eq!(first[0].amount_minor, Some(105_000_000));
        assert_eq!(first[0].page_number, 3);
    }

    #[test]
    fn nu_linear_and_layout_copies_of_a_movement_are_deduplicated() {
        let lines = vec![
            line(1, "Llegó tu extracto de Junio 2026 CDT Nu"),
            line(2, "24 jun Abriste un CDT -$900.000,00"),
            cell(2, "24 jun Abriste un CDT", 40.0, 300.0),
            cell(2, "-$900.000,00", 360.0, 300.0),
        ];

        let (_, events) = detect_and_parse("src", &lines).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].amount_minor, Some(-90_000_000));
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
    fn nu_mixed_layout_emits_ordinary_rows_without_duplicating_provider_rows() {
        let lines = vec![
            line(1, "Llegó tu extracto de Mayo 2026 CDT Nu"),
            cell(2, "17 may", 40.0, 300.0),
            cell(2, "Recibiste transferencia", 120.0, 300.0),
            cell(2, "+$3.500,00", 360.0, 301.0),
            cell(2, "18 may Abriste un CDT", 40.0, 320.0),
            cell(2, "-$2.000,00", 360.0, 320.0),
        ];
        let (_, events, candidates) = detect_and_parse_with_ordinary("src", &lines).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].amount_minor, 350_000);
        assert!(candidates[0].provenance.bbox.is_some());
        assert_eq!(
            candidates[0].provenance.evidence.text,
            "## may <description> <amount>"
        );
        assert!(!candidates[0]
            .provenance
            .evidence
            .text
            .contains("transferencia"));
    }

    #[test]
    fn nu_linear_and_layout_ordinary_copies_prefer_layout_provenance() {
        let lines = vec![
            line(1, "Llegó tu extracto de Junio 2026 CDT Nu"),
            line(2, "17 jun Recibiste transferencia +$3.500,00"),
            cell(2, "17 jun Recibiste transferencia", 40.0, 300.0),
            cell(2, "+$3.500,00", 360.0, 300.0),
            line(2, "18 jun Abriste un CDT -$2.000,00"),
        ];
        let (_, _, candidates) = detect_and_parse_with_ordinary("src", &lines).unwrap();
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].provenance.bbox.is_some());
    }

    #[test]
    fn nu_card_payment_remains_transfer_like_for_review() {
        let lines = vec![
            line(1, "Llegó tu extracto de Mayo 2026 CDT Nu"),
            line(2, "17 may Pagaste tu tarjeta Nu -$3.500,00"),
            line(2, "18 may Abriste un CDT -$2.000,00"),
        ];
        let (_, _, candidates) = detect_and_parse_with_ordinary("src", &lines).unwrap();
        assert_eq!(
            candidates[0].semantic_hint,
            crate::pdf::SemanticHint::CardPayment
        );
        assert_eq!(
            candidates[0].direction_hint,
            crate::pdf::DirectionHint::Outflow
        );
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
    EnrichCdtConstitution,
    EnrichCdtRenewal,
    EnrichCdtRedemption,
    Reject,
}
impl ReviewDecision {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ReconcileDeposit => "reconcile_deposit",
            Self::ReconcileWithdrawal => "reconcile_withdrawal",
            Self::AcceptSnapshot => "accept_snapshot",
            Self::EnrichCdtConstitution => "enrich_cdt_constitution",
            Self::EnrichCdtRenewal => "enrich_cdt_renewal",
            Self::EnrichCdtRedemption => "enrich_cdt_redemption",
            Self::Reject => "reject",
        }
    }
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "reconcile_deposit" => Some(Self::ReconcileDeposit),
            "reconcile_withdrawal" => Some(Self::ReconcileWithdrawal),
            "accept_snapshot" => Some(Self::AcceptSnapshot),
            "enrich_cdt_constitution" => Some(Self::EnrichCdtConstitution),
            "enrich_cdt_renewal" => Some(Self::EnrichCdtRenewal),
            "enrich_cdt_redemption" => Some(Self::EnrichCdtRedemption),
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
    CdtOperation,
}
impl ReconciliationKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Reconciliation => "reconciliation",
            Self::CanonicalTransaction => "canonical_transaction",
            Self::ProviderEvent => "provider_event",
            Self::InvestmentSnapshot => "investment_snapshot",
            Self::CdtOperation => "cdt_operation",
        }
    }
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "reconciliation" => Some(Self::Reconciliation),
            "canonical_transaction" => Some(Self::CanonicalTransaction),
            "provider_event" => Some(Self::ProviderEvent),
            "investment_snapshot" => Some(Self::InvestmentSnapshot),
            "cdt_operation" => Some(Self::CdtOperation),
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
