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
use tracky::pdf::{
    inspect_pdf as inspect_pdf_core, redact_line, supported_institution_hint_from_path,
    CredentialSource, InspectPdfOptions,
};

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
struct ExtractedLine {
    page: usize,
    text: String,
    bbox: Option<BBox>,
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
        generated_by: "cargo run --example pdf_extraction_spike -- --pretty".to_string(),
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
    let institution = supported_institution_hint_from_path(file);
    let month = file
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|stem| stem.split('-').nth(1))
        .map(|s| s.to_ascii_uppercase());

    let mut keys = Vec::new();
    if let Some(institution) = institution {
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
    }
    keys.push("TRACKY_PDF_PASSWORD".to_string());

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
    let institution = supported_institution_hint_from_path(file);
    let institution_label = institution.unwrap_or("unknown").to_string();
    let password_source = institution
        .map(|institution| format!("env_or_prompt:{}", institution.to_ascii_uppercase()))
        .unwrap_or_else(|| "env_or_prompt:GENERIC".to_string());

    let parsing = run_pdf_oxide_parser(file, password, institution);

    Ok(DocumentReport {
        file: display_path(file),
        institution: institution_label,
        password_source,
        sha256_prefix,
        results: vec![run_pdf_oxide(file, password), run_pdfium(file, password)],
        parsing,
    })
}

fn run_pdf_oxide_parser(
    file: &Path,
    password: &str,
    institution: Option<&str>,
) -> ParsingDiagnostic {
    match inspect_pdf_core(
        file,
        InspectPdfOptions {
            document_credential: Some(password),
            credential_source: CredentialSource::Unknown,
            institution_hint: institution.map(ToString::to_string),
        },
    ) {
        Ok(response) => parsing_diagnostic_from_core(response),
        Err(error) => ParsingDiagnostic {
            extractor: "pdf_oxide",
            parser: institution
                .map(|institution| format!("{institution}.statement.v1"))
                .unwrap_or_else(|| "unknown.statement.v1".to_string()),
            status: "error".to_string(),
            candidate_count: 0,
            candidates: Vec::new(),
            row_samples: Vec::new(),
            notes: vec![format!("product pdf inspection failed: {error}")],
        },
    }
}

fn parsing_diagnostic_from_core(response: tracky::pdf::PdfInspectResponse) -> ParsingDiagnostic {
    let candidates = response
        .candidates
        .iter()
        .map(|candidate| MovementCandidate {
            page: candidate.provenance.page_number,
            row_bbox: candidate.provenance.bbox.map(|bbox| BBox {
                x: bbox.x,
                y: bbox.y,
                width: bbox.width,
                height: bbox.height,
            }),
            date: candidate.posted_date.clone(),
            description_sample: candidate.description.clone(),
            amount: ParsedMoney {
                text: "<amount>".to_string(),
                value_minor_units: Some(candidate.amount_minor),
                currency: candidate.currency,
            },
            balance: candidate.balance_minor.map(|value| ParsedMoney {
                text: "<amount>".to_string(),
                value_minor_units: Some(value),
                currency: candidate.currency,
            }),
            confidence: candidate.confidence,
            evidence_text: candidate.provenance.evidence.text.clone(),
        })
        .collect::<Vec<_>>();
    let row_samples = candidates
        .iter()
        .take(8)
        .map(|candidate| ParsedRowSample {
            kind: "candidate",
            page: candidate.page,
            text: candidate.evidence_text.clone(),
            bbox: candidate.row_bbox,
        })
        .collect::<Vec<_>>();
    let mut notes = vec![
        "Diagnostic wrapper: parser candidates come from tracky::pdf::inspect_pdf; not canonical import."
            .to_string(),
    ];
    notes.extend(response.errors.iter().map(|error| error.message.clone()));
    ParsingDiagnostic {
        extractor: response.extractor_status.extractor,
        parser: response.parser_status.parser_id,
        status: serde_json::to_value(&response.parser_status.status)
            .ok()
            .and_then(|value| value.as_str().map(ToString::to_string))
            .unwrap_or_else(|| "unknown".to_string()),
        candidate_count: candidates.len(),
        candidates,
        row_samples,
        notes,
    }
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
