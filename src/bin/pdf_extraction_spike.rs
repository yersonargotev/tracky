use anyhow::{anyhow, Context, Result};
use clap::Parser;
use pdf_oxide::PdfDocument as OxideDocument;
use pdfium_render::prelude::*;
use regex::Regex;
use rpassword::prompt_password;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(about = "Local PDF extraction spike for Tracky; never prints or stores passwords.")]
struct Args {
    /// Files to inspect. Defaults to assets/*.pdf.
    #[arg(value_name = "PDF")]
    files: Vec<PathBuf>,

    /// Emit pretty JSON.
    #[arg(long)]
    pretty: bool,

    /// Skip interactive prompt when a password env var is missing.
    #[arg(long)]
    no_prompt: bool,

    /// Output JSON file. If omitted, writes to stdout.
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct SpikeReport {
    generated_by: String,
    inputs: Vec<String>,
    extractors: Vec<&'static str>,
    documents: Vec<DocumentReport>,
    summary: Summary,
}

#[derive(Debug, Serialize)]
struct Summary {
    by_extractor: BTreeMap<&'static str, ExtractorSummary>,
}

#[derive(Debug, Default, Serialize)]
struct ExtractorSummary {
    attempted: usize,
    opened: usize,
    useful_text: usize,
    layout_lines: usize,
    errors: usize,
}

#[derive(Debug, Serialize)]
struct DocumentReport {
    file: String,
    institution: String,
    password_source: String,
    sha256_prefix: String,
    results: Vec<ExtractorResult>,
    parsing: ParsingDiagnostic,
}

#[derive(Debug, Serialize)]
struct ExtractorResult {
    extractor: &'static str,
    opened: bool,
    encrypted: Option<bool>,
    authenticated: Option<bool>,
    pages: Option<usize>,
    elapsed_ms: u128,
    text_chars: usize,
    line_count: usize,
    useful: Usefulness,
    layout: LayoutEvidence,
    samples: Vec<SampleLine>,
    error: Option<String>,
}

#[derive(Debug, Default, Serialize)]
struct Usefulness {
    has_dates: bool,
    has_amounts: bool,
    has_descriptions: bool,
    has_balances: bool,
    date_hits: usize,
    amount_hits: usize,
    description_like_lines: usize,
    balance_lines: usize,
}

#[derive(Debug, Default, Serialize)]
struct LayoutEvidence {
    has_coordinates: bool,
    has_lines: bool,
    line_order: String,
    bbox_samples: Vec<BBox>,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct BBox {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Debug, Serialize)]
struct SampleLine {
    page: usize,
    text: String,
    bbox: Option<BBox>,
}

#[derive(Debug, Serialize)]
struct ParsingDiagnostic {
    extractor: &'static str,
    parser: String,
    status: String,
    candidate_count: usize,
    candidates: Vec<MovementCandidate>,
    row_samples: Vec<ParsedRowSample>,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct MovementCandidate {
    page: usize,
    row_bbox: Option<BBox>,
    date: String,
    description_sample: String,
    amount: ParsedMoney,
    balance: Option<ParsedMoney>,
    confidence: f32,
    evidence_text: String,
}

#[derive(Debug, Clone, Serialize)]
struct ParsedMoney {
    text: String,
    value_minor_units: Option<i64>,
    currency: &'static str,
}

#[derive(Debug, Serialize)]
struct ParsedRowSample {
    kind: &'static str,
    page: usize,
    text: String,
    bbox: Option<BBox>,
}

#[derive(Debug, Clone)]
struct VisualRow {
    page: usize,
    cells: Vec<ExtractedLine>,
    bbox: Option<BBox>,
}

#[derive(Debug, Clone)]
struct ExtractedLine {
    page: usize,
    text: String,
    bbox: Option<BBox>,
}

#[derive(Debug, Clone)]
struct MoneyToken {
    money: ParsedMoney,
    x: f32,
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();
    let files = if args.files.is_empty() {
        default_assets()?
    } else {
        args.files.clone()
    };

    let mut documents = Vec::new();
    for file in &files {
        let password = password_for(file, args.no_prompt)?;
        documents.push(inspect_document(file, &password)?);
    }

    let summary = summarize(&documents);
    let report = SpikeReport {
        generated_by: "cargo run --bin pdf-extraction-spike -- --pretty".to_string(),
        inputs: files.iter().map(|path| display_path(path)).collect(),
        extractors: vec!["pdf_oxide", "pdfium-render"],
        documents,
        summary,
    };

    let json = if args.pretty {
        serde_json::to_string_pretty(&report)?
    } else {
        serde_json::to_string(&report)?
    };

    if let Some(output) = args.output {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(output, json)?;
    } else {
        println!("{json}");
    }

    Ok(())
}

fn default_assets() -> Result<Vec<PathBuf>> {
    let expected = [
        "assets/nequi-abril.pdf",
        "assets/nequi-mayo.pdf",
        "assets/nequi-junio.pdf",
        "assets/rappi-abril.pdf",
        "assets/rappi-mayo.pdf",
        "assets/rappi-junio.pdf",
    ];
    expected
        .iter()
        .map(|path| {
            let path = PathBuf::from(path);
            if path.exists() {
                Ok(path)
            } else {
                Err(anyhow!("missing input PDF: {}", path.display()))
            }
        })
        .collect()
}

fn password_for(file: &Path, no_prompt: bool) -> Result<String> {
    let institution = institution_for(file);
    let month = file
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|stem| stem.split('-').nth(1))
        .map(|s| s.to_ascii_uppercase());

    let mut keys = Vec::new();
    if let Some(month) = month {
        keys.push(format!(
            "TRACKY_{}_{}_PDF_PASSWORD",
            institution.to_ascii_uppercase(),
            month
        ));
    }
    keys.push(format!(
        "TRACKY_{}_PDF_PASSWORD",
        institution.to_ascii_uppercase()
    ));

    for key in &keys {
        if let Ok(value) = env::var(key) {
            if !value.is_empty() {
                return Ok(value);
            }
        }
    }

    if no_prompt {
        return Err(anyhow!(
            "missing password env var for {}: tried {}",
            display_path(file),
            keys.join(", ")
        ));
    }

    prompt_password(format!(
        "Password for {} (not stored; tried {}): ",
        display_path(file),
        keys.join(", ")
    ))
    .context("failed to read password from prompt")
}

fn inspect_document(file: &Path, password: &str) -> Result<DocumentReport> {
    let bytes = fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let sha256_prefix = hex_prefix(&bytes);
    let institution = institution_for(file).to_string();
    let password_source = format!("env_or_prompt:{}", institution.to_ascii_uppercase());

    let parsing = run_pdf_oxide_parser(file, password, &institution);

    Ok(DocumentReport {
        file: display_path(file),
        institution,
        password_source,
        sha256_prefix,
        results: vec![run_pdf_oxide(file, password), run_pdfium(file, password)],
        parsing,
    })
}

fn run_pdf_oxide_parser(file: &Path, password: &str, institution: &str) -> ParsingDiagnostic {
    match extract_pdf_oxide_lines(file, password) {
        Ok(lines) => parse_movements(institution, &lines),
        Err(error) => ParsingDiagnostic {
            extractor: "pdf_oxide",
            parser: format!("{institution}_movement_rows_v0"),
            status: "error".to_string(),
            candidate_count: 0,
            candidates: Vec::new(),
            row_samples: Vec::new(),
            notes: vec![format!("pdf_oxide parser extraction failed: {error}")],
        },
    }
}

fn extract_pdf_oxide_lines(file: &Path, password: &str) -> Result<Vec<ExtractedLine>> {
    let doc = open_authenticated_pdf_oxide(file, password)?;
    extract_pdf_oxide_document_lines(&doc)
}

fn open_authenticated_pdf_oxide(file: &Path, password: &str) -> Result<OxideDocument> {
    let doc = OxideDocument::open(file)?;
    let authenticated = doc.authenticate(password.as_bytes())?;
    if !authenticated {
        return Err(anyhow!("pdf_oxide authentication returned false"));
    }
    Ok(doc)
}

fn extract_pdf_oxide_document_lines(doc: &OxideDocument) -> Result<Vec<ExtractedLine>> {
    let mut lines = Vec::new();
    for page in 0..doc.page_count()? {
        let page_lines = doc
            .extract_text_lines(page)
            .with_context(|| format!("pdf_oxide line extraction failed on page {}", page + 1))?;
        for line in page_lines {
            lines.push(ExtractedLine {
                page: page + 1,
                text: normalize_spaces(&line.text),
                bbox: Some(BBox {
                    x: line.bbox.x,
                    y: line.bbox.y,
                    width: line.bbox.width,
                    height: line.bbox.height,
                }),
            });
        }
    }
    Ok(lines)
}

fn parse_movements(institution: &str, lines: &[ExtractedLine]) -> ParsingDiagnostic {
    let rows = visual_rows(lines);
    let candidates = match institution {
        "rappi" => parse_rappi_rows(&rows),
        _ => parse_nequi_rows(&rows),
    };
    let row_samples = diagnostic_row_samples(institution, &rows, &candidates);
    let notes = match institution {
        "rappi" => vec![
            "Diagnostic only: detects dated Rappi transaction-table rows from pdf_oxide bboxes; not canonical import.".to_string(),
            "Descriptions are redacted samples and may omit wrapped continuation text.".to_string(),
            "Rappi statements do not expose a per-row running balance in the transaction table, so balance is normally null.".to_string(),
        ],
        _ => vec![
            "Diagnostic only: detects Nequi rows under Fecha del movimiento / Descripción / Valor / Saldo headers; not canonical import.".to_string(),
            "Some pdf_oxide cells combine amount and balance; parser splits money tokens by regex.".to_string(),
            "Descriptions are redacted samples for agent inspection.".to_string(),
        ],
    };
    ParsingDiagnostic {
        extractor: "pdf_oxide",
        parser: format!("{institution}_movement_rows_v0"),
        status: "diagnostic_candidates".to_string(),
        candidate_count: candidates.len(),
        candidates,
        row_samples,
        notes,
    }
}

fn visual_rows(lines: &[ExtractedLine]) -> Vec<VisualRow> {
    let mut sorted = lines
        .iter()
        .filter(|line| !line.text.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    sorted.sort_by(|a, b| {
        a.page.cmp(&b.page).then_with(|| {
            let ay = a.bbox.map(|bbox| bbox.y).unwrap_or_default();
            let by = b.bbox.map(|bbox| bbox.y).unwrap_or_default();
            by.partial_cmp(&ay)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let ax = a.bbox.map(|bbox| bbox.x).unwrap_or_default();
                    let bx = b.bbox.map(|bbox| bbox.x).unwrap_or_default();
                    ax.partial_cmp(&bx).unwrap_or(std::cmp::Ordering::Equal)
                })
        })
    });

    let mut rows: Vec<VisualRow> = Vec::new();
    for line in sorted {
        let line_y = line.bbox.map(|bbox| bbox.y).unwrap_or_default();
        let line_h = line.bbox.map(|bbox| bbox.height).unwrap_or(4.0).max(4.0);
        if let Some(row) = rows.iter_mut().rev().find(|row| {
            row.page == line.page
                && row
                    .bbox
                    .is_some_and(|bbox| (bbox.y - line_y).abs() <= line_h.max(bbox.height) * 0.75)
        }) {
            row.cells.push(line);
            row.cells.sort_by(|a, b| {
                bbox_x(a)
                    .partial_cmp(&bbox_x(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            row.bbox = union_bbox(row.cells.iter().filter_map(|cell| cell.bbox));
        } else {
            rows.push(VisualRow {
                page: line.page,
                bbox: line.bbox,
                cells: vec![line],
            });
        }
    }
    rows
}

fn parse_nequi_rows(rows: &[VisualRow]) -> Vec<MovementCandidate> {
    let date_re = Regex::new(r"\b\d{1,2}/\d{1,2}/\d{4}\b").unwrap();
    rows.iter()
        .filter_map(|row| {
            let row_text = row_text(row);
            let date = date_re.find(&row_text)?.as_str().to_string();
            if row_text.to_lowercase().contains("fecha del movimiento") {
                return None;
            }
            let money = money_tokens(&row_text);
            if money.is_empty() {
                return None;
            }
            let description = row
                .cells
                .iter()
                .filter(|cell| bbox_x(cell) >= 145.0 && bbox_x(cell) < 365.0)
                .map(|cell| cell.text.as_str())
                .filter(|text| !date_re.is_match(text) && money_tokens(text).is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let description = if description.trim().is_empty() {
                description_from_row(&row_text, &date)
            } else {
                description
            };
            let confidence = if money.len() >= 2 && !description.trim().is_empty() {
                0.92
            } else {
                0.78
            };
            Some(MovementCandidate {
                page: row.page,
                row_bbox: row.bbox,
                date,
                description_sample: redact_description_sample(&description),
                amount: money[0].clone(),
                balance: money.get(1).cloned(),
                confidence,
                evidence_text: redact_row_for_evidence(&row_text),
            })
        })
        .collect()
}

fn parse_rappi_rows(rows: &[VisualRow]) -> Vec<MovementCandidate> {
    let date_re = Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").unwrap();
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let row_text = row_text(row);
            let date_match = date_re.find(&row_text)?;
            let money = money_tokens_with_x(row);
            if money.is_empty() || row_text.to_lowercase().contains("detalle de transacciones") {
                return None;
            }
            let mut description = description_from_row(&row_text, date_match.as_str());
            for near in nearby_description_rows(rows, index) {
                if !description.is_empty() {
                    description.push(' ');
                }
                description.push_str(&near);
            }
            let amount = select_rappi_amount(&money);
            Some(MovementCandidate {
                page: row.page,
                row_bbox: row.bbox,
                date: date_match.as_str().to_string(),
                description_sample: redact_description_sample(&description),
                amount,
                balance: None,
                confidence: if description.trim().is_empty() {
                    0.72
                } else {
                    0.86
                },
                evidence_text: redact_row_for_evidence(&row_text),
            })
        })
        .collect()
}

fn select_rappi_amount(money: &[MoneyToken]) -> ParsedMoney {
    // Rappi rows can contain purchase value, foreign-currency original value, fees,
    // and/or taxes. Use visual order from pdf_oxide cells and prefer the first
    // non-zero monetary value; zero-valued cells are usually ancillary columns.
    money
        .iter()
        .filter(|token| token.money.value_minor_units != Some(0))
        .min_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
        .or_else(|| {
            money
                .iter()
                .min_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
        })
        .map(|token| token.money.clone())
        .expect("caller ensures at least one money token")
}

fn money_tokens_with_x(row: &VisualRow) -> Vec<MoneyToken> {
    row.cells
        .iter()
        .flat_map(|cell| {
            let x = bbox_x(cell);
            money_tokens(&cell.text)
                .into_iter()
                .map(move |money| MoneyToken { money, x })
        })
        .collect()
}

fn diagnostic_row_samples(
    institution: &str,
    rows: &[VisualRow],
    candidates: &[MovementCandidate],
) -> Vec<ParsedRowSample> {
    let candidate_keys = candidates
        .iter()
        .filter_map(|candidate| candidate.row_bbox.map(|bbox| (candidate.page, bbox.y)))
        .collect::<Vec<_>>();
    let mut samples = Vec::new();

    for row in rows
        .iter()
        .filter(|row| is_header_row(institution, row))
        .take(4)
    {
        samples.push(row_sample("header", row));
    }

    for row in rows
        .iter()
        .filter(|row| is_raw_table_row(institution, row))
        .take(6)
    {
        samples.push(row_sample("raw_table", row));
    }

    for row in rows
        .iter()
        .filter(|row| is_near_miss_row(institution, row, &candidate_keys))
        .take(6)
    {
        samples.push(row_sample("near_miss", row));
    }

    for candidate in candidates.iter().take(8) {
        samples.push(ParsedRowSample {
            kind: "candidate",
            page: candidate.page,
            text: candidate.evidence_text.clone(),
            bbox: candidate.row_bbox,
        });
    }

    samples
}

fn row_sample(kind: &'static str, row: &VisualRow) -> ParsedRowSample {
    ParsedRowSample {
        kind,
        page: row.page,
        text: redact_row_for_evidence(&row_text(row)),
        bbox: row.bbox,
    }
}

fn is_header_row(institution: &str, row: &VisualRow) -> bool {
    let lower = row_text(row).to_lowercase();
    match institution {
        "rappi" => {
            lower.contains("detalle de transacciones")
                || (lower.contains("fecha") && lower.contains("descrip"))
        }
        _ => {
            lower.contains("fecha del movimiento")
                || (lower.contains("descrip") && lower.contains("saldo"))
        }
    }
}

fn is_raw_table_row(institution: &str, row: &VisualRow) -> bool {
    let text = row_text(row);
    match institution {
        "rappi" => Regex::new(r"\b\d{4}-\d{2}-\d{2}\b")
            .unwrap()
            .is_match(&text),
        _ => Regex::new(r"\b\d{1,2}/\d{1,2}/\d{4}\b")
            .unwrap()
            .is_match(&text),
    }
}

fn is_near_miss_row(institution: &str, row: &VisualRow, candidate_keys: &[(usize, f32)]) -> bool {
    if row.bbox.is_some_and(|bbox| {
        candidate_keys
            .iter()
            .any(|(page, y)| *page == row.page && (*y - bbox.y).abs() < 0.1)
    }) {
        return false;
    }
    let text = row_text(row);
    if is_header_row(institution, row) {
        return false;
    }
    !money_tokens(&text).is_empty()
        || Regex::new(r"\b(?:\d{1,2}/\d{1,2}/\d{4}|\d{4}-\d{2}-\d{2})\b")
            .unwrap()
            .is_match(&text)
}

fn nearby_description_rows(rows: &[VisualRow], index: usize) -> Vec<String> {
    let Some(row_bbox) = rows[index].bbox else {
        return Vec::new();
    };
    let mut parts = Vec::new();
    let date_re = Regex::new(r"\d{4}-\d{2}-\d{2}").unwrap();
    for offset in [-1isize, 1] {
        let near_index = index as isize + offset;
        if near_index < 0 || near_index as usize >= rows.len() {
            continue;
        }
        let near = &rows[near_index as usize];
        let Some(near_bbox) = near.bbox else {
            continue;
        };
        if near.page != rows[index].page || (near_bbox.y - row_bbox.y).abs() > 8.0 {
            continue;
        }
        let text = row_text(near);
        if date_re.is_match(&text) || !money_tokens(&text).is_empty() {
            continue;
        }
        let desc = near
            .cells
            .iter()
            .filter(|cell| bbox_x(cell) >= 140.0 && bbox_x(cell) < 235.0)
            .map(|cell| cell.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if !desc.trim().is_empty() {
            parts.push(desc);
        }
    }
    parts
}

fn row_text(row: &VisualRow) -> String {
    normalize_spaces(
        &row.cells
            .iter()
            .map(|cell| cell.text.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn bbox_x(line: &ExtractedLine) -> f32 {
    line.bbox.map(|bbox| bbox.x).unwrap_or_default()
}

fn money_tokens(text: &str) -> Vec<ParsedMoney> {
    let money_re = Regex::new(r"\$\s*-?\d{1,3}(?:[.,]\d{3})*(?:[.,]\d{1,2})?").unwrap();
    money_re
        .find_iter(text)
        .map(|hit| ParsedMoney {
            text: hit.as_str().to_string(),
            value_minor_units: parse_money_minor_units(hit.as_str()),
            currency: "COP",
        })
        .collect()
}

fn parse_money_minor_units(text: &str) -> Option<i64> {
    let negative = text.contains('-');
    let cleaned = text
        .chars()
        .filter(|ch| ch.is_ascii_digit() || *ch == '.' || *ch == ',')
        .collect::<String>();
    if cleaned.is_empty() {
        return None;
    }
    let last_dot = cleaned.rfind('.');
    let last_comma = cleaned.rfind(',');
    let decimal_index = match (last_dot, last_comma) {
        (Some(dot), Some(comma)) => Some(dot.max(comma)),
        (Some(index), None) | (None, Some(index)) => {
            let decimals = cleaned.len().saturating_sub(index + 1);
            if decimals <= 2 {
                Some(index)
            } else {
                None
            }
        }
        (None, None) => None,
    };
    let (whole, decimals) = if let Some(index) = decimal_index {
        (&cleaned[..index], &cleaned[index + 1..])
    } else {
        (cleaned.as_str(), "")
    };
    let whole_digits = whole
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect::<String>();
    let mut decimal_digits = decimals
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect::<String>();
    while decimal_digits.len() < 2 {
        decimal_digits.push('0');
    }
    decimal_digits.truncate(2);
    let whole_minor = whole_digits.parse::<i64>().ok()?.checked_mul(100)?;
    let decimal_minor = if decimal_digits.is_empty() {
        0
    } else {
        decimal_digits.parse::<i64>().ok()?
    };
    let value = whole_minor.checked_add(decimal_minor)?;
    Some(if negative { -value } else { value })
}

fn description_from_row(row_text: &str, date: &str) -> String {
    let mut text = row_text.replace(date, " ");
    for money in money_tokens(row_text) {
        text = text.replace(&money.text, " ");
    }
    for token in [
        "Virtual", "Fisica", "Física", "-", "N/A", "1 de 1", "0,0000%", "0,00%", "0%",
    ] {
        text = text.replace(token, " ");
    }
    normalize_spaces(&text)
}

fn redact_description_sample(text: &str) -> String {
    let mut sample = redact_counterparties(&redact_line(text));
    if sample.split_whitespace().count() > 8 {
        sample = sample
            .split_whitespace()
            .take(8)
            .collect::<Vec<_>>()
            .join(" ");
        sample.push_str(" …");
    }
    sample
}

fn redact_row_for_evidence(text: &str) -> String {
    let text = redact_line(text);
    redact_description_sample(&text)
}

fn run_pdf_oxide(file: &Path, password: &str) -> ExtractorResult {
    let started = Instant::now();
    let result = (|| -> Result<(bool, bool, usize, String, Vec<ExtractedLine>)> {
        let doc = open_authenticated_pdf_oxide(file, password)?;
        let encrypted = doc.is_encrypted();
        let authenticated = true;
        let pages = doc.page_count()?;
        let mut text = String::new();
        for page in 0..pages {
            let page_text = doc.extract_text(page).with_context(|| {
                format!("pdf_oxide text extraction failed on page {}", page + 1)
            })?;
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&page_text);
        }
        let lines = extract_pdf_oxide_document_lines(&doc)?;
        Ok((encrypted, authenticated, pages, text, lines))
    })();

    result_to_report("pdf_oxide", started, result)
}

fn run_pdfium(file: &Path, password: &str) -> ExtractorResult {
    let started = Instant::now();
    let result = (|| -> Result<(bool, bool, usize, String, Vec<ExtractedLine>)> {
        let pdfium = pdfium_auto::bind_pdfium_silent()
            .map_err(|e| anyhow!("failed to bind/download Pdfium: {e}"))?;
        let document = pdfium.load_pdf_from_file(file, Some(password))?;
        let pages = document.pages().len() as usize;
        let mut all_text = String::new();
        let mut lines = Vec::new();
        for page_index in 0..pages {
            let page = document.pages().get(page_index as u16)?;
            let page_text = page.text()?;
            let text = page_text.all();
            if !all_text.is_empty() {
                all_text.push('\n');
            }
            all_text.push_str(&text);
            lines.extend(group_pdfium_lines(page_index + 1, &page_text));
        }
        // Pdfium exposes password opening here but not a simple encrypted/authenticated flag in this path.
        Ok((true, true, pages, all_text, lines))
    })();

    result_to_report("pdfium-render", started, result)
}

fn group_pdfium_lines(page: usize, page_text: &PdfPageText<'_>) -> Vec<ExtractedLine> {
    let mut chars = Vec::new();
    for ch in page_text.chars().iter() {
        let Some(text) = ch.unicode_string() else {
            continue;
        };
        if text.trim().is_empty() && text != " " {
            continue;
        }
        let Ok(bounds) = ch.loose_bounds().or_else(|_| ch.tight_bounds()) else {
            continue;
        };
        chars.push((
            text,
            BBox {
                x: bounds.left().value,
                y: bounds.bottom().value,
                width: bounds.width().value,
                height: bounds.height().value,
            },
        ));
    }

    chars.sort_by(|a, b| {
        b.1.y
            .partial_cmp(&a.1.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.1.x
                    .partial_cmp(&b.1.x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let mut grouped: Vec<Vec<(String, BBox)>> = Vec::new();
    for item in chars {
        let target = grouped.iter_mut().find(|line| {
            let avg_y = line.iter().map(|(_, b)| b.y).sum::<f32>() / line.len() as f32;
            (avg_y - item.1.y).abs() <= item.1.height.max(2.0) * 0.65
        });
        if let Some(line) = target {
            line.push(item);
        } else {
            grouped.push(vec![item]);
        }
    }

    grouped
        .into_iter()
        .filter_map(|mut line| {
            line.sort_by(|a, b| {
                a.1.x
                    .partial_cmp(&b.1.x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let text = line.iter().map(|(s, _)| s.as_str()).collect::<String>();
            let text = normalize_spaces(&text);
            if text.is_empty() {
                return None;
            }
            let bbox = union_bbox(line.iter().map(|(_, b)| *b));
            Some(ExtractedLine { page, text, bbox })
        })
        .collect()
}

fn result_to_report(
    extractor: &'static str,
    started: Instant,
    result: Result<(bool, bool, usize, String, Vec<ExtractedLine>)>,
) -> ExtractorResult {
    match result {
        Ok((encrypted, authenticated, pages, text, lines)) => {
            let useful = usefulness(&text, &lines);
            let layout = LayoutEvidence {
                has_coordinates: lines.iter().any(|line| line.bbox.is_some()),
                has_lines: !lines.is_empty(),
                line_order: "page_then_visual_top_to_bottom_left_to_right_or_extractor_order"
                    .to_string(),
                bbox_samples: lines.iter().filter_map(|line| line.bbox).take(8).collect(),
            };
            let samples = sample_lines(&lines);
            ExtractorResult {
                extractor,
                opened: true,
                encrypted: Some(encrypted),
                authenticated: Some(authenticated),
                pages: Some(pages),
                elapsed_ms: started.elapsed().as_millis(),
                text_chars: text.chars().count(),
                line_count: lines.len(),
                useful,
                layout,
                samples,
                error: None,
            }
        }
        Err(error) => ExtractorResult {
            extractor,
            opened: false,
            encrypted: None,
            authenticated: None,
            pages: None,
            elapsed_ms: started.elapsed().as_millis(),
            text_chars: 0,
            line_count: 0,
            useful: Usefulness::default(),
            layout: LayoutEvidence::default(),
            samples: Vec::new(),
            error: Some(error.to_string()),
        },
    }
}

fn usefulness(text: &str, lines: &[ExtractedLine]) -> Usefulness {
    let date_re = Regex::new(r"(?i)\b(\d{1,2}[/-]\d{1,2}(?:[/-]\d{2,4})?|\d{4}-\d{2}-\d{2}|(?:ene|feb|mar|abr|may|jun|jul|ago|sep|oct|nov|dic)[a-z]*\.?\s+\d{1,2})\b").unwrap();
    let amount_re =
        Regex::new(r"(?:\$|COP)?\s*-?\d{1,3}(?:[.,]\d{3})+(?:[.,]\d{2})?|(?:\$|COP)\s*-?\d+")
            .unwrap();
    let balance_re =
        Regex::new(r"(?i)\b(saldo|balance|disponible|total|cupo|pago\s+m[ií]nimo|deuda)\b")
            .unwrap();
    let description_re = Regex::new(r"(?i)[a-záéíóúñ]{4,}").unwrap();

    let description_like_lines = lines
        .iter()
        .filter(|line| description_re.is_match(&line.text) && amount_re.is_match(&line.text))
        .count();
    let balance_lines = lines
        .iter()
        .filter(|line| balance_re.is_match(&line.text))
        .count();
    let date_hits = date_re.find_iter(text).count();
    let amount_hits = amount_re.find_iter(text).count();

    Usefulness {
        has_dates: date_hits > 0,
        has_amounts: amount_hits > 0,
        has_descriptions: description_like_lines > 0,
        has_balances: balance_lines > 0,
        date_hits,
        amount_hits,
        description_like_lines,
        balance_lines,
    }
}

fn sample_lines(lines: &[ExtractedLine]) -> Vec<SampleLine> {
    lines
        .iter()
        .filter(|line| {
            let lower = line.text.to_lowercase();
            lower.contains("saldo")
                || lower.contains("total")
                || lower.contains("fecha")
                || line.text.contains('$')
                || line.text.chars().any(|c| c.is_ascii_digit())
        })
        .take(12)
        .map(|line| SampleLine {
            page: line.page,
            text: redact_line(&line.text),
            bbox: line.bbox,
        })
        .collect()
}

fn redact_counterparties(text: &str) -> String {
    let counterparties = [
        Regex::new(r"(?i)(BRE-B:\s*)[^$]+$ ").unwrap(),
        Regex::new(r"(?i)(BRE-B:\s*)[^$]+").unwrap(),
        Regex::new(r"(?i)(\bA:\s*)[^$]+").unwrap(),
        Regex::new(r"(?i)(recibido de\s+)[^$]+").unwrap(),
        Regex::new(r"(?i)(\bPara\s+)[^$]+").unwrap(),
        Regex::new(r"(\bDe\s+)[^$]+").unwrap(),
    ];
    let mut redacted = text.to_string();
    for re in counterparties {
        redacted = re.replace_all(&redacted, "$1<counterparty>").to_string();
    }
    redacted
}

fn redact_line(text: &str) -> String {
    let email_re = Regex::new(r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b").unwrap();
    let money_re = Regex::new(
        r"(?:(?:\$|COP)\s*)-?\d+(?:[.,]\d{3})*(?:[.,]\d{1,2})?|\b-?\d{1,3}(?:[.,]\d{3})+(?:[.,]\d{1,2})?\b",
    )
    .unwrap();
    let long_number_re = Regex::new(r"\b\d{5,}\b").unwrap();
    let text = email_re.replace_all(text, "<email>");
    let text = money_re.replace_all(&text, "<amount>");
    let text = long_number_re.replace_all(&text, "<number>");
    let address_re = Regex::new(r"(?i)\bDirecci[oó]n\b.*$").unwrap();
    let card_re = Regex::new(r"(?i)(N[uú]mero de tarjeta\s+\w+)\s+\d{4}\b").unwrap();
    let holder_re =
        Regex::new(r"(?i)(Detalle de transacciones:\s*)[^()]+(\s*\(Titular\))").unwrap();
    let text = address_re.replace_all(&text, "Dirección <address>");
    let text = card_re.replace_all(&text, "$1 <card-last4>");
    let text = holder_re.replace_all(&text, "$1<cardholder>$2");
    normalize_spaces(&redact_counterparties(&text))
}

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn union_bbox<I>(mut boxes: I) -> Option<BBox>
where
    I: Iterator<Item = BBox>,
{
    let first = boxes.next()?;
    let (mut left, mut bottom, mut right, mut top) = (
        first.x,
        first.y,
        first.x + first.width,
        first.y + first.height,
    );
    for bbox in boxes {
        left = left.min(bbox.x);
        bottom = bottom.min(bbox.y);
        right = right.max(bbox.x + bbox.width);
        top = top.max(bbox.y + bbox.height);
    }
    Some(BBox {
        x: left,
        y: bottom,
        width: right - left,
        height: top - bottom,
    })
}

fn summarize(documents: &[DocumentReport]) -> Summary {
    let mut by_extractor = BTreeMap::<&'static str, ExtractorSummary>::new();
    for document in documents {
        for result in &document.results {
            let summary = by_extractor.entry(result.extractor).or_default();
            summary.attempted += 1;
            if result.opened {
                summary.opened += 1;
            }
            if result.useful.has_dates
                && result.useful.has_amounts
                && result.useful.has_descriptions
            {
                summary.useful_text += 1;
            }
            if result.layout.has_lines && result.layout.has_coordinates {
                summary.layout_lines += 1;
            }
            if result.error.is_some() {
                summary.errors += 1;
            }
        }
    }
    Summary { by_extractor }
}

fn institution_for(file: &Path) -> &str {
    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if stem.to_ascii_lowercase().starts_with("rappi") {
        "rappi"
    } else {
        "nequi"
    }
}

fn display_path(path: &Path) -> String {
    path.strip_prefix(env::current_dir().unwrap_or_default())
        .unwrap_or(path)
        .display()
        .to_string()
}

fn hex_prefix(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}
