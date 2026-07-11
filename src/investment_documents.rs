use crate::investments::canonical_exact_decimal;
use crate::pdf::{hex_sha256, source_document_id, BBox, ExtractedLine};
use crate::storage::apply_migrations;
use anyhow::{anyhow, Context, Result};
use pdf_oxide::PdfDocument;
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub const SCHEMA_VERSION: &str = "tracky.investment-documents.v1";
const PARSER_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProviderEvent {
    pub id: String,
    pub provider: String,
    pub parser_id: String,
    pub parser_version: String,
    pub event_type: String,
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
    pub status: String,
    pub decision: Option<String>,
    pub reconciled_kind: Option<String>,
    pub reconciled_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    pub provider: Option<String>,
    pub source_document_id: Option<String>,
    pub import_batch_id: Option<String>,
    pub events: Vec<ProviderEvent>,
    pub errors: Vec<Error>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Error {
    pub code: &'static str,
    pub path: &'static str,
    pub message: String,
}

pub fn inspect(path: &Path, credential: Option<&str>) -> Result<Response> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let source = source_document_id(&hex_sha256(&bytes));
    let lines = extract(path, credential.unwrap_or(""))?;
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
    let bytes = fs::read(path)?;
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
    let lines = extract(path, credential.unwrap_or(""))?;
    let (provider, events) = match detect_and_parse(&source, &lines) {
        Ok(v) => v,
        Err(e) => return Ok(err("investment-documents import", e)),
    };
    let batch = unique("batch", &hash);
    let tx = connection.transaction()?;
    tx.execute("INSERT INTO source_documents(id,input_name,content_sha256,mime_type,byte_size,institution_hint) VALUES(?1,?2,?3,'application/pdf',?4,?5)", params![source,path.file_name().and_then(|x|x.to_str()).unwrap_or("document.pdf"),hash,bytes.len() as i64,provider])?;
    tx.execute("INSERT INTO import_batches(id,source_document_id,started_at,completed_at,status,candidate_count,error_count,duplicate_count,error_details_json) VALUES(?1,?2,?3,?3,'completed',?4,0,0,'[]')",params![batch,source,now(),events.len() as i64])?;
    for e in &events {
        if tx.execute("INSERT INTO investment_document_events(id,source_document_id,import_batch_id,provider,parser_id,parser_version,event_type,provider_effective_date,currency,amount_minor,instrument_hint,quantity,external_reference,page_number,row_index,evidence_redaction,fingerprint,status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,'pending_review')",params![e.id,source,batch,e.provider,e.parser_id,e.parser_version,e.event_type,e.provider_effective_date,e.currency,e.amount_minor,e.instrument_hint,e.quantity,e.external_reference,e.page_number as i64,e.row_index as i64,e.evidence_redaction,e.fingerprint]).is_err() {
            return Ok(err("investment-documents import", Error { code:"duplicate_provider_movement", path:"events.fingerprint", message:"A normalized provider movement from this document was already imported.".into() }));
        }
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

pub fn list(connection: &Connection) -> Result<Response> {
    let mut s=connection.prepare("SELECT id,provider,parser_id,parser_version,event_type,provider_effective_date,currency,amount_minor,instrument_hint,quantity,external_reference,page_number,row_index,evidence_redaction,fingerprint,status,decision,reconciled_kind,reconciled_id FROM investment_document_events ORDER BY created_at,id")?;
    let events = s
        .query_map([], row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ok("investment-documents list", None, None, None, events))
}

pub fn review(
    connection: &mut Connection,
    id: &str,
    decision: &str,
    reconciled_kind: Option<&str>,
    reconciled_id: Option<&str>,
) -> Result<Response> {
    if !matches!(decision, "reconcile_transaction" | "reject") {
        return Ok(err(
            "investment-documents review",
            Error {
                code: "invalid_decision",
                path: "decision",
                message: "decision must be reconcile_transaction or reject".into(),
            },
        ));
    }
    if decision == "reconcile_transaction"
        && (reconciled_kind != Some("canonical_transaction") || reconciled_id.is_none())
    {
        return Ok(err(
            "investment-documents review",
            Error {
                code: "incomplete_reconciliation",
                path: "reconciled_id",
                message: "reconcile_transaction requires canonical_transaction and its id".into(),
            },
        ));
    }
    if decision == "reject" && (reconciled_kind.is_some() || reconciled_id.is_some()) {
        return Ok(err(
            "investment-documents review",
            Error {
                code: "reconciliation_not_allowed",
                path: "reconciled_id",
                message: "Rejected evidence cannot consume a reconciliation target.".into(),
            },
        ));
    }
    let tx = connection.transaction()?;
    if decision == "reconcile_transaction" {
        let compatible: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM investment_document_events e JOIN canonical_transactions c ON c.id=?1 WHERE e.id=?2 AND e.status='pending_review' AND e.currency=c.currency AND e.provider_effective_date=c.posted_date AND abs(e.amount_minor)=abs(c.amount_minor) AND c.transaction_kind IN ('investment_contribution','own_account_transfer'))",
            params![reconciled_id, id], |r| r.get(0))?;
        if !compatible {
            return Ok(err("investment-documents review",Error{code:"reconciliation_mismatch",path:"reconciled_id",message:"Canonical counterpart does not match exact date, absolute amount, currency, and supported direction.".into()}));
        }
    }
    let changed=match tx.execute("UPDATE investment_document_events SET status=?1,decision=?2,reconciled_kind=?3,reconciled_id=?4,reviewed_at=?5 WHERE id=?6 AND status='pending_review'",params![if decision=="reconcile_transaction"{"accepted"}else{"rejected"},decision,reconciled_kind,reconciled_id,now(),id]) { Ok(value)=>value, Err(_)=>return Ok(err("investment-documents review",Error{code:"reconciliation_already_consumed",path:"reconciled_id",message:"The canonical counterpart was already reconciled to another provider event.".into()})) };
    if changed == 0 {
        return Ok(err(
            "investment-documents review",
            Error {
                code: "event_not_pending",
                path: "event_id",
                message: "event does not exist or was already reviewed".into(),
            },
        ));
    }
    tx.commit()?;
    list(connection).map(|mut r| {
        r.command = "investment-documents review";
        r.events.retain(|e| e.id == id);
        r
    })
}

pub fn detect_and_parse(
    source: &str,
    lines: &[ExtractedLine],
) -> std::result::Result<(String, Vec<ProviderEvent>), Error> {
    let all = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let provider = if all.contains("Comportamiento de tus criptos") && all.contains("Wenia") {
        "wenia"
    } else if all.contains("Extracto Transaccional") && all.contains("SOMOS PLENTI") {
        "plenti"
    } else if all.contains("Llegó tu extracto") && all.contains("CDT Nu") {
        "nu"
    } else {
        return Err(Error {
            code: "unsupported_document",
            path: "parser",
            message: "No supported investment-provider format matched document content.".into(),
        });
    };
    let rows = logical_rows(lines);
    let mut events = match provider {
        "nu" => parse_nu(source, lines),
        "plenti" => parse_plenti(source, &rows),
        _ => parse_wenia(source, &rows),
    };
    let mut fingerprints = std::collections::HashSet::new();
    events.retain(|event| fingerprints.insert(event.fingerprint.clone()));
    if events.is_empty() {
        return Err(Error {
            code: "partially_recognized_document",
            path: "events",
            message: "Provider matched but no sufficiently supported rows were found.".into(),
        });
    }
    Ok((provider.into(), events))
}

fn logical_rows(lines: &[ExtractedLine]) -> Vec<ExtractedLine> {
    let mut cells = lines.to_vec();
    cells.sort_by(|a, b| {
        let ay = a.bbox.map(|x| x.y).unwrap_or(0.0);
        let by = b.bbox.map(|x| x.y).unwrap_or(0.0);
        a.page
            .cmp(&b.page)
            .then_with(|| ay.total_cmp(&by))
            .then_with(|| {
                a.bbox
                    .map(|x| x.x)
                    .unwrap_or(0.0)
                    .total_cmp(&b.bbox.map(|x| x.x).unwrap_or(0.0))
            })
    });
    let mut rows: Vec<ExtractedLine> = Vec::new();
    for cell in cells {
        let y = cell.bbox.map(|x| x.y).unwrap_or(0.0);
        if let Some(row) = rows.last_mut() {
            let row_y = row.bbox.map(|x| x.y).unwrap_or(0.0);
            if row.bbox.is_some()
                && cell.bbox.is_some()
                && row.page == cell.page
                && (row_y - y).abs() <= 3.0
            {
                row.text.push(' ');
                row.text.push_str(&cell.text);
                continue;
            }
        }
        rows.push(cell);
    }
    rows
}

fn parse_nu(source: &str, lines: &[ExtractedLine]) -> Vec<ProviderEvent> {
    let re=Regex::new(r"(?i)(\d{2})\s+(ene|feb|mar|abr|may|jun|jul|ago|sep|oct|nov|dic)\s+(Abriste\s+un\s+CDT|Recibiste\s+dinero\s+de\s+un\s+CDT|Enviaste\s+a\s+Plenti)\s+([+-]?\$\s*[\d.,]+)").unwrap();
    let Some(year) = year_from(lines) else {
        return vec![];
    };
    let text = lines
        .iter()
        .filter(|x| x.bbox.is_none())
        .map(|x| x.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    re.captures_iter(&text)
        .enumerate()
        .filter_map(|(index, c)| {
            let kind = c[3].to_lowercase();
            let typ = if kind.starts_with("abriste") {
                "cdt_opening"
            } else if kind.starts_with("recibiste") {
                "cdt_return"
            } else {
                "withdrawal"
            };
            let date = format!("{year}-{}-{}", month(&c[2]), &c[1]);
            let amount = money(&c[4].replace(' ', ""))?;
            let fp = digest(&format!("nu|{typ}|{date}|COP|{amount}"));
            Some(ProviderEvent {
                id: format!("invevt_{}", &digest(&format!("{source}|{fp}"))[..24]),
                provider: "nu".into(),
                parser_id: "nu.investment-document.v1".into(),
                parser_version: PARSER_VERSION.into(),
                event_type: typ.into(),
                provider_effective_date: date,
                currency: "COP".into(),
                amount_minor: Some(amount),
                instrument_hint: None,
                quantity: None,
                external_reference: None,
                page_number: 1,
                row_index: index + 1,
                evidence_redaction: redact(c.get(0)?.as_str()),
                fingerprint: fp,
                status: "pending_review".into(),
                decision: None,
                reconciled_kind: None,
                reconciled_id: None,
            })
        })
        .collect()
}
fn parse_plenti(source: &str, lines: &[ExtractedLine]) -> Vec<ProviderEvent> {
    let re = Regex::new(
        r"^(\d{2}/\d{2}/\d{4})\s+(Recarga Bre-B|Depósito amigo Plenti).*?([\d,.]+)\s+([\d,.]+)$",
    )
    .unwrap();
    parse_rows(source, "plenti", lines, |text| {
        let c = re.captures(text)?;
        Some((
            "deposit",
            date_slash(&c[1])?,
            "COP",
            money(&c[4])?,
            None,
            None,
        ))
    })
}
fn parse_wenia(source: &str, lines: &[ExtractedLine]) -> Vec<ProviderEvent> {
    let date = Regex::new(r"Periodo del informe: del \d{2}/\d{2}/\d{4} al (\d{2}/\d{2}/\d{4})")
        .unwrap()
        .captures(
            &lines
                .iter()
                .map(|x| x.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        )
        .and_then(|c| date_slash(&c[1]))
        .unwrap_or_default();
    let re = Regex::new(r"^(COPW|USDW).*?([\d.,]+)\s+(COPW|USDW).*?([\d.,]+)\s+USD").unwrap();
    parse_rows(source, "wenia", lines, |text| {
        let c = re.captures(text)?;
        Some((
            "observed_position",
            date.clone(),
            "USD",
            money(&c[4])?,
            Some(c[1].to_string()),
            Some(decimal(&c[2])?),
        ))
    })
}
fn parse_rows<F>(
    source: &str,
    provider: &str,
    lines: &[ExtractedLine],
    mut f: F,
) -> Vec<ProviderEvent>
where
    F: FnMut(
        &str,
    ) -> Option<(
        &'static str,
        String,
        &'static str,
        i64,
        Option<String>,
        Option<String>,
    )>,
{
    let mut out = vec![];
    for (i, l) in lines.iter().enumerate() {
        if let Some((typ, date, currency, amount, instrument, quantity)) = f(&l.text) {
            let evidence = redact(&l.text);
            let fp = digest(&format!(
                "{provider}|{typ}|{date}|{currency}|{amount}|{instrument:?}|{quantity:?}"
            ));
            out.push(ProviderEvent {
                id: format!("invevt_{}", &digest(&format!("{source}|{fp}"))[..24]),
                provider: provider.into(),
                parser_id: format!("{provider}.investment-document.v1"),
                parser_version: PARSER_VERSION.into(),
                event_type: typ.into(),
                provider_effective_date: date,
                currency: currency.into(),
                amount_minor: Some(amount),
                instrument_hint: instrument,
                quantity,
                external_reference: None,
                page_number: l.page,
                row_index: i + 1,
                evidence_redaction: evidence,
                fingerprint: fp,
                status: "pending_review".into(),
                decision: None,
                reconciled_kind: None,
                reconciled_id: None,
            });
        }
    }
    out
}
fn extract(path: &Path, password: &str) -> Result<Vec<ExtractedLine>> {
    let doc = PdfDocument::open(path)?;
    if doc.is_encrypted() && !doc.authenticate(password.as_bytes())? {
        return Err(anyhow!("PDF credential rejected"));
    }
    let mut out = vec![];
    for p in 0..doc.page_count()? {
        let page_text = doc.extract_text(p)?;
        out.extend(
            page_text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| ExtractedLine {
                    page: p + 1,
                    text: line.split_whitespace().collect::<Vec<_>>().join(" "),
                    bbox: None,
                }),
        );
        for l in doc.extract_text_lines(p)? {
            out.push(ExtractedLine {
                page: p + 1,
                text: l.text.split_whitespace().collect::<Vec<_>>().join(" "),
                bbox: Some(BBox {
                    x: l.bbox.x,
                    y: l.bbox.y,
                    width: l.bbox.width,
                    height: l.bbox.height,
                    unit: "pdf_point",
                }),
            });
        }
    }
    Ok(out)
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderEvent> {
    Ok(ProviderEvent {
        id: r.get(0)?,
        provider: r.get(1)?,
        parser_id: r.get(2)?,
        parser_version: r.get(3)?,
        event_type: r.get(4)?,
        provider_effective_date: r.get(5)?,
        currency: r.get(6)?,
        amount_minor: r.get(7)?,
        instrument_hint: r.get(8)?,
        quantity: r.get(9)?,
        external_reference: r.get(10)?,
        page_number: r.get::<_, i64>(11)? as usize,
        row_index: r.get::<_, i64>(12)? as usize,
        evidence_redaction: r.get(13)?,
        fingerprint: r.get(14)?,
        status: r.get(15)?,
        decision: r.get(16)?,
        reconciled_kind: r.get(17)?,
        reconciled_id: r.get(18)?,
    })
}
fn money(s: &str) -> Option<i64> {
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
fn decimal(s: &str) -> Option<String> {
    let x = if s.contains(',') {
        s.replace('.', "").replace(',', ".")
    } else {
        s.to_string()
    };
    canonical_exact_decimal(&x, false)
}
fn date_slash(s: &str) -> Option<String> {
    let x = s.split('/').collect::<Vec<_>>();
    Some(format!("{}-{}-{}", x.get(2)?, x.get(1)?, x.first()?))
}
fn month(s: &str) -> &'static str {
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
fn year_from(lines: &[ExtractedLine]) -> Option<String> {
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
fn redact(s: &str) -> String {
    Regex::new(r"\d").unwrap().replace_all(s, "#").into_owned()
}
fn digest(s: &str) -> String {
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
fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
fn ok(
    command: &'static str,
    provider: Option<String>,
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
        errors: vec![],
    }
}
fn err(command: &'static str, e: Error) -> Response {
    Response {
        schema_version: SCHEMA_VERSION,
        command,
        ok: false,
        provider: None,
        source_document_id: None,
        import_batch_id: None,
        events: vec![],
        errors: vec![e],
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
        assert_eq!(provider, "nu");
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
