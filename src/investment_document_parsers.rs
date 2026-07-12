use crate::investment_documents::{
    date_slash, decimal, digest, money, month, redact, year_from, Error, EventType, Provider,
    ProviderEvent, ReviewStatus, PARSER_VERSION,
};
use crate::pdf::{BBox, ExtractedLine};
use anyhow::{anyhow, Result};
use pdf_oxide::PdfDocument;
use regex::Regex;
use std::path::Path;

pub(crate) fn detect_and_parse(
    source: &str,
    lines: &[ExtractedLine],
) -> std::result::Result<(Provider, Vec<ProviderEvent>), Error> {
    let all = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let provider = if all.contains("Comportamiento de tus criptos") && all.contains("Wenia") {
        Provider::Wenia
    } else if all.contains("Extracto Transaccional") && all.contains("SOMOS PLENTI") {
        Provider::Plenti
    } else if (all.contains("Llegó tu extracto") || all.contains("Llego tu extracto"))
        && all.contains("CDT Nu")
    {
        Provider::Nu
    } else {
        return Err(Error {
            code: "unsupported_document",
            path: "parser",
            message: "No supported investment-provider format matched document content.".into(),
        });
    };
    let rows = logical_rows(lines);
    let mut events = match provider {
        Provider::Nu => parse_nu(source, lines, &rows),
        Provider::Plenti => parse_plenti(source, &rows),
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
    Ok((provider, events))
}

fn logical_rows(lines: &[ExtractedLine]) -> Vec<ExtractedLine> {
    let mut rows = lines
        .iter()
        .filter(|line| line.bbox.is_none())
        .cloned()
        .map(|line| vec![line])
        .collect::<Vec<_>>();
    let mut cells = lines
        .iter()
        .filter(|line| line.bbox.is_some())
        .cloned()
        .collect::<Vec<_>>();
    cells.sort_by(|a, b| {
        let ay = a.bbox.expect("layout cells have bounds").y;
        let by = b.bbox.expect("layout cells have bounds").y;
        a.page
            .cmp(&b.page)
            .then_with(|| ay.total_cmp(&by))
            .then_with(|| a.bbox.unwrap().x.total_cmp(&b.bbox.unwrap().x))
    });
    for cell in cells {
        let y = cell.bbox.expect("layout cells have bounds").y;
        if let Some(row) = rows.last_mut() {
            let first = &row[0];
            let row_y = first.bbox.map(|bounds| bounds.y);
            if first.page == cell.page && row_y.is_some_and(|row_y| (row_y - y).abs() <= 3.0) {
                row.push(cell);
                continue;
            }
        }
        rows.push(vec![cell]);
    }
    rows.into_iter()
        .map(|mut cells| {
            cells.sort_by(|a, b| {
                a.bbox
                    .map(|bounds| bounds.x)
                    .unwrap_or(0.0)
                    .total_cmp(&b.bbox.map(|bounds| bounds.x).unwrap_or(0.0))
            });
            let mut row = cells.remove(0);
            for cell in cells {
                row.text.push(' ');
                row.text.push_str(&cell.text);
            }
            row
        })
        .collect()
}

fn parse_nu(
    source: &str,
    lines: &[ExtractedLine],
    logical_rows: &[ExtractedLine],
) -> Vec<ProviderEvent> {
    let re=Regex::new(r"(?i)(\d{2})\s+(ene|feb|mar|abr|may|jun|jul|ago|sep|oct|nov|dic)\s+(Abriste\s+un\s+CDT|Recibiste\s+dinero\s+de\s+un\s+CDT|Enviaste\s+a\s+Plenti)\s+([+-]?\$\s*[\d.,]+)").unwrap();
    let Some(year) = year_from(lines) else {
        return vec![];
    };
    let linear_text = lines
        .iter()
        .filter(|x| x.bbox.is_none())
        .map(|x| x.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let mut rows = logical_rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.bbox.is_some())
        .map(|(index, row)| (row.page, index + 1, row.text.clone()))
        .collect::<Vec<_>>();
    rows.extend(
        re.captures_iter(&linear_text)
            .enumerate()
            .filter_map(|(index, captures)| {
                Some((1, index + 1, captures.get(0)?.as_str().to_owned()))
            }),
    );
    rows.iter()
        .filter_map(|(page, row_index, text)| {
            let c = re.captures(text)?;
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
                provider: Provider::Nu,
                parser_id: "nu.investment-document.v1".into(),
                parser_version: PARSER_VERSION.into(),
                event_type: EventType::parse(typ).expect("NU parser emits a closed event type"),
                provider_effective_date: date,
                currency: "COP".into(),
                amount_minor: Some(amount),
                instrument_hint: None,
                quantity: None,
                external_reference: None,
                page_number: *page,
                row_index: *row_index,
                evidence_redaction: redact(c.get(0)?.as_str()),
                fingerprint: fp,
                status: ReviewStatus::PendingReview,
                decision: None,
                reconciled_kind: None,
                reconciled_id: None,
                source_document_id: source.into(),
                import_batch_id: None,
                provenance_id: None,
                accepted_snapshot_id: None,
                account_id: None,
            })
        })
        .collect()
}
fn parse_plenti(source: &str, lines: &[ExtractedLine]) -> Vec<ProviderEvent> {
    let re = Regex::new(
        r"^(\d{2}/\d{2}/\d{4})\s+(Recarga Bre-B|Depósito amigo Plenti).*?([\d,.]+)\s+([\d,.]+)$",
    )
    .unwrap();
    parse_rows(source, Provider::Plenti, lines, |text| {
        let c = re.captures(text)?;
        Some(ParsedRow {
            event_type: EventType::Deposit,
            effective_date: date_slash(&c[1])?,
            currency: "COP",
            amount_minor: money(&c[4])?,
            instrument_hint: None,
            quantity: None,
        })
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
    parse_rows(source, Provider::Wenia, lines, |text| {
        let c = re.captures(text)?;
        Some(ParsedRow {
            event_type: EventType::ObservedPosition,
            effective_date: date.clone(),
            currency: "USD",
            amount_minor: money(&c[4])?,
            instrument_hint: Some(c[1].to_string()),
            quantity: Some(decimal(&c[2])?),
        })
    })
}
struct ParsedRow {
    event_type: EventType,
    effective_date: String,
    currency: &'static str,
    amount_minor: i64,
    instrument_hint: Option<String>,
    quantity: Option<String>,
}
fn parse_rows<F>(
    source: &str,
    provider: Provider,
    lines: &[ExtractedLine],
    mut f: F,
) -> Vec<ProviderEvent>
where
    F: FnMut(&str) -> Option<ParsedRow>,
{
    let mut out = vec![];
    for (i, l) in lines.iter().enumerate() {
        if let Some(parsed) = f(&l.text) {
            let evidence = redact(&l.text);
            let fp = digest(&format!(
                "{provider}|{}|{}|{}|{}|{:?}|{:?}",
                parsed.event_type,
                parsed.effective_date,
                parsed.currency,
                parsed.amount_minor,
                parsed.instrument_hint,
                parsed.quantity
            ));
            out.push(ProviderEvent {
                id: format!("invevt_{}", &digest(&format!("{source}|{fp}"))[..24]),
                provider,
                parser_id: format!("{provider}.investment-document.v1"),
                parser_version: PARSER_VERSION.into(),
                event_type: parsed.event_type,
                provider_effective_date: parsed.effective_date,
                currency: parsed.currency.into(),
                amount_minor: Some(parsed.amount_minor),
                instrument_hint: parsed.instrument_hint,
                quantity: parsed.quantity,
                external_reference: None,
                page_number: l.page,
                row_index: i + 1,
                evidence_redaction: evidence,
                fingerprint: fp,
                status: ReviewStatus::PendingReview,
                decision: None,
                reconciled_kind: None,
                reconciled_id: None,
                source_document_id: source.into(),
                import_batch_id: None,
                provenance_id: None,
                accepted_snapshot_id: None,
                account_id: None,
            });
        }
    }
    out
}
pub(crate) fn extract(path: &Path, password: &str) -> Result<Vec<ExtractedLine>> {
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
