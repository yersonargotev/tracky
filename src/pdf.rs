use anyhow::{anyhow, Context, Result};
use pdf_oxide::PdfDocument as OxideDocument;
use regex::Regex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

pub const PDF_INSPECT_SCHEMA_VERSION: &str = "tracky.pdf-inspect.v1";

#[derive(Debug, Clone)]
pub struct InspectPdfOptions<'a> {
    pub document_credential: Option<&'a str>,
    pub credential_source: CredentialSource,
    pub institution_hint: Option<String>,
}

impl Default for InspectPdfOptions<'_> {
    fn default() -> Self {
        Self {
            document_credential: None,
            credential_source: CredentialSource::None,
            institution_hint: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialSource {
    None,
    CliFlag,
    Prompt,
    Env,
    Unknown,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PdfInspectResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    pub source_document: SourceDocument,
    pub extractor_status: ExtractorStatus,
    pub parser_status: ParserStatus,
    pub candidates: Vec<CandidateTransaction>,
    pub errors: Vec<TrackyError>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SourceDocument {
    pub id: String,
    pub input_name: String,
    pub content_sha256: String,
    pub mime_type: &'static str,
    pub byte_size: u64,
    pub institution_hint: String,
    pub account_hint: AccountHint,
    pub document_duplicate_status: DocumentDuplicateStatus,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct AccountHint {
    pub label: String,
    pub currency: &'static str,
    pub masked_identifier: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct DocumentDuplicateStatus {
    pub status: &'static str,
    pub matched_source_document_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ExtractorStatus {
    pub status: ExtractorState,
    pub extractor: &'static str,
    pub pages_seen: usize,
    pub pages_extracted: usize,
    pub requires_document_credential: bool,
    pub credential_source: CredentialSource,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExtractorState {
    NotRun,
    Succeeded,
    Partial,
    Failed,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ParserStatus {
    pub status: ParserState,
    pub parser_id: String,
    pub parser_version: &'static str,
    pub candidates_found: usize,
    pub candidates_valid: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ParserState {
    NotRun,
    Succeeded,
    Partial,
    Failed,
    UnsupportedDocument,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CandidateTransaction {
    pub id: String,
    pub import_batch_id: Option<String>,
    pub source_document_id: String,
    pub status: &'static str,
    pub duplicate_status: DuplicateStatus,
    pub institution_hint: String,
    pub account_hint: AccountHint,
    pub posted_date: String,
    pub description: String,
    pub amount_minor: i64,
    pub currency: &'static str,
    pub balance_minor: Option<i64>,
    pub direction_hint: &'static str,
    pub confidence: f32,
    pub provenance: Provenance,
    pub validation_warnings: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct DuplicateStatus {
    pub status: &'static str,
    pub fingerprint: String,
    pub matched_candidate_ids: Vec<String>,
    pub matched_canonical_transaction_ids: Vec<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Provenance {
    pub source_document_id: String,
    pub page_number: usize,
    pub row_index: usize,
    pub bbox: Option<BBox>,
    pub extractor: ExtractorRef,
    pub parser: ParserRef,
    pub evidence: Evidence,
    pub confidence: f32,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ExtractorRef {
    pub name: &'static str,
    pub version: Option<&'static str>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ParserRef {
    pub id: String,
    pub version: &'static str,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Evidence {
    pub redaction: &'static str,
    pub text: String,
    pub raw_storage_policy: &'static str,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TrackyError {
    pub category: &'static str,
    pub code: &'static str,
    pub message: String,
    pub path: String,
    pub recoverable: bool,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct BBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub unit: &'static str,
}

#[derive(Debug, Clone)]
pub struct ExtractedLine {
    pub page: usize,
    pub text: String,
    pub bbox: Option<BBox>,
}

#[derive(Debug, Clone)]
struct VisualRow {
    page: usize,
    row_index: usize,
    cells: Vec<ExtractedLine>,
    bbox: Option<BBox>,
}

#[derive(Debug, Clone)]
struct ParsedMovement {
    page: usize,
    row_index: usize,
    row_bbox: Option<BBox>,
    posted_date: String,
    description_sample: String,
    amount: ParsedMoney,
    balance: Option<ParsedMoney>,
    confidence: f32,
    evidence_text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedMoney {
    pub text: String,
    pub value_minor_units: Option<i64>,
    pub currency: &'static str,
}

#[derive(Debug, Clone)]
struct MoneyToken {
    money: ParsedMoney,
    x: f32,
}

pub fn inspect_pdf(path: &Path, options: InspectPdfOptions<'_>) -> Result<PdfInspectResponse> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let content_sha256 = hex_sha256(&bytes);
    let institution = options
        .institution_hint
        .unwrap_or_else(|| institution_for(path).to_string());
    let source_document = SourceDocument {
        id: source_document_id(&content_sha256),
        input_name: input_name(path),
        content_sha256,
        mime_type: "application/pdf",
        byte_size: bytes.len() as u64,
        institution_hint: institution.clone(),
        account_hint: account_hint(&institution),
        document_duplicate_status: DocumentDuplicateStatus {
            status: "unknown",
            matched_source_document_id: None,
            reason: Some("read-only pdf inspect does not perform duplicate lookup".to_string()),
        },
    };

    let extraction = extract_pdf_oxide_lines(path, options.document_credential.unwrap_or(""));
    let (extractor_status, parser_status, candidates, errors) = match extraction {
        Ok(extracted) => {
            let extractor_status = ExtractorStatus {
                status: if extracted.errors.is_empty() {
                    ExtractorState::Succeeded
                } else if extracted.lines.is_empty() {
                    ExtractorState::Failed
                } else {
                    ExtractorState::Partial
                },
                extractor: "pdf_oxide",
                pages_seen: extracted.pages_seen,
                pages_extracted: extracted.pages_extracted,
                requires_document_credential: extracted.requires_document_credential,
                credential_source: options.credential_source,
                warnings: extracted
                    .errors
                    .iter()
                    .map(|error| error.message.clone())
                    .collect(),
            };
            let mut errors = extracted.errors;
            if matches!(extractor_status.status, ExtractorState::Failed) {
                let parser_status = ParserStatus {
                    status: ParserState::NotRun,
                    parser_id: parser_id_for(&institution),
                    parser_version: "1",
                    candidates_found: 0,
                    candidates_valid: 0,
                    warnings: vec!["parser skipped because pdf_oxide extraction failed".to_string()],
                };
                (extractor_status, parser_status, Vec::new(), errors)
            } else {
                let (parser_status, candidates, parser_errors) =
                    parse_lines_for_contract(&source_document, &institution, &extracted.lines);
                errors.extend(parser_errors);
                (extractor_status, parser_status, candidates, errors)
            }
        }
        Err(error) => {
            let message = "PDF extraction failed before candidate transactions could be produced.";
            let extractor_error = TrackyError {
                category: "extractor_failure",
                code: "pdf_open_failed",
                message: message.to_string(),
                path: "extractor_status".to_string(),
                recoverable: true,
                details: serde_json::json!({
                    "extractor": "pdf_oxide",
                    "credential_required": true,
                    "cause": error.to_string(),
                }),
            };
            let extractor_status = ExtractorStatus {
                status: ExtractorState::Failed,
                extractor: "pdf_oxide",
                pages_seen: 0,
                pages_extracted: 0,
                requires_document_credential: true,
                credential_source: options.credential_source,
                warnings: vec![message.to_string()],
            };
            let parser_status = ParserStatus {
                status: ParserState::NotRun,
                parser_id: parser_id_for(&institution),
                parser_version: "1",
                candidates_found: 0,
                candidates_valid: 0,
                warnings: vec!["parser skipped because pdf_oxide extraction failed".to_string()],
            };
            (
                extractor_status,
                parser_status,
                Vec::new(),
                vec![extractor_error],
            )
        }
    };

    Ok(PdfInspectResponse {
        schema_version: PDF_INSPECT_SCHEMA_VERSION,
        command: "pdf inspect",
        ok: errors.is_empty(),
        source_document,
        extractor_status,
        parser_status,
        candidates,
        errors,
    })
}

struct ExtractedDocument {
    pages_seen: usize,
    pages_extracted: usize,
    requires_document_credential: bool,
    lines: Vec<ExtractedLine>,
    errors: Vec<TrackyError>,
}

fn extract_pdf_oxide_lines(file: &Path, credential: &str) -> Result<ExtractedDocument> {
    let doc = OxideDocument::open(file)?;
    let requires_document_credential = doc.is_encrypted();
    if requires_document_credential {
        let authenticated = doc.authenticate(credential.as_bytes())?;
        if !authenticated {
            return Err(anyhow!("pdf_oxide authentication returned false"));
        }
    }
    let pages_seen = doc.page_count()?;
    let mut lines = Vec::new();
    let mut errors = Vec::new();
    let mut pages_extracted = 0;
    for page in 0..pages_seen {
        let mut page_failed = false;
        if let Err(error) = doc
            .extract_text(page)
            .with_context(|| format!("pdf_oxide text extraction failed on page {}", page + 1))
        {
            page_failed = true;
            errors.push(page_extraction_error(
                "pdf_text_extraction_failed",
                page + 1,
                error.to_string(),
            ));
        }
        match doc
            .extract_text_lines(page)
            .with_context(|| format!("pdf_oxide line extraction failed on page {}", page + 1))
        {
            Ok(page_lines) => {
                pages_extracted += 1;
                lines.extend(page_lines.into_iter().map(|line| ExtractedLine {
                    page: page + 1,
                    text: normalize_spaces(&line.text),
                    bbox: Some(BBox {
                        x: line.bbox.x,
                        y: line.bbox.y,
                        width: line.bbox.width,
                        height: line.bbox.height,
                        unit: "pdf_point",
                    }),
                }));
            }
            Err(error) => {
                page_failed = true;
                errors.push(page_extraction_error(
                    "pdf_layout_extraction_failed",
                    page + 1,
                    error.to_string(),
                ));
            }
        }
        let _ = page_failed;
    }
    Ok(ExtractedDocument {
        pages_seen,
        pages_extracted,
        requires_document_credential,
        lines,
        errors,
    })
}

fn page_extraction_error(code: &'static str, page: usize, cause: String) -> TrackyError {
    TrackyError {
        category: "extractor_failure",
        code,
        message: format!("pdf_oxide extraction failed on page {page}"),
        path: format!("extractor_status.pages[{page}]"),
        recoverable: true,
        details: serde_json::json!({
            "extractor": "pdf_oxide",
            "page_number": page,
            "cause": cause,
        }),
    }
}

pub fn parse_lines_for_inspection(
    source_document: &SourceDocument,
    lines: &[ExtractedLine],
) -> (ParserStatus, Vec<CandidateTransaction>, Vec<TrackyError>) {
    parse_lines_for_contract(source_document, &source_document.institution_hint, lines)
}

fn parse_lines_for_contract(
    source_document: &SourceDocument,
    institution: &str,
    lines: &[ExtractedLine],
) -> (ParserStatus, Vec<CandidateTransaction>, Vec<TrackyError>) {
    let parser_id = parser_id_for(institution);
    if !matches!(institution, "nequi" | "rappi") {
        return (
            ParserStatus {
                status: ParserState::UnsupportedDocument,
                parser_id,
                parser_version: "1",
                candidates_found: 0,
                candidates_valid: 0,
                warnings: vec![format!(
                    "no deterministic parser matched institution '{institution}'"
                )],
            },
            Vec::new(),
            vec![TrackyError {
                category: "parser_failure",
                code: "unsupported_document",
                message: "No deterministic parser matched the document source.".to_string(),
                path: "parser_status".to_string(),
                recoverable: true,
                details: serde_json::json!({ "institution_hint": institution }),
            }],
        );
    }

    let rows = visual_rows(lines);
    let movements = match institution {
        "rappi" => parse_rappi_rows(&rows),
        _ => parse_nequi_rows(&rows),
    };
    let candidates = movements
        .into_iter()
        .enumerate()
        .filter_map(|(index, movement)| {
            candidate_from_movement(source_document, &parser_id, index, movement)
        })
        .collect::<Vec<_>>();
    let mut errors = Vec::new();
    let status = if candidates.is_empty() {
        errors.push(TrackyError {
            category: "parser_failure",
            code: "movement_rows_not_found",
            message: "No movement rows were parsed into transacciones candidatas.".to_string(),
            path: "parser_status".to_string(),
            recoverable: true,
            details: serde_json::json!({
                "parser_id": parser_id,
                "line_count": lines.len(),
            }),
        });
        ParserState::Failed
    } else {
        ParserState::Succeeded
    };
    (
        ParserStatus {
            status,
            parser_id,
            parser_version: "1",
            candidates_found: candidates.len(),
            candidates_valid: candidates.len(),
            warnings: Vec::new(),
        },
        candidates,
        errors,
    )
}

fn candidate_from_movement(
    source_document: &SourceDocument,
    parser_id: &str,
    index: usize,
    movement: ParsedMovement,
) -> Option<CandidateTransaction> {
    let amount_minor = movement.amount.value_minor_units?;
    let direction_hint = if amount_minor < 0 {
        "outflow"
    } else {
        "inflow"
    };
    let fingerprint = normalized_fingerprint(source_document, &movement, amount_minor);
    let candidate_id = format!(
        "cand_{}_{:04}",
        &source_document.id.replace("srcdoc_", ""),
        index + 1
    );
    Some(CandidateTransaction {
        id: candidate_id,
        import_batch_id: None,
        source_document_id: source_document.id.clone(),
        status: "pending_review",
        duplicate_status: DuplicateStatus {
            status: "not_checked",
            fingerprint,
            matched_candidate_ids: Vec::new(),
            matched_canonical_transaction_ids: Vec::new(),
            reason: None,
        },
        institution_hint: source_document.institution_hint.clone(),
        account_hint: account_hint(&source_document.institution_hint),
        posted_date: movement.posted_date,
        description: movement.description_sample,
        amount_minor,
        currency: movement.amount.currency,
        balance_minor: movement.balance.and_then(|money| money.value_minor_units),
        direction_hint,
        confidence: movement.confidence,
        provenance: Provenance {
            source_document_id: source_document.id.clone(),
            page_number: movement.page,
            row_index: movement.row_index,
            bbox: movement.row_bbox,
            extractor: ExtractorRef {
                name: "pdf_oxide",
                version: None,
            },
            parser: ParserRef {
                id: parser_id.to_string(),
                version: "1",
            },
            evidence: Evidence {
                redaction: "redacted",
                text: movement.evidence_text,
                raw_storage_policy: "redacted_only",
            },
            confidence: movement.confidence,
        },
        validation_warnings: Vec::new(),
    })
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
            let row_index = rows.len() + 1;
            rows.push(VisualRow {
                page: line.page,
                row_index,
                bbox: line.bbox,
                cells: vec![line],
            });
        }
    }
    rows
}

fn parse_nequi_rows(rows: &[VisualRow]) -> Vec<ParsedMovement> {
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
            Some(ParsedMovement {
                page: row.page,
                row_index: row.row_index,
                row_bbox: row.bbox,
                posted_date: normalize_nequi_date(&date),
                description_sample: redact_description_sample(&description),
                amount: money[0].clone(),
                balance: money.get(1).cloned(),
                confidence,
                evidence_text: redact_row_for_evidence(&row_text),
            })
        })
        .collect()
}

fn normalize_nequi_date(date: &str) -> String {
    let mut parts = date.split('/');
    let Some(day) = parts.next().and_then(|part| part.parse::<u32>().ok()) else {
        return date.to_string();
    };
    let Some(month) = parts.next().and_then(|part| part.parse::<u32>().ok()) else {
        return date.to_string();
    };
    let Some(year) = parts.next().and_then(|part| part.parse::<u32>().ok()) else {
        return date.to_string();
    };
    format!("{year:04}-{month:02}-{day:02}")
}

fn parse_rappi_rows(rows: &[VisualRow]) -> Vec<ParsedMovement> {
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
            Some(ParsedMovement {
                page: row.page,
                row_index: row.row_index,
                row_bbox: row.bbox,
                posted_date: date_match.as_str().to_string(),
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

pub fn money_tokens(text: &str) -> Vec<ParsedMoney> {
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

pub fn redact_line(text: &str) -> String {
    let email_re = Regex::new(r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b").unwrap();
    let money_re = Regex::new(
        r"(?:(?:\$|COP)\s*)-?\d+(?:[.,]\d{3})*(?:[.,]\d{1,2})?|\b-?\d{1,3}(?:[.,]\d{3})+(?:[.,]\d{1,2})?\b",
    ).unwrap();
    let long_number_re = Regex::new(r"\b\d{5,}\b").unwrap();
    let text = email_re.replace_all(text, "<email>");
    let text = money_re.replace_all(&text, "<amount>");
    let text = long_number_re.replace_all(&text, "<number>");
    let address_re = Regex::new(r"(?i)\bDirecci[oó]n\b\s+[^$<]+").unwrap();
    let card_re = Regex::new(r"(?i)(N[uú]mero de tarjeta\s+\w+)\s+\d{4}\b").unwrap();
    let holder_re =
        Regex::new(r"(?i)(Detalle de transacciones:\s*)[^()]+(\s*\(Titular\))").unwrap();
    let header_name_re =
        Regex::new(r"(?i)(Estado de cuenta\s+)[A-ZÁÉÍÓÚÑ][A-ZÁÉÍÓÚÑ ]{4,}").unwrap();
    let text = address_re.replace_all(&text, "Dirección <address>");
    let text = card_re.replace_all(&text, "$1 <card-last4>");
    let text = holder_re.replace_all(&text, "$1<cardholder>$2");
    let text = header_name_re.replace_all(&text, "$1<cardholder>");
    normalize_spaces(&redact_counterparties(&text))
}

fn redact_counterparties(text: &str) -> String {
    let counterparties = [
        Regex::new(r"(?i)(BRE-B:\s*)[^$<]+").unwrap(),
        Regex::new(r"(?i)(\bA:\s*)[^$<]+").unwrap(),
        Regex::new(r"(?i)(recibido de\s+)[^$<]+").unwrap(),
        Regex::new(r"(?i)(\bPara\s+)[^$<]+").unwrap(),
        Regex::new(r"(\bDe\s+)[^$<]+").unwrap(),
    ];
    let mut redacted = text.to_string();
    for re in counterparties {
        redacted = re.replace_all(&redacted, "$1<counterparty>").to_string();
    }
    redacted
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
        unit: "pdf_point",
    })
}

fn parser_id_for(institution: &str) -> String {
    match institution {
        "rappi" => "rappi.statement.v1".to_string(),
        "nequi" => "nequi.statement.v1".to_string(),
        other => format!("{other}.statement.v1"),
    }
}

fn account_hint(institution: &str) -> AccountHint {
    AccountHint {
        label: match institution {
            "rappi" => "Rappi card".to_string(),
            "nequi" => "Nequi wallet".to_string(),
            other => format!("{other} account"),
        },
        currency: "COP",
        masked_identifier: None,
    }
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

fn input_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn source_document_id(content_sha256: &str) -> String {
    format!("srcdoc_{}", &content_sha256[..26.min(content_sha256.len())])
}

fn normalized_fingerprint(
    source_document: &SourceDocument,
    movement: &ParsedMovement,
    amount_minor: i64,
) -> String {
    let input = format!(
        "{}|{}|{}|{}|{}",
        source_document.institution_hint,
        source_document.account_hint.label,
        movement.posted_date,
        amount_minor,
        movement.description_sample.to_lowercase()
    );
    let digest = Sha256::digest(input.as_bytes());
    digest[..16].iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bbox(x: f32, y: f32) -> Option<BBox> {
        Some(BBox {
            x,
            y,
            width: 30.0,
            height: 5.0,
            unit: "pdf_point",
        })
    }

    fn line(page: usize, text: &str, x: f32, y: f32) -> ExtractedLine {
        ExtractedLine {
            page,
            text: text.to_string(),
            bbox: bbox(x, y),
        }
    }

    fn source(institution: &str) -> SourceDocument {
        SourceDocument {
            id: "srcdoc_redactedfixture0000000000".to_string(),
            input_name: format!("{institution}-redacted.pdf"),
            content_sha256: "00".repeat(32),
            mime_type: "application/pdf",
            byte_size: 123,
            institution_hint: institution.to_string(),
            account_hint: account_hint(institution),
            document_duplicate_status: DocumentDuplicateStatus {
                status: "unknown",
                matched_source_document_id: None,
                reason: None,
            },
        }
    }

    #[test]
    fn parses_nequi_candidate_and_balance_from_redacted_visual_row() {
        let lines = vec![
            line(1, "15/05/2026", 40.0, 700.0),
            line(1, "Para PERSONA REDACTADA", 160.0, 700.0),
            line(1, "$ -45.900,00 $ 125.000,00", 370.0, 700.0),
        ];
        let (status, candidates, errors) = parse_lines_for_inspection(&source("nequi"), &lines);
        assert_eq!(status.status, ParserState::Succeeded);
        assert!(errors.is_empty());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].posted_date, "2026-05-15");
        assert_eq!(candidates[0].amount_minor, -4_590_000);
        assert_eq!(candidates[0].balance_minor, Some(12_500_000));
        assert_eq!(candidates[0].provenance.page_number, 1);
        assert_eq!(candidates[0].provenance.row_index, 1);
        assert!(candidates[0].provenance.evidence.text.contains("<amount>"));
    }

    #[test]
    fn rappi_amount_selection_uses_leftmost_non_zero_money_cell() {
        let lines = vec![
            line(1, "2026-06-01", 40.0, 500.0),
            line(1, "COMERCIO REDACTADO", 150.0, 500.0),
            line(1, "$ 0,00", 245.0, 500.0),
            line(1, "$ 32.500,00", 300.0, 500.0),
            line(1, "$ 9.900,00", 360.0, 500.0),
        ];
        let (_, candidates, errors) = parse_lines_for_inspection(&source("rappi"), &lines);
        assert!(errors.is_empty());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].amount_minor, 3_250_000);
        assert_eq!(candidates[0].balance_minor, None);
    }

    #[test]
    fn redacts_agent_visible_evidence_fields() {
        let redacted = redact_line(
            "Detalle de transacciones: JANE DOE (Titular) jane@example.com Dirección Calle 123 #45-67 Número de tarjeta virtual 1234 BRE-B: JOHN DOE $ 10.000,00 123456789",
        );
        assert!(redacted.contains("<cardholder>"));
        assert!(redacted.contains("<email>"));
        assert!(redacted.contains("Dirección <address>"));
        assert!(redacted.contains("<amount>"));
        assert!(!redacted.contains("jane@example.com"));
        assert!(!redacted.contains("123456789"));
        assert!(!redacted.contains("JOHN DOE"));
    }
}
