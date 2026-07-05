use crate::pdf::{
    inspect_pdf, supported_institution_hint_from_path, AccountHint, CredentialSource,
    DocumentDuplicateState, DocumentDuplicateStatus, ExtractorState, ExtractorStatus,
    InspectPdfOptions, ParserState, ParserStatus, PdfInspectResponse, SourceDocument, TrackyError,
    TrackyErrorCategory, TrackyErrorCode, TrackyErrorPath, PDF_INSPECT_SCHEMA_VERSION,
};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(name = "tracky", about = "Local-first finance tracker")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Pdf(PdfCommand),
}

#[derive(Debug, Parser)]
struct PdfCommand {
    #[command(subcommand)]
    command: PdfCommands,
}

#[derive(Debug, Subcommand)]
enum PdfCommands {
    Inspect(PdfInspectArgs),
}

#[derive(Debug, Parser)]
struct PdfInspectArgs {
    /// PDF document to inspect.
    #[arg(value_name = "PDF")]
    pdf: PathBuf,

    /// Name of the environment variable containing the runtime-only PDF password.
    #[arg(long, value_name = "ENV_VAR")]
    password_env: Option<String>,

    /// Emit the stable review-first JSON contract.
    #[arg(long)]
    json: bool,
}

pub fn run_from_env() -> Result<i32> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    run_with(cli, |key| env::var(key).ok(), inspect_pdf, &mut stdout)
}

fn run_with<EnvLookup, Inspector, W>(
    cli: Cli,
    env_lookup: EnvLookup,
    inspector: Inspector,
    mut stdout: W,
) -> Result<i32>
where
    EnvLookup: Fn(&str) -> Option<String>,
    Inspector: FnOnce(&Path, InspectPdfOptions<'_>) -> Result<PdfInspectResponse>,
    W: Write,
{
    match cli.command {
        Commands::Pdf(pdf) => match pdf.command {
            PdfCommands::Inspect(args) => inspect_command(args, env_lookup, inspector, &mut stdout),
        },
    }
}

fn inspect_command<EnvLookup, Inspector, W>(
    args: PdfInspectArgs,
    env_lookup: EnvLookup,
    inspector: Inspector,
    stdout: &mut W,
) -> Result<i32>
where
    EnvLookup: Fn(&str) -> Option<String>,
    Inspector: FnOnce(&Path, InspectPdfOptions<'_>) -> Result<PdfInspectResponse>,
    W: Write,
{
    if !args.json {
        return write_error_response(
            stdout,
            &args.pdf,
            CredentialSource::None,
            TrackyError {
                category: TrackyErrorCategory::ValidationFailure,
                code: TrackyErrorCode::JsonOutputRequired,
                message: "The pdf inspect command currently requires --json.".to_string(),
                path: TrackyErrorPath::Command,
                recoverable: true,
                details: serde_json::json!({ "flag": "--json" }),
            },
        );
    }

    let password = match args.password_env.as_deref() {
        Some(key) => match env_lookup(key).filter(|value| !value.is_empty()) {
            Some(value) => Some(value),
            None => {
                return write_error_response(
                    stdout,
                    &args.pdf,
                    CredentialSource::Env,
                    TrackyError {
                        category: TrackyErrorCategory::ValidationFailure,
                        code: TrackyErrorCode::MissingDocumentCredential,
                        message: "The requested password environment variable was not set."
                            .to_string(),
                        path: TrackyErrorPath::ExtractorCredentialSource,
                        recoverable: true,
                        details: serde_json::json!({ "env_var": key }),
                    },
                )
            }
        },
        None => None,
    };

    let credential_source = if password.is_some() {
        CredentialSource::Env
    } else {
        CredentialSource::None
    };
    let options = InspectPdfOptions {
        document_credential: password.as_deref(),
        credential_source,
        institution_hint: None,
    };

    match inspector(&args.pdf, options) {
        Ok(response) => {
            let exit_code = if response.ok { 0 } else { 1 };
            serde_json::to_writer(&mut *stdout, &response).context("writing pdf inspect JSON")?;
            writeln!(stdout).context("writing trailing newline")?;
            Ok(exit_code)
        }
        Err(error) => write_error_response(
            stdout,
            &args.pdf,
            credential_source,
            TrackyError {
                category: TrackyErrorCategory::ExtractorFailure,
                code: TrackyErrorCode::PdfOpenFailed,
                message: "PDF extraction failed before candidate transactions could be produced."
                    .to_string(),
                path: TrackyErrorPath::ExtractorStatus,
                recoverable: true,
                details: serde_json::json!({ "cause": error.to_string() }),
            },
        ),
    }
}

fn write_error_response<W: Write>(
    stdout: &mut W,
    path: &Path,
    credential_source: CredentialSource,
    error: TrackyError,
) -> Result<i32> {
    let institution_hint = institution_hint_for_path(path);
    let parser_id = format!("{institution_hint}.statement.v1");
    let extractor_state = if error.code == TrackyErrorCode::PdfOpenFailed {
        ExtractorState::Failed
    } else {
        ExtractorState::NotRun
    };
    let response = PdfInspectResponse {
        schema_version: PDF_INSPECT_SCHEMA_VERSION,
        command: "pdf inspect",
        ok: false,
        source_document: source_document_for_error(path, &institution_hint),
        extractor_status: ExtractorStatus {
            status: extractor_state,
            extractor: "pdf_oxide",
            pages_seen: 0,
            pages_extracted: 0,
            requires_document_credential: error.code == TrackyErrorCode::MissingDocumentCredential,
            credential_source,
            warnings: vec![error.message.clone()],
        },
        parser_status: ParserStatus {
            status: ParserState::NotRun,
            parser_id,
            parser_version: "1",
            candidates_found: 0,
            candidates_valid: 0,
            warnings: vec![
                "parser skipped because pdf inspect did not complete extraction".to_string(),
            ],
        },
        candidates: Vec::new(),
        errors: vec![error],
    };
    serde_json::to_writer(&mut *stdout, &response).context("writing pdf inspect error JSON")?;
    writeln!(stdout).context("writing trailing newline")?;
    Ok(1)
}

fn source_document_for_error(path: &Path, institution_hint: &str) -> SourceDocument {
    let bytes = fs::read(path).ok();
    let (content_sha256, byte_size) = bytes
        .as_deref()
        .map(|bytes| (hex_sha256(bytes), bytes.len() as u64))
        .unwrap_or_else(|| (String::new(), 0));
    let id = if content_sha256.is_empty() {
        "srcdoc_unavailable".to_string()
    } else {
        format!("srcdoc_{}", &content_sha256[..26.min(content_sha256.len())])
    };
    SourceDocument {
        id,
        input_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        content_sha256,
        mime_type: "application/pdf",
        byte_size,
        institution_hint: institution_hint.to_string(),
        account_hint: AccountHint {
            label: format!("{institution_hint} account"),
            currency: "COP",
            masked_identifier: None,
        },
        document_duplicate_status: DocumentDuplicateStatus {
            status: DocumentDuplicateState::Unknown,
            matched_source_document_id: None,
            reason: Some("duplicate lookup not available before successful inspection".to_string()),
        },
    }
}

fn institution_hint_for_path(path: &Path) -> String {
    supported_institution_hint_from_path(path)
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| !stem.is_empty())
                .unwrap_or("unknown")
                .to_ascii_lowercase()
        })
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::{
        AccountHint, DocumentDuplicateState, DocumentDuplicateStatus, ExtractorState,
        ExtractorStatus, ParserState, ParserStatus, SourceDocument,
    };
    use clap::Parser;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("valid CLI args")
    }

    fn successful_response(credential_source: CredentialSource) -> PdfInspectResponse {
        PdfInspectResponse {
            schema_version: PDF_INSPECT_SCHEMA_VERSION,
            command: "pdf inspect",
            ok: true,
            source_document: SourceDocument {
                id: "srcdoc_test".to_string(),
                input_name: "nequi-redacted.pdf".to_string(),
                content_sha256: "00".repeat(32),
                mime_type: "application/pdf",
                byte_size: 42,
                institution_hint: "nequi".to_string(),
                account_hint: AccountHint {
                    label: "Nequi wallet".to_string(),
                    currency: "COP",
                    masked_identifier: None,
                },
                document_duplicate_status: DocumentDuplicateStatus {
                    status: DocumentDuplicateState::Unknown,
                    matched_source_document_id: None,
                    reason: None,
                },
            },
            extractor_status: ExtractorStatus {
                status: ExtractorState::Succeeded,
                extractor: "pdf_oxide",
                pages_seen: 1,
                pages_extracted: 1,
                requires_document_credential: true,
                credential_source,
                warnings: Vec::new(),
            },
            parser_status: ParserStatus {
                status: ParserState::Succeeded,
                parser_id: "nequi.statement.v1".to_string(),
                parser_version: "1",
                candidates_found: 0,
                candidates_valid: 0,
                warnings: Vec::new(),
            },
            candidates: Vec::new(),
            errors: Vec::new(),
        }
    }

    #[test]
    fn pdf_inspect_json_uses_password_env_without_printing_secret() {
        let cli = parse(&[
            "tracky",
            "pdf",
            "inspect",
            "assets/nequi-redacted.pdf",
            "--password-env",
            "TRACKY_TEST_PDF_PASSWORD",
            "--json",
        ]);
        let mut output = Vec::new();
        let exit = run_with(
            cli,
            |key| (key == "TRACKY_TEST_PDF_PASSWORD").then(|| "super-secret".to_string()),
            |path, options| {
                assert_eq!(path, Path::new("assets/nequi-redacted.pdf"));
                assert_eq!(options.document_credential, Some("super-secret"));
                assert_eq!(options.credential_source, CredentialSource::Env);
                Ok(successful_response(options.credential_source))
            },
            &mut output,
        )
        .expect("runs CLI seam");

        assert_eq!(exit, 0);
        let json: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
        assert_eq!(json["schema_version"], "tracky.pdf-inspect.v1");
        assert_eq!(json["command"], "pdf inspect");
        assert_eq!(json["ok"], true);
        assert_eq!(json["extractor_status"]["credential_source"], "env");
        let serialized = String::from_utf8(output).expect("utf8");
        assert!(!serialized.contains("super-secret"));
        assert!(json.get("import_batch").is_none());
    }

    #[test]
    fn missing_password_env_returns_stable_json_error_without_running_inspector() {
        let cli = parse(&[
            "tracky",
            "pdf",
            "inspect",
            "assets/nequi-redacted.pdf",
            "--password-env",
            "MISSING_PASSWORD",
            "--json",
        ]);
        let mut output = Vec::new();
        let exit = run_with(
            cli,
            |_| None,
            |_path, _options| panic!("inspector should not run without requested env password"),
            &mut output,
        )
        .expect("runs CLI seam");

        assert_eq!(exit, 1);
        let json: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
        assert_eq!(json["schema_version"], "tracky.pdf-inspect.v1");
        assert_eq!(json["command"], "pdf inspect");
        assert_eq!(json["ok"], false);
        assert!(json.get("source_document").is_some());
        assert!(json.get("extractor_status").is_some());
        assert!(json.get("parser_status").is_some());
        assert_eq!(json["candidates"].as_array().unwrap().len(), 0);
        assert_eq!(json["extractor_status"]["status"], "not_run");
        assert_eq!(json["parser_status"]["status"], "not_run");
        assert_eq!(json["errors"][0]["category"], "validation_failure");
        assert_eq!(json["errors"][0]["code"], "missing_document_credential");
        assert_eq!(
            json["errors"][0]["path"],
            "extractor_status.credential_source"
        );
        assert_eq!(json["errors"][0]["details"]["env_var"], "MISSING_PASSWORD");
    }

    #[test]
    fn inspector_failure_returns_stable_json_error() {
        let cli = parse(&["tracky", "pdf", "inspect", "assets/not-a-pdf.pdf", "--json"]);
        let mut output = Vec::new();
        let exit = run_with(
            cli,
            |_| None,
            |_path, _options| Err(anyhow::anyhow!("fixture open failed")),
            &mut output,
        )
        .expect("runs CLI seam");

        assert_eq!(exit, 1);
        let json: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
        assert_eq!(json["ok"], false);
        assert!(json.get("source_document").is_some());
        assert!(json.get("extractor_status").is_some());
        assert!(json.get("parser_status").is_some());
        assert_eq!(json["candidates"].as_array().unwrap().len(), 0);
        assert_eq!(json["extractor_status"]["status"], "failed");
        assert_eq!(json["parser_status"]["status"], "not_run");
        assert_eq!(json["errors"][0]["category"], "extractor_failure");
        assert_eq!(json["errors"][0]["code"], "pdf_open_failed");
        assert_eq!(json["errors"][0]["path"], "extractor_status");
    }
}
