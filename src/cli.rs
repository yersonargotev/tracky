use crate::pdf::{
    hex_sha256, inspect_pdf, source_document_id, supported_institution_hint_from_path, AccountHint,
    CredentialSource, DocumentDuplicateState, DocumentDuplicateStatus, ExtractorState,
    ExtractorStatus, InspectPdfOptions, ParserState, ParserStatus, PdfInspectResponse,
    SourceDocument, TrackyError, TrackyErrorCategory, TrackyErrorCode, TrackyErrorPath,
    PDF_INSPECT_SCHEMA_VERSION,
};
use crate::storage::{
    accept_candidate, apply_migrations, duplicate_import_response, find_source_document_by_hash,
    list_review_candidates, persist_pdf_import, reject_candidate, review_error_response,
    CandidateReviewResponse, ImportPdfResponse, IMPORT_PDF_SCHEMA_VERSION,
};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rusqlite::Connection;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JsonCommand {
    PdfInspect,
    ImportPdf,
}

impl JsonCommand {
    fn label(self) -> &'static str {
        match self {
            Self::PdfInspect => "pdf inspect",
            Self::ImportPdf => "import pdf",
        }
    }

    fn schema_version(self) -> &'static str {
        match self {
            Self::PdfInspect => PDF_INSPECT_SCHEMA_VERSION,
            Self::ImportPdf => IMPORT_PDF_SCHEMA_VERSION,
        }
    }

    fn parser_not_run_warning(self) -> &'static str {
        match self {
            Self::PdfInspect => "parser skipped because pdf inspect did not complete extraction",
            Self::ImportPdf => "parser skipped because import pdf did not complete extraction",
        }
    }

    fn writing_error_context(self) -> &'static str {
        match self {
            Self::PdfInspect => "writing pdf inspect error JSON",
            Self::ImportPdf => "writing import pdf error JSON",
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "tracky", about = "Local-first finance tracker")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Pdf(PdfCommand),
    Import(ImportCommand),
    Candidates(CandidatesCommand),
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
struct ImportCommand {
    #[command(subcommand)]
    command: ImportCommands,
}

#[derive(Debug, Subcommand)]
enum ImportCommands {
    Pdf(ImportPdfArgs),
}

#[derive(Debug, Parser)]
struct CandidatesCommand {
    #[command(subcommand)]
    command: CandidateCommands,
}

#[derive(Debug, Subcommand)]
enum CandidateCommands {
    List(CandidateListArgs),
    Accept(CandidateActionArgs),
    Reject(CandidateActionArgs),
}

#[derive(Debug, Parser)]
struct CandidateListArgs {
    /// SQLite database path.
    #[arg(long, value_name = "PATH")]
    db: PathBuf,

    /// Import batch id to list.
    #[arg(long, value_name = "ID")]
    import_batch_id: Option<String>,

    /// Candidate status to list: pending_review, possible_duplicate, accepted, or rejected.
    #[arg(long, value_name = "STATUS")]
    status: Option<String>,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct CandidateActionArgs {
    /// Candidate transaction id.
    #[arg(value_name = "CANDIDATE_ID")]
    candidate_id: String,

    /// SQLite database path.
    #[arg(long, value_name = "PATH")]
    db: PathBuf,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct ImportPdfArgs {
    /// PDF document to import.
    #[arg(value_name = "PDF")]
    pdf: PathBuf,

    /// SQLite database path.
    #[arg(long, value_name = "PATH")]
    db: PathBuf,

    /// Name of the environment variable containing the runtime-only PDF password.
    #[arg(long, value_name = "ENV_VAR")]
    password_env: Option<String>,

    /// Emit the stable review-first JSON contract.
    #[arg(long)]
    json: bool,
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
        Commands::Import(import) => match import.command {
            ImportCommands::Pdf(args) => {
                import_pdf_command(args, env_lookup, inspector, &mut stdout)
            }
        },
        Commands::Candidates(candidates) => match candidates.command {
            CandidateCommands::List(args) => candidate_list_command(args, &mut stdout),
            CandidateCommands::Accept(args) => candidate_accept_command(args, &mut stdout),
            CandidateCommands::Reject(args) => candidate_reject_command(args, &mut stdout),
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
        return write_command_error_response(
            stdout,
            JsonCommand::PdfInspect,
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

    let Some((password, credential_source)) = runtime_password(
        args.password_env.as_deref(),
        &env_lookup,
        stdout,
        &args.pdf,
        JsonCommand::PdfInspect,
    )?
    else {
        return Ok(1);
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
        Err(error) => write_command_error_response(
            stdout,
            JsonCommand::PdfInspect,
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

fn import_pdf_command<EnvLookup, Inspector, W>(
    args: ImportPdfArgs,
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
        return write_command_error_response(
            stdout,
            JsonCommand::ImportPdf,
            &args.pdf,
            CredentialSource::None,
            TrackyError {
                category: TrackyErrorCategory::ValidationFailure,
                code: TrackyErrorCode::JsonOutputRequired,
                message: "The import pdf command currently requires --json.".to_string(),
                path: TrackyErrorPath::Command,
                recoverable: true,
                details: serde_json::json!({ "flag": "--json" }),
            },
        );
    }

    let mut connection = Connection::open(&args.db)
        .with_context(|| format!("opening SQLite database {}", args.db.display()))?;
    apply_migrations(&connection).context("applying SQLite migrations")?;

    match duplicate_response_if_imported(&connection, &args.pdf) {
        Ok(Some(duplicate_response)) => {
            serde_json::to_writer(&mut *stdout, &duplicate_response)
                .context("writing import pdf duplicate JSON")?;
            writeln!(stdout).context("writing trailing newline")?;
            return Ok(1);
        }
        Ok(None) => {}
        Err(error) => {
            return write_command_error_response(
                stdout,
                JsonCommand::ImportPdf,
                &args.pdf,
                CredentialSource::None,
                TrackyError {
                    category: TrackyErrorCategory::ExtractorFailure,
                    code: TrackyErrorCode::PdfOpenFailed,
                    message:
                        "PDF extraction failed before candidate transactions could be produced."
                            .to_string(),
                    path: TrackyErrorPath::ExtractorStatus,
                    recoverable: true,
                    details: serde_json::json!({ "cause": error.to_string() }),
                },
            )
        }
    }

    let Some((password, credential_source)) = runtime_password(
        args.password_env.as_deref(),
        &env_lookup,
        stdout,
        &args.pdf,
        JsonCommand::ImportPdf,
    )?
    else {
        return Ok(1);
    };

    let options = InspectPdfOptions {
        document_credential: password.as_deref(),
        credential_source,
        institution_hint: None,
    };
    let inspect = match inspector(&args.pdf, options) {
        Ok(response) => response,
        Err(error) => {
            return write_command_error_response(
                stdout,
                JsonCommand::ImportPdf,
                &args.pdf,
                credential_source,
                TrackyError {
                    category: TrackyErrorCategory::ExtractorFailure,
                    code: TrackyErrorCode::PdfOpenFailed,
                    message:
                        "PDF extraction failed before candidate transactions could be produced."
                            .to_string(),
                    path: TrackyErrorPath::ExtractorStatus,
                    recoverable: true,
                    details: serde_json::json!({ "cause": error.to_string() }),
                },
            )
        }
    };
    let response = persist_pdf_import(&mut connection, inspect).context("persisting pdf import")?;
    let exit_code = if response.ok { 0 } else { 1 };
    serde_json::to_writer(&mut *stdout, &response).context("writing import pdf JSON")?;
    writeln!(stdout).context("writing trailing newline")?;
    Ok(exit_code)
}

fn candidate_list_command<W>(args: CandidateListArgs, stdout: &mut W) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_candidate_json(args.json, stdout, "candidates list")? {
        return Ok(exit_code);
    }
    let connection = open_review_database(&args.db)?;
    let response = match list_review_candidates(
        &connection,
        args.import_batch_id.as_deref(),
        args.status.as_deref(),
    ) {
        Ok(candidates) => CandidateReviewResponse {
            schema_version: crate::storage::CANDIDATE_REVIEW_SCHEMA_VERSION,
            command: "candidates list",
            ok: true,
            candidate: None,
            candidates,
            canonical_transaction: None,
            errors: Vec::new(),
        },
        Err(error) => review_error_response(
            "candidates list",
            "validation_failure",
            "invalid_candidate_filter",
            "Candidate list filter is invalid.".to_string(),
            "command",
            true,
            serde_json::json!({ "cause": error.to_string() }),
        ),
    };
    write_candidate_review_response(stdout, response)
}

fn candidate_accept_command<W>(args: CandidateActionArgs, stdout: &mut W) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_candidate_json(args.json, stdout, "candidates accept")? {
        return Ok(exit_code);
    }
    let mut connection = open_review_database(&args.db)?;
    let response = accept_candidate(&mut connection, &args.candidate_id)?;
    write_candidate_review_response(stdout, response)
}

fn candidate_reject_command<W>(args: CandidateActionArgs, stdout: &mut W) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_candidate_json(args.json, stdout, "candidates reject")? {
        return Ok(exit_code);
    }
    let mut connection = open_review_database(&args.db)?;
    let response = reject_candidate(&mut connection, &args.candidate_id)?;
    write_candidate_review_response(stdout, response)
}

fn require_candidate_json<W>(
    json: bool,
    stdout: &mut W,
    command: &'static str,
) -> Result<Option<i32>>
where
    W: Write,
{
    if json {
        return Ok(None);
    }
    let exit_code = write_candidate_review_response(
        stdout,
        review_error_response(
            command,
            "validation_failure",
            "json_output_required",
            format!("The {command} command currently requires --json."),
            "command",
            true,
            serde_json::json!({ "flag": "--json" }),
        ),
    )?;
    Ok(Some(exit_code))
}

fn open_review_database(db: &Path) -> Result<Connection> {
    let connection = Connection::open(db)
        .with_context(|| format!("opening SQLite database {}", db.display()))?;
    apply_migrations(&connection).context("applying SQLite migrations")?;
    Ok(connection)
}

fn write_candidate_review_response<W: Write>(
    stdout: &mut W,
    response: CandidateReviewResponse,
) -> Result<i32> {
    let exit_code = if response.ok { 0 } else { 1 };
    serde_json::to_writer(&mut *stdout, &response).context("writing candidate review JSON")?;
    writeln!(stdout).context("writing trailing newline")?;
    Ok(exit_code)
}

fn runtime_password<EnvLookup, W>(
    password_env: Option<&str>,
    env_lookup: &EnvLookup,
    stdout: &mut W,
    pdf: &Path,
    command: JsonCommand,
) -> Result<Option<(Option<String>, CredentialSource)>>
where
    EnvLookup: Fn(&str) -> Option<String>,
    W: Write,
{
    match password_env {
        Some(key) => match env_lookup(key).filter(|value| !value.is_empty()) {
            Some(value) => Ok(Some((Some(value), CredentialSource::Env))),
            None => {
                write_command_error_response(
                    stdout,
                    command,
                    pdf,
                    CredentialSource::Env,
                    TrackyError {
                        category: TrackyErrorCategory::ValidationFailure,
                        code: TrackyErrorCode::MissingDocumentCredential,
                        message: "The requested password environment variable was not set."
                            .to_string(),
                        path: TrackyErrorPath::ExtractorCredentialSource,
                        recoverable: true,
                        details: serde_json::json!({ "env_var": key, "command": command.label() }),
                    },
                )?;
                Ok(None)
            }
        },
        None => Ok(Some((None, CredentialSource::None))),
    }
}

fn duplicate_response_if_imported(
    connection: &Connection,
    path: &Path,
) -> Result<Option<crate::storage::ImportPdfResponse>> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let content_sha256 = hex_sha256(&bytes);
    let Some(existing_id) = find_source_document_by_hash(connection, &content_sha256)? else {
        return Ok(None);
    };
    let institution_hint = institution_hint_for_path(path);
    let parser_id = format!("{institution_hint}.statement.v1");
    Ok(Some(duplicate_import_response(
        SourceDocument {
            id: source_document_id(&content_sha256),
            input_name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
            content_sha256,
            mime_type: "application/pdf",
            byte_size: bytes.len() as u64,
            institution_hint,
            account_hint: AccountHint {
                label: "duplicate source account".to_string(),
                currency: "COP",
                masked_identifier: None,
            },
            document_duplicate_status: DocumentDuplicateStatus {
                status: DocumentDuplicateState::Unknown,
                matched_source_document_id: None,
                reason: None,
            },
        },
        existing_id,
        ExtractorStatus {
            status: ExtractorState::NotRun,
            extractor: "pdf_oxide",
            pages_seen: 0,
            pages_extracted: 0,
            requires_document_credential: false,
            credential_source: CredentialSource::None,
            warnings: vec!["duplicate source document detected before extraction".to_string()],
        },
        ParserStatus {
            status: ParserState::NotRun,
            parser_id,
            parser_version: "1",
            candidates_found: 0,
            candidates_valid: 0,
            warnings: vec![
                "parser skipped because source document was already imported".to_string(),
            ],
        },
    )))
}

struct ErrorEnvelopeParts {
    source_document: SourceDocument,
    extractor_status: ExtractorStatus,
    parser_status: ParserStatus,
    errors: Vec<TrackyError>,
}

fn write_command_error_response<W: Write>(
    stdout: &mut W,
    command: JsonCommand,
    path: &Path,
    credential_source: CredentialSource,
    error: TrackyError,
) -> Result<i32> {
    let parts = error_envelope_parts(path, credential_source, error, command);
    match command {
        JsonCommand::PdfInspect => serde_json::to_writer(
            &mut *stdout,
            &PdfInspectResponse {
                schema_version: command.schema_version(),
                command: command.label(),
                ok: false,
                source_document: parts.source_document,
                extractor_status: parts.extractor_status,
                parser_status: parts.parser_status,
                candidates: Vec::new(),
                errors: parts.errors,
            },
        ),
        JsonCommand::ImportPdf => serde_json::to_writer(
            &mut *stdout,
            &ImportPdfResponse {
                schema_version: command.schema_version(),
                command: command.label(),
                ok: false,
                import_batch: None,
                source_document: parts.source_document,
                extractor_status: parts.extractor_status,
                parser_status: parts.parser_status,
                candidates: Vec::new(),
                errors: parts.errors,
            },
        ),
    }
    .with_context(|| command.writing_error_context())?;
    writeln!(stdout).context("writing trailing newline")?;
    Ok(1)
}

fn error_envelope_parts(
    path: &Path,
    credential_source: CredentialSource,
    error: TrackyError,
    command: JsonCommand,
) -> ErrorEnvelopeParts {
    let institution_hint = institution_hint_for_path(path);
    let parser_id = format!("{institution_hint}.statement.v1");
    let extractor_state = if error.code == TrackyErrorCode::PdfOpenFailed {
        ExtractorState::Failed
    } else {
        ExtractorState::NotRun
    };
    ErrorEnvelopeParts {
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
            warnings: vec![command.parser_not_run_warning().to_string()],
        },
        errors: vec![error],
    }
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
    #[test]
    fn import_pdf_json_persists_to_requested_db_without_printing_secret() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pdf_path = dir.path().join("nequi-redacted.pdf");
        let db_path = dir.path().join("tracky.sqlite");
        std::fs::write(&pdf_path, b"redacted fake pdf bytes").expect("write fake pdf");
        let pdf_arg = pdf_path.to_string_lossy().to_string();
        let db_arg = db_path.to_string_lossy().to_string();
        let cli = parse(&[
            "tracky",
            "import",
            "pdf",
            &pdf_arg,
            "--db",
            &db_arg,
            "--password-env",
            "TRACKY_TEST_PDF_PASSWORD",
            "--json",
        ]);
        let mut output = Vec::new();
        let exit = run_with(
            cli,
            |key| (key == "TRACKY_TEST_PDF_PASSWORD").then(|| "super-secret".to_string()),
            |_path, options| {
                assert_eq!(options.document_credential, Some("super-secret"));
                Ok(successful_response(options.credential_source))
            },
            &mut output,
        )
        .expect("runs import CLI seam");

        assert_eq!(exit, 0);
        let json: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
        assert_eq!(json["schema_version"], "tracky.import-pdf.v1");
        assert_eq!(json["command"], "import pdf");
        assert_eq!(json["ok"], true);
        assert!(json.get("import_batch").is_some());
        assert!(!String::from_utf8(output).unwrap().contains("super-secret"));

        let connection = rusqlite::Connection::open(db_path).expect("open db");
        let canonical_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM canonical_transactions", [], |row| {
                row.get(0)
            })
            .expect("count canonical");
        let batch_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM import_batches", [], |row| row.get(0))
            .expect("count batches");
        assert_eq!(canonical_count, 0);
        assert_eq!(batch_count, 1);
    }
    #[test]
    fn import_pdf_duplicate_precheck_runs_before_missing_password_env() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pdf_path = dir.path().join("nequi-redacted.pdf");
        let db_path = dir.path().join("tracky.sqlite");
        let pdf_bytes = b"redacted duplicate fake pdf bytes";
        std::fs::write(&pdf_path, pdf_bytes).expect("write fake pdf");
        let content_sha256 = crate::pdf::hex_sha256(pdf_bytes);
        let connection = rusqlite::Connection::open(&db_path).expect("open db");
        crate::storage::apply_migrations(&connection).expect("apply migrations");
        connection
            .execute(
                "INSERT INTO source_documents (id, input_name, content_sha256, mime_type, byte_size, institution_hint)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    "srcdoc_existing",
                    "nequi-redacted.pdf",
                    content_sha256,
                    "application/pdf",
                    pdf_bytes.len() as i64,
                    "nequi"
                ],
            )
            .expect("seed duplicate source document");
        drop(connection);

        let pdf_arg = pdf_path.to_string_lossy().to_string();
        let db_arg = db_path.to_string_lossy().to_string();
        let cli = parse(&[
            "tracky",
            "import",
            "pdf",
            &pdf_arg,
            "--db",
            &db_arg,
            "--password-env",
            "MISSING_PASSWORD",
            "--json",
        ]);
        let mut output = Vec::new();
        let exit = run_with(
            cli,
            |_| None,
            |_path, _options| panic!("inspector should not run for duplicate source document"),
            &mut output,
        )
        .expect("runs import CLI seam");

        assert_eq!(exit, 1);
        let json: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
        assert_eq!(json["schema_version"], "tracky.import-pdf.v1");
        assert_eq!(json["command"], "import pdf");
        assert_eq!(json["ok"], false);
        assert!(json.get("import_batch").is_none());
        assert_eq!(
            json["source_document"]["document_duplicate_status"]["status"],
            "duplicate_source_document"
        );
        assert_eq!(json["errors"][0]["code"], "duplicate_source_document");
        assert_eq!(
            json["errors"][0]["details"]["reason"],
            "source_document_already_imported"
        );
    }
    #[test]
    fn import_pdf_unreadable_path_returns_stable_json_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pdf_path = dir.path().join("does-not-exist.pdf");
        let db_path = dir.path().join("tracky.sqlite");
        let pdf_arg = pdf_path.to_string_lossy().to_string();
        let db_arg = db_path.to_string_lossy().to_string();
        let cli = parse(&[
            "tracky", "import", "pdf", &pdf_arg, "--db", &db_arg, "--json",
        ]);
        let mut output = Vec::new();
        let exit = run_with(
            cli,
            |_| None,
            |_path, _options| panic!("inspector should not run when source cannot be read"),
            &mut output,
        )
        .expect("runs import CLI seam");

        assert_eq!(exit, 1);
        let json: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
        assert_eq!(json["schema_version"], "tracky.import-pdf.v1");
        assert_eq!(json["command"], "import pdf");
        assert_eq!(json["ok"], false);
        assert_eq!(json["extractor_status"]["status"], "failed");
        assert_eq!(json["parser_status"]["status"], "not_run");
        assert_eq!(json["errors"][0]["category"], "extractor_failure");
        assert_eq!(json["errors"][0]["code"], "pdf_open_failed");
        assert_eq!(json["errors"][0]["path"], "extractor_status");
    }
}
