use crate::pdf::{
    hex_sha256, inspect_pdf, source_document_id, supported_institution_hint_from_path, AccountHint,
    CredentialSource, DocumentDuplicateState, DocumentDuplicateStatus, ExtractorState,
    ExtractorStatus, InspectPdfOptions, ParserState, ParserStatus, PdfInspectResponse,
    SourceDocument, TrackyError, TrackyErrorCategory, TrackyErrorCode, TrackyErrorPath,
    PDF_INSPECT_SCHEMA_VERSION,
};
use crate::storage::{
    accept_candidate, accept_expense_candidate, accept_income_candidate,
    accept_investment_candidate, accept_transfer_pair, account_registry_error_response,
    apply_batch_actions, apply_migrations, batch_review_error_response,
    batch_review_error_response_with_dry_run, category_registry_error_response,
    compare_duplicate_candidate, create_category, create_income_source, create_manual_expense,
    create_manual_income, create_manual_investment, create_manual_transfer,
    duplicate_import_response, finance_report_error_response, find_source_document_by_hash,
    income_source_registry_error_response, inspect_canonical_transaction,
    list_canonical_transactions, list_categories, list_income_sources, list_likely_transfer_pairs,
    list_owned_accounts, list_review_candidates, persist_pdf_import, register_owned_account,
    reject_candidate, replace_expense_transaction_lines, review_error_response,
    suggest_batch_actions, summarize_finances, summarize_import_batch, transfer_error_response,
    update_canonical_transaction, AccountRegisterInput, AccountRegistryResponse,
    BatchActionRequest, BatchReviewResponse, CandidateReviewResponse, CategoryCreateInput,
    CategoryRegistryResponse, ExpenseLineInput, FinanceReportResponse, ImportPdfResponse,
    IncomeSourceCreateInput, IncomeSourceRegistryResponse, ManualExpenseInput, ManualIncomeInput,
    ManualInvestmentInput, ManualTransactionResponse, ManualTransferInput,
    TransactionLedgerResponse, TransactionListFilter, TransactionUpdateInput,
    TransferReviewResponse, IMPORT_PDF_SCHEMA_VERSION,
};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rusqlite::{Connection, OpenFlags};
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
    Accounts(AccountsCommand),
    IncomeSources(IncomeSourcesCommand),
    Categories(CategoriesCommand),
    Transactions(TransactionsCommand),
    Reports(ReportsCommand),
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

#[derive(Debug, Parser)]
struct AccountsCommand {
    #[command(subcommand)]
    command: AccountCommands,
}

#[derive(Debug, Parser)]
struct IncomeSourcesCommand {
    #[command(subcommand)]
    command: IncomeSourceCommands,
}

#[derive(Debug, Parser)]
struct CategoriesCommand {
    #[command(subcommand)]
    command: CategoryCommands,
}

#[derive(Debug, Parser)]
struct TransactionsCommand {
    #[command(subcommand)]
    command: TransactionCommands,
}

#[derive(Debug, Parser)]
struct ReportsCommand {
    #[command(subcommand)]
    command: ReportCommands,
}

#[derive(Debug, Subcommand)]
enum AccountCommands {
    Register(AccountRegisterArgs),
    List(AccountListArgs),
}

#[derive(Debug, Subcommand)]
enum CandidateCommands {
    List(CandidateListArgs),
    BatchSummary(CandidateBatchSummaryArgs),
    CompareDuplicate(CandidateCompareDuplicateArgs),
    SuggestActions(CandidateSuggestActionsArgs),
    ApplyActions(CandidateApplyActionsArgs),
    Accept(CandidateActionArgs),
    AcceptIncome(CandidateIncomeAcceptArgs),
    AcceptExpense(CandidateExpenseAcceptArgs),
    AcceptInvestment(CandidateActionArgs),
    SetExpenseLines(CandidateExpenseLinesArgs),
    Reject(CandidateActionArgs),
    ListTransferPairs(CandidateTransferListArgs),
    AcceptTransferPair(CandidateTransferAcceptArgs),
}

#[derive(Debug, Subcommand)]
enum IncomeSourceCommands {
    Create(IncomeSourceCreateArgs),
    List(IncomeSourceListArgs),
}

#[derive(Debug, Subcommand)]
enum CategoryCommands {
    Create(CategoryCreateArgs),
    List(CategoryListArgs),
}

#[derive(Debug, Subcommand)]
enum TransactionCommands {
    AddExpense(ManualExpenseArgs),
    AddIncome(ManualIncomeArgs),
    AddInvestment(ManualInvestmentArgs),
    AddTransfer(ManualTransferArgs),
    List(TransactionListArgs),
    Inspect(TransactionInspectArgs),
    Update(TransactionUpdateArgs),
}

#[derive(Debug, Subcommand)]
enum ReportCommands {
    Summary(FinanceReportArgs),
}

#[derive(Debug, Parser)]
struct FinanceReportArgs {
    #[arg(long, value_name = "PATH")]
    db: PathBuf,
    #[arg(long = "start-date", value_name = "YYYY-MM-DD")]
    start_date: String,
    #[arg(long = "end-date", value_name = "YYYY-MM-DD")]
    end_date: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct TransactionListArgs {
    #[arg(long, value_name = "PATH")]
    db: PathBuf,
    #[arg(long = "start-date", value_name = "YYYY-MM-DD")]
    start_date: Option<String>,
    #[arg(long = "end-date", value_name = "YYYY-MM-DD")]
    end_date: Option<String>,
    #[arg(long = "account-id", value_name = "ID")]
    account_id: Option<String>,
    #[arg(long = "category-id", value_name = "ID")]
    category_id: Option<String>,
    #[arg(long = "income-source-id", value_name = "ID")]
    income_source_id: Option<String>,
    #[arg(long = "type", value_name = "TYPE")]
    transaction_kind: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct TransactionInspectArgs {
    #[arg(value_name = "TRANSACTION_ID")]
    transaction_id: String,
    #[arg(long, value_name = "PATH")]
    db: PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct TransactionUpdateArgs {
    #[arg(value_name = "TRANSACTION_ID")]
    transaction_id: String,
    #[arg(long, value_name = "PATH")]
    db: PathBuf,
    #[arg(long, value_name = "TEXT")]
    description: Option<String>,
    #[arg(long = "category-id", value_name = "ID")]
    category_id: Option<String>,
    #[arg(long = "line", value_name = "CATEGORY_ID:AMOUNT_MINOR:CURRENCY")]
    lines: Vec<String>,
    #[arg(long = "income-source-id", value_name = "ID")]
    income_source_id: Option<String>,
    #[arg(long = "income-kind", value_name = "KIND")]
    income_kind: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct ManualExpenseArgs {
    #[arg(long, value_name = "PATH")]
    db: PathBuf,
    #[arg(long = "account-id", value_name = "ID")]
    account_id: String,
    #[arg(long = "posted-date", value_name = "YYYY-MM-DD")]
    posted_date: String,
    #[arg(long, value_name = "TEXT")]
    description: String,
    #[arg(
        long = "amount-minor",
        value_name = "AMOUNT",
        allow_hyphen_values = true
    )]
    amount_minor: i64,
    #[arg(long, value_name = "CURRENCY")]
    currency: String,
    #[arg(long = "category-id", value_name = "ID")]
    category_id: Option<String>,
    #[arg(long = "line", value_name = "CATEGORY_ID:AMOUNT_MINOR:CURRENCY")]
    lines: Vec<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct ManualIncomeArgs {
    #[arg(long, value_name = "PATH")]
    db: PathBuf,
    #[arg(long = "account-id", value_name = "ID")]
    account_id: String,
    #[arg(long = "posted-date", value_name = "YYYY-MM-DD")]
    posted_date: String,
    #[arg(long, value_name = "TEXT")]
    description: String,
    #[arg(
        long = "amount-minor",
        value_name = "AMOUNT",
        allow_hyphen_values = true
    )]
    amount_minor: i64,
    #[arg(long, value_name = "CURRENCY")]
    currency: String,
    #[arg(long = "income-source-id", value_name = "ID")]
    income_source_id: String,
    #[arg(long = "income-kind", value_name = "KIND")]
    income_kind: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct ManualInvestmentArgs {
    #[arg(long, value_name = "PATH")]
    db: PathBuf,
    #[arg(long = "account-id", value_name = "ID")]
    account_id: String,
    #[arg(long = "posted-date", value_name = "YYYY-MM-DD")]
    posted_date: String,
    #[arg(long, value_name = "TEXT")]
    description: String,
    #[arg(
        long = "amount-minor",
        value_name = "AMOUNT",
        allow_hyphen_values = true
    )]
    amount_minor: i64,
    #[arg(long, value_name = "CURRENCY")]
    currency: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct ManualTransferArgs {
    #[arg(long, value_name = "PATH")]
    db: PathBuf,
    #[arg(long = "from-account-id", value_name = "ID")]
    from_account_id: String,
    #[arg(long = "to-account-id", value_name = "ID")]
    to_account_id: String,
    #[arg(long = "posted-date", value_name = "YYYY-MM-DD")]
    posted_date: String,
    #[arg(long, value_name = "TEXT")]
    description: String,
    #[arg(
        long = "amount-minor",
        value_name = "AMOUNT",
        allow_hyphen_values = true
    )]
    amount_minor: i64,
    #[arg(long, value_name = "CURRENCY")]
    currency: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct AccountRegisterArgs {
    /// SQLite database path.
    #[arg(long, value_name = "PATH")]
    db: PathBuf,

    /// Institution name or stable hint, such as nequi or rappi.
    #[arg(long, value_name = "NAME")]
    institution: String,

    /// User-facing account label, such as Nequi wallet or RappiCard.
    #[arg(long, value_name = "LABEL")]
    label: String,

    /// Account type, such as wallet, checking, credit_card, or card.
    #[arg(long = "account-type", value_name = "TYPE")]
    account_type: String,

    /// ISO-like currency code.
    #[arg(long, value_name = "CURRENCY")]
    currency: String,

    /// Optional masked identifier; never pass a full account number.
    #[arg(long, value_name = "MASKED")]
    masked_identifier: Option<String>,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct AccountListArgs {
    /// SQLite database path.
    #[arg(long, value_name = "PATH")]
    db: PathBuf,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct IncomeSourceCreateArgs {
    /// SQLite database path.
    #[arg(long, value_name = "PATH")]
    db: PathBuf,

    /// Stable human name for the income source, such as an employer or client.
    #[arg(long, value_name = "NAME")]
    name: String,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct IncomeSourceListArgs {
    /// SQLite database path.
    #[arg(long, value_name = "PATH")]
    db: PathBuf,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct CategoryCreateArgs {
    /// SQLite database path.
    #[arg(long, value_name = "PATH")]
    db: PathBuf,

    /// Stable human name for the expense category.
    #[arg(long, value_name = "NAME")]
    name: String,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct CategoryListArgs {
    /// SQLite database path.
    #[arg(long, value_name = "PATH")]
    db: PathBuf,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
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
struct CandidateBatchSummaryArgs {
    #[arg(long, value_name = "PATH")]
    db: PathBuf,
    #[arg(long = "import-batch-id", value_name = "ID")]
    import_batch_id: String,
    #[arg(long = "largest-limit", value_name = "COUNT", default_value_t = 10)]
    largest_limit: usize,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct CandidateCompareDuplicateArgs {
    #[arg(value_name = "CANDIDATE_ID")]
    candidate_id: String,
    #[arg(long, value_name = "PATH")]
    db: PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct CandidateSuggestActionsArgs {
    #[arg(long, value_name = "PATH")]
    db: PathBuf,
    #[arg(long = "import-batch-id", value_name = "ID")]
    import_batch_id: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct CandidateApplyActionsArgs {
    #[arg(long, value_name = "PATH")]
    db: PathBuf,
    #[arg(long = "action", value_name = "ACTION")]
    actions: Vec<String>,
    #[arg(long = "dry-run")]
    dry_run: bool,
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
struct CandidateIncomeAcceptArgs {
    /// Candidate transaction id.
    #[arg(value_name = "CANDIDATE_ID")]
    candidate_id: String,

    /// SQLite database path.
    #[arg(long, value_name = "PATH")]
    db: PathBuf,

    /// Income source id created via `income-sources create`.
    #[arg(long = "income-source-id", value_name = "ID")]
    income_source_id: String,

    /// Explicit income kind: salary, freelance, client_payment, sale, interest, reimbursement, or other.
    #[arg(long = "income-kind", value_name = "KIND")]
    income_kind: String,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct CandidateExpenseAcceptArgs {
    /// Candidate transaction id.
    #[arg(value_name = "CANDIDATE_ID")]
    candidate_id: String,

    /// SQLite database path.
    #[arg(long, value_name = "PATH")]
    db: PathBuf,

    /// Expense category id created via `categories create`. Omit when using --line.
    #[arg(long = "category-id", value_name = "ID")]
    category_id: Option<String>,

    /// Categorized expense line as CATEGORY_ID:AMOUNT_MINOR:CURRENCY. Repeat for a split.
    #[arg(long = "line", value_name = "CATEGORY_ID:AMOUNT_MINOR:CURRENCY")]
    lines: Vec<String>,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct CandidateExpenseLinesArgs {
    /// Accepted expense candidate transaction id.
    #[arg(value_name = "CANDIDATE_ID")]
    candidate_id: String,

    /// SQLite database path.
    #[arg(long, value_name = "PATH")]
    db: PathBuf,

    /// Categorized expense line as CATEGORY_ID:AMOUNT_MINOR:CURRENCY. Repeat for a split.
    #[arg(
        long = "line",
        required = true,
        value_name = "CATEGORY_ID:AMOUNT_MINOR:CURRENCY"
    )]
    lines: Vec<String>,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct CandidateTransferListArgs {
    /// SQLite database path.
    #[arg(long, value_name = "PATH")]
    db: PathBuf,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct CandidateTransferAcceptArgs {
    /// Paying/outflow candidate transaction id, such as the Nequi PSE outflow.
    #[arg(value_name = "FROM_CANDIDATE_ID")]
    from_candidate_id: String,

    /// Card-payment candidate transaction id, such as the RappiCard PSE payment.
    #[arg(value_name = "TO_CANDIDATE_ID")]
    to_candidate_id: String,

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
            CandidateCommands::BatchSummary(args) => {
                candidate_batch_summary_command(args, &mut stdout)
            }
            CandidateCommands::CompareDuplicate(args) => {
                candidate_compare_duplicate_command(args, &mut stdout)
            }
            CandidateCommands::SuggestActions(args) => {
                candidate_suggest_actions_command(args, &mut stdout)
            }
            CandidateCommands::ApplyActions(args) => {
                candidate_apply_actions_command(args, &mut stdout)
            }
            CandidateCommands::Accept(args) => candidate_accept_command(args, &mut stdout),
            CandidateCommands::AcceptIncome(args) => {
                candidate_accept_income_command(args, &mut stdout)
            }
            CandidateCommands::AcceptExpense(args) => {
                candidate_accept_expense_command(args, &mut stdout)
            }
            CandidateCommands::AcceptInvestment(args) => {
                candidate_accept_investment_command(args, &mut stdout)
            }
            CandidateCommands::SetExpenseLines(args) => {
                candidate_set_expense_lines_command(args, &mut stdout)
            }
            CandidateCommands::Reject(args) => candidate_reject_command(args, &mut stdout),
            CandidateCommands::ListTransferPairs(args) => {
                candidate_list_transfer_pairs_command(args, &mut stdout)
            }
            CandidateCommands::AcceptTransferPair(args) => {
                candidate_accept_transfer_pair_command(args, &mut stdout)
            }
        },
        Commands::Accounts(accounts) => match accounts.command {
            AccountCommands::Register(args) => account_register_command(args, &mut stdout),
            AccountCommands::List(args) => account_list_command(args, &mut stdout),
        },
        Commands::IncomeSources(income_sources) => match income_sources.command {
            IncomeSourceCommands::Create(args) => income_source_create_command(args, &mut stdout),
            IncomeSourceCommands::List(args) => income_source_list_command(args, &mut stdout),
        },
        Commands::Categories(categories) => match categories.command {
            CategoryCommands::Create(args) => category_create_command(args, &mut stdout),
            CategoryCommands::List(args) => category_list_command(args, &mut stdout),
        },
        Commands::Transactions(transactions) => match transactions.command {
            TransactionCommands::AddExpense(args) => manual_expense_command(args, &mut stdout),
            TransactionCommands::AddIncome(args) => manual_income_command(args, &mut stdout),
            TransactionCommands::AddInvestment(args) => {
                manual_investment_command(args, &mut stdout)
            }
            TransactionCommands::AddTransfer(args) => manual_transfer_command(args, &mut stdout),
            TransactionCommands::List(args) => transaction_list_command(args, &mut stdout),
            TransactionCommands::Inspect(args) => transaction_inspect_command(args, &mut stdout),
            TransactionCommands::Update(args) => transaction_update_command(args, &mut stdout),
        },
        Commands::Reports(reports) => match reports.command {
            ReportCommands::Summary(args) => finance_report_command(args, &mut stdout),
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

fn account_register_command<W>(args: AccountRegisterArgs, stdout: &mut W) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_account_json(args.json, stdout, "accounts register")? {
        return Ok(exit_code);
    }
    let connection = open_review_database(&args.db)?;
    let response = match register_owned_account(
        &connection,
        AccountRegisterInput {
            institution: args.institution,
            label: args.label,
            account_type: args.account_type,
            currency: args.currency,
            masked_identifier: args.masked_identifier,
        },
    ) {
        Ok(response) => response,
        Err(error) => account_registry_error_response(
            "accounts register",
            "validation_failure",
            "invalid_account_registration",
            "Owned account registration is invalid.".to_string(),
            "command",
            true,
            serde_json::json!({ "cause": error.to_string() }),
        ),
    };
    write_account_registry_response(stdout, response)
}

fn account_list_command<W>(args: AccountListArgs, stdout: &mut W) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_account_json(args.json, stdout, "accounts list")? {
        return Ok(exit_code);
    }
    let connection = open_review_database(&args.db)?;
    let response = list_owned_accounts(&connection)?;
    write_account_registry_response(stdout, response)
}

fn income_source_create_command<W>(args: IncomeSourceCreateArgs, stdout: &mut W) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_income_source_json(args.json, stdout, "income-sources create")?
    {
        return Ok(exit_code);
    }
    let connection = open_review_database(&args.db)?;
    let response =
        match create_income_source(&connection, IncomeSourceCreateInput { name: args.name }) {
            Ok(response) => response,
            Err(error) => income_source_registry_error_response(
                "income-sources create",
                "validation_failure",
                "invalid_income_source",
                "Income source is invalid.".to_string(),
                "command",
                true,
                serde_json::json!({ "cause": error.to_string() }),
            ),
        };
    write_income_source_registry_response(stdout, response)
}

fn income_source_list_command<W>(args: IncomeSourceListArgs, stdout: &mut W) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_income_source_json(args.json, stdout, "income-sources list")? {
        return Ok(exit_code);
    }
    let connection = open_review_database(&args.db)?;
    let response = list_income_sources(&connection)?;
    write_income_source_registry_response(stdout, response)
}

fn category_create_command<W>(args: CategoryCreateArgs, stdout: &mut W) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_category_json(args.json, stdout, "categories create")? {
        return Ok(exit_code);
    }
    let connection = open_review_database(&args.db)?;
    let response = match create_category(&connection, CategoryCreateInput { name: args.name }) {
        Ok(response) => response,
        Err(error) => category_registry_error_response(
            "categories create",
            "validation_failure",
            "invalid_category",
            "Category is invalid.".to_string(),
            "command",
            true,
            serde_json::json!({ "cause": error.to_string() }),
        ),
    };
    write_category_registry_response(stdout, response)
}

fn category_list_command<W>(args: CategoryListArgs, stdout: &mut W) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_category_json(args.json, stdout, "categories list")? {
        return Ok(exit_code);
    }
    let connection = open_review_database(&args.db)?;
    let response = list_categories(&connection)?;
    write_category_registry_response(stdout, response)
}

fn manual_expense_command<W>(args: ManualExpenseArgs, stdout: &mut W) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_manual_json(args.json, stdout, "transactions add-expense")? {
        return Ok(exit_code);
    }
    let lines =
        match expense_lines_from_args(args.category_id, args.lines, "transactions add-expense") {
            Ok(lines) => lines,
            Err(response) => {
                return write_manual_transaction_response(
                    stdout,
                    manual_from_candidate_response(*response),
                )
            }
        };
    let mut connection = open_review_database(&args.db)?;
    let response = create_manual_expense(
        &mut connection,
        ManualExpenseInput {
            account_id: args.account_id,
            posted_date: args.posted_date,
            description: args.description,
            amount_minor: args.amount_minor,
            currency: args.currency,
            lines,
        },
    )?;
    write_manual_transaction_response(stdout, response)
}

fn manual_income_command<W>(args: ManualIncomeArgs, stdout: &mut W) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_manual_json(args.json, stdout, "transactions add-income")? {
        return Ok(exit_code);
    }
    let mut connection = open_review_database(&args.db)?;
    let response = create_manual_income(
        &mut connection,
        ManualIncomeInput {
            account_id: args.account_id,
            posted_date: args.posted_date,
            description: args.description,
            amount_minor: args.amount_minor,
            currency: args.currency,
            income_source_id: args.income_source_id,
            income_kind: args.income_kind,
        },
    )?;
    write_manual_transaction_response(stdout, response)
}

fn manual_investment_command<W>(args: ManualInvestmentArgs, stdout: &mut W) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_manual_json(args.json, stdout, "transactions add-investment")?
    {
        return Ok(exit_code);
    }
    let mut connection = open_review_database(&args.db)?;
    let response = create_manual_investment(
        &mut connection,
        ManualInvestmentInput {
            account_id: args.account_id,
            posted_date: args.posted_date,
            description: args.description,
            amount_minor: args.amount_minor,
            currency: args.currency,
        },
    )?;
    write_manual_transaction_response(stdout, response)
}

fn manual_transfer_command<W>(args: ManualTransferArgs, stdout: &mut W) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_manual_json(args.json, stdout, "transactions add-transfer")? {
        return Ok(exit_code);
    }
    let mut connection = open_review_database(&args.db)?;
    let response = create_manual_transfer(
        &mut connection,
        ManualTransferInput {
            from_account_id: args.from_account_id,
            to_account_id: args.to_account_id,
            posted_date: args.posted_date,
            description: args.description,
            amount_minor: args.amount_minor,
            currency: args.currency,
        },
    )?;
    write_manual_transaction_response(stdout, response)
}

fn transaction_list_command<W: Write>(args: TransactionListArgs, stdout: &mut W) -> Result<i32> {
    if let Some(exit_code) = require_transaction_json(args.json, stdout, "transactions list")? {
        return Ok(exit_code);
    }
    let connection = open_review_database(&args.db)?;
    let response = list_canonical_transactions(
        &connection,
        TransactionListFilter {
            start_date: args.start_date.as_deref(),
            end_date: args.end_date.as_deref(),
            account_id: args.account_id.as_deref(),
            category_id: args.category_id.as_deref(),
            income_source_id: args.income_source_id.as_deref(),
            transaction_kind: args.transaction_kind.as_deref(),
        },
    )?;
    write_transaction_ledger_response(stdout, response)
}

fn transaction_inspect_command<W: Write>(
    args: TransactionInspectArgs,
    stdout: &mut W,
) -> Result<i32> {
    if let Some(exit_code) = require_transaction_json(args.json, stdout, "transactions inspect")? {
        return Ok(exit_code);
    }
    let connection = open_review_database(&args.db)?;
    write_transaction_ledger_response(
        stdout,
        inspect_canonical_transaction(&connection, &args.transaction_id)?,
    )
}

fn transaction_update_command<W: Write>(
    args: TransactionUpdateArgs,
    stdout: &mut W,
) -> Result<i32> {
    if let Some(exit_code) = require_transaction_json(args.json, stdout, "transactions update")? {
        return Ok(exit_code);
    }
    let expense_lines = if args.category_id.is_some() || !args.lines.is_empty() {
        match expense_lines_from_args(args.category_id, args.lines, "transactions update") {
            Ok(lines) => Some(lines),
            Err(response) => {
                return write_transaction_ledger_response(
                    stdout,
                    transaction_ledger_from_candidate_response(*response),
                )
            }
        }
    } else {
        None
    };
    if args.description.is_none()
        && args.income_source_id.is_none()
        && args.income_kind.is_none()
        && expense_lines.is_none()
    {
        return write_transaction_ledger_response(
            stdout,
            transaction_ledger_cli_error(
                "transactions update",
                "update_fields_required",
                "Provide at least one supported update field.",
            ),
        );
    }
    let mut connection = open_review_database(&args.db)?;
    write_transaction_ledger_response(
        stdout,
        update_canonical_transaction(
            &mut connection,
            &args.transaction_id,
            TransactionUpdateInput {
                description: args.description,
                income_source_id: args.income_source_id,
                income_kind: args.income_kind,
                expense_lines,
            },
        )?,
    )
}

fn finance_report_command<W: Write>(args: FinanceReportArgs, stdout: &mut W) -> Result<i32> {
    if !args.json {
        return write_finance_report_response(
            stdout,
            finance_report_error_response(
                args.start_date,
                args.end_date,
                "json_output_required",
                "The reports summary command currently requires --json.",
                "command",
            ),
        );
    }
    let connection = open_review_database(&args.db)?;
    write_finance_report_response(
        stdout,
        summarize_finances(&connection, &args.start_date, &args.end_date)?,
    )
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
            transaction_lines: Vec::new(),
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

fn candidate_batch_summary_command<W: Write>(
    args: CandidateBatchSummaryArgs,
    stdout: &mut W,
) -> Result<i32> {
    if let Some(exit_code) = require_batch_json(
        args.json,
        stdout,
        "candidates batch-summary",
        Some(&args.import_batch_id),
    )? {
        return Ok(exit_code);
    }
    let connection = match open_readonly_database(&args.db) {
        Ok(connection) => connection,
        Err(_) => {
            return write_batch_review_response(
                stdout,
                database_open_batch_error("candidates batch-summary", Some(&args.import_batch_id)),
            )
        }
    };
    let response = summarize_import_batch(&connection, &args.import_batch_id, args.largest_limit)
        .unwrap_or_else(|_| {
            database_operation_batch_error(
                "candidates batch-summary",
                Some(&args.import_batch_id),
                None,
            )
        });
    write_batch_review_response(stdout, response)
}

fn candidate_compare_duplicate_command<W: Write>(
    args: CandidateCompareDuplicateArgs,
    stdout: &mut W,
) -> Result<i32> {
    if let Some(exit_code) =
        require_batch_json(args.json, stdout, "candidates compare-duplicate", None)?
    {
        return Ok(exit_code);
    }
    let connection = match open_readonly_database(&args.db) {
        Ok(connection) => connection,
        Err(_) => {
            return write_batch_review_response(
                stdout,
                database_open_batch_error("candidates compare-duplicate", None),
            )
        }
    };
    let response =
        compare_duplicate_candidate(&connection, &args.candidate_id).unwrap_or_else(|_| {
            database_operation_batch_error("candidates compare-duplicate", None, None)
        });
    write_batch_review_response(stdout, response)
}

fn candidate_suggest_actions_command<W: Write>(
    args: CandidateSuggestActionsArgs,
    stdout: &mut W,
) -> Result<i32> {
    if let Some(exit_code) = require_batch_json(
        args.json,
        stdout,
        "candidates suggest-actions",
        Some(&args.import_batch_id),
    )? {
        return Ok(exit_code);
    }
    let connection = match open_readonly_database(&args.db) {
        Ok(connection) => connection,
        Err(_) => {
            return write_batch_review_response(
                stdout,
                database_open_batch_error(
                    "candidates suggest-actions",
                    Some(&args.import_batch_id),
                ),
            )
        }
    };
    let response = suggest_batch_actions(&connection, &args.import_batch_id).unwrap_or_else(|_| {
        database_operation_batch_error(
            "candidates suggest-actions",
            Some(&args.import_batch_id),
            None,
        )
    });
    write_batch_review_response(stdout, response)
}

fn candidate_apply_actions_command<W: Write>(
    args: CandidateApplyActionsArgs,
    stdout: &mut W,
) -> Result<i32> {
    if !args.json {
        return write_batch_review_response(
            stdout,
            batch_review_error_response_with_dry_run(
                "candidates apply-actions",
                "validation_failure",
                "json_output_required",
                "The candidates apply-actions command currently requires --json.",
                "command",
                serde_json::json!({ "flag": "--json" }),
                args.dry_run,
            ),
        );
    }
    let actions = match parse_batch_actions(&args.actions, args.dry_run) {
        Ok(actions) => actions,
        Err(response) => return write_batch_review_response(stdout, *response),
    };
    let mut connection = if args.dry_run {
        match open_readonly_database(&args.db) {
            Ok(connection) => connection,
            Err(_) => {
                return write_batch_review_response(
                    stdout,
                    database_open_batch_error_with_dry_run(args.dry_run),
                )
            }
        }
    } else {
        match open_review_database(&args.db) {
            Ok(connection) => connection,
            Err(_) => {
                return write_batch_review_response(
                    stdout,
                    database_open_batch_error_with_dry_run(args.dry_run),
                )
            }
        }
    };
    let response =
        apply_batch_actions(&mut connection, &actions, args.dry_run).unwrap_or_else(|_| {
            database_operation_batch_error("candidates apply-actions", None, Some(args.dry_run))
        });
    write_batch_review_response(stdout, response)
}

fn parse_batch_actions(
    raw_actions: &[String],
    dry_run: bool,
) -> Result<Vec<BatchActionRequest>, Box<BatchReviewResponse>> {
    raw_actions
        .iter()
        .map(|raw_action| parse_batch_action(raw_action, dry_run))
        .collect()
}

fn parse_batch_action(
    raw_action: &str,
    dry_run: bool,
) -> Result<BatchActionRequest, Box<BatchReviewResponse>> {
    let parts = raw_action.split(':').collect::<Vec<_>>();
    match parts.as_slice() {
        ["reject-duplicate" | "reject_duplicate", candidate_id] if !candidate_id.is_empty() => {
            Ok(BatchActionRequest::reject_duplicate(
                (*candidate_id).to_string(),
            ))
        }
        ["accept-transfer-pair" | "accept_transfer_pair", from_id, to_id]
            if !from_id.is_empty() && !to_id.is_empty() =>
        {
            Ok(BatchActionRequest::accept_transfer_pair(
                (*from_id).to_string(),
                (*to_id).to_string(),
            ))
        }
        ["reject-duplicate" | "reject_duplicate", ..]
        | ["accept-transfer-pair" | "accept_transfer_pair", ..] => Err(Box::new(
            batch_review_error_response_with_dry_run(
                "candidates apply-actions",
                "validation_failure",
                "candidate_ids_required",
                "Batch actions require explicit candidate ids.",
                "actions",
                serde_json::json!({ "action": raw_action }),
                dry_run,
            ),
        )),
        _ => Err(Box::new(batch_review_error_response_with_dry_run(
            "candidates apply-actions",
            "validation_failure",
            "invalid_batch_action",
            "Batch action must be reject-duplicate:CANDIDATE_ID or accept-transfer-pair:FROM_ID:TO_ID.",
            "actions",
            serde_json::json!({ "action": raw_action }),
            dry_run,
        ))),
    }
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

fn candidate_accept_income_command<W>(
    args: CandidateIncomeAcceptArgs,
    stdout: &mut W,
) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_candidate_json(args.json, stdout, "candidates accept-income")?
    {
        return Ok(exit_code);
    }
    let mut connection = open_review_database(&args.db)?;
    let response = accept_income_candidate(
        &mut connection,
        &args.candidate_id,
        &args.income_source_id,
        &args.income_kind,
    )?;
    write_candidate_review_response(stdout, response)
}

fn candidate_accept_expense_command<W>(
    args: CandidateExpenseAcceptArgs,
    stdout: &mut W,
) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) = require_candidate_json(args.json, stdout, "candidates accept-expense")?
    {
        return Ok(exit_code);
    }
    let lines =
        match expense_lines_from_args(args.category_id, args.lines, "candidates accept-expense") {
            Ok(lines) => lines,
            Err(response) => return write_candidate_review_response(stdout, *response),
        };
    let mut connection = open_review_database(&args.db)?;
    let response = accept_expense_candidate(&mut connection, &args.candidate_id, &lines)?;
    write_candidate_review_response(stdout, response)
}

fn candidate_accept_investment_command<W>(args: CandidateActionArgs, stdout: &mut W) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) =
        require_candidate_json(args.json, stdout, "candidates accept-investment")?
    {
        return Ok(exit_code);
    }
    let mut connection = open_review_database(&args.db)?;
    write_candidate_review_response(
        stdout,
        accept_investment_candidate(&mut connection, &args.candidate_id)?,
    )
}

fn candidate_set_expense_lines_command<W>(
    args: CandidateExpenseLinesArgs,
    stdout: &mut W,
) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) =
        require_candidate_json(args.json, stdout, "candidates set-expense-lines")?
    {
        return Ok(exit_code);
    }
    let lines = match expense_lines_from_args(None, args.lines, "candidates set-expense-lines") {
        Ok(lines) => lines,
        Err(response) => return write_candidate_review_response(stdout, *response),
    };
    let mut connection = open_review_database(&args.db)?;
    let response = replace_expense_transaction_lines(&mut connection, &args.candidate_id, &lines)?;
    write_candidate_review_response(stdout, response)
}

fn expense_lines_from_args(
    category_id: Option<String>,
    raw_lines: Vec<String>,
    command: &'static str,
) -> Result<Vec<ExpenseLineInput>, Box<CandidateReviewResponse>> {
    if category_id.is_some() && !raw_lines.is_empty() {
        return Err(Box::new(review_error_response(
            command,
            "validation_failure",
            "expense_category_or_lines_required",
            "Use either --category-id or one or more --line values, not both.".to_string(),
            "category_id",
            true,
            serde_json::json!({}),
        )));
    }
    if let Some(category_id) = category_id {
        return Ok(vec![ExpenseLineInput {
            category_id,
            amount_minor: 0,
            currency: String::new(),
        }]);
    }
    if raw_lines.is_empty() {
        return Err(Box::new(review_error_response(
            command,
            "validation_failure",
            "expense_lines_required",
            "Provide --category-id or at least one --line value.".to_string(),
            "lines",
            true,
            serde_json::json!({}),
        )));
    }
    raw_lines
        .into_iter()
        .map(|raw_line| parse_expense_line(&raw_line, command))
        .collect()
}

fn parse_expense_line(
    raw_line: &str,
    command: &'static str,
) -> Result<ExpenseLineInput, Box<CandidateReviewResponse>> {
    let Some((category_id, remainder)) = raw_line.split_once(':') else {
        return Err(invalid_expense_line_response(command, raw_line));
    };
    let Some((amount_minor, currency)) = remainder.split_once(':') else {
        return Err(invalid_expense_line_response(command, raw_line));
    };
    let Ok(amount_minor) = amount_minor.parse::<i64>() else {
        return Err(invalid_expense_line_response(command, raw_line));
    };
    if category_id.trim().is_empty() || currency.trim().is_empty() {
        return Err(invalid_expense_line_response(command, raw_line));
    }
    Ok(ExpenseLineInput {
        category_id: category_id.to_string(),
        amount_minor,
        currency: currency.to_ascii_uppercase(),
    })
}

fn invalid_expense_line_response(
    command: &'static str,
    raw_line: &str,
) -> Box<CandidateReviewResponse> {
    Box::new(review_error_response(
        command,
        "validation_failure",
        "invalid_expense_line",
        "Expense lines must use CATEGORY_ID:AMOUNT_MINOR:CURRENCY.".to_string(),
        "lines",
        true,
        serde_json::json!({ "line": raw_line }),
    ))
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

fn candidate_list_transfer_pairs_command<W>(
    args: CandidateTransferListArgs,
    stdout: &mut W,
) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) =
        require_transfer_json(args.json, stdout, "candidates list-transfer-pairs")?
    {
        return Ok(exit_code);
    }
    let connection = open_review_database(&args.db)?;
    let response = list_likely_transfer_pairs(&connection)?;
    write_transfer_review_response(stdout, response)
}

fn candidate_accept_transfer_pair_command<W>(
    args: CandidateTransferAcceptArgs,
    stdout: &mut W,
) -> Result<i32>
where
    W: Write,
{
    if let Some(exit_code) =
        require_transfer_json(args.json, stdout, "candidates accept-transfer-pair")?
    {
        return Ok(exit_code);
    }
    let mut connection = open_review_database(&args.db)?;
    let response = accept_transfer_pair(
        &mut connection,
        &args.from_candidate_id,
        &args.to_candidate_id,
    )?;
    write_transfer_review_response(stdout, response)
}

fn require_account_json<W>(json: bool, stdout: &mut W, command: &'static str) -> Result<Option<i32>>
where
    W: Write,
{
    require_json_flag(
        json,
        stdout,
        command,
        account_registry_error_response,
        write_account_registry_response,
    )
}

fn require_income_source_json<W>(
    json: bool,
    stdout: &mut W,
    command: &'static str,
) -> Result<Option<i32>>
where
    W: Write,
{
    require_json_flag(
        json,
        stdout,
        command,
        income_source_registry_error_response,
        write_income_source_registry_response,
    )
}

fn require_category_json<W>(
    json: bool,
    stdout: &mut W,
    command: &'static str,
) -> Result<Option<i32>>
where
    W: Write,
{
    require_json_flag(
        json,
        stdout,
        command,
        category_registry_error_response,
        write_category_registry_response,
    )
}

fn require_candidate_json<W>(
    json: bool,
    stdout: &mut W,
    command: &'static str,
) -> Result<Option<i32>>
where
    W: Write,
{
    require_json_flag(
        json,
        stdout,
        command,
        review_error_response,
        write_candidate_review_response,
    )
}

fn require_batch_json<W: Write>(
    json: bool,
    stdout: &mut W,
    command: &'static str,
    import_batch_id: Option<&str>,
) -> Result<Option<i32>> {
    if json {
        return Ok(None);
    }
    write_batch_review_response(
        stdout,
        batch_review_error_response(
            command,
            import_batch_id,
            "validation_failure",
            "json_output_required",
            "This candidates batch review command currently requires --json.",
            "command",
            serde_json::json!({ "flag": "--json" }),
        ),
    )
    .map(Some)
}

fn require_transaction_json<W>(
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
    write_transaction_ledger_response(
        stdout,
        TransactionLedgerResponse {
            schema_version: crate::storage::TRANSACTION_LEDGER_SCHEMA_VERSION,
            command,
            ok: false,
            canonical_transaction: None,
            canonical_transactions: Vec::new(),
            candidate: None,
            transaction_lines: Vec::new(),
            provenance: Vec::new(),
            edits: Vec::new(),
            transfer: None,
            errors: vec![crate::storage::ReviewError {
                category: "validation_failure",
                code: "json_output_required",
                message: format!("The {command} command currently requires --json."),
                path: "command",
                recoverable: true,
                details: serde_json::json!({ "flag": "--json" }),
            }],
        },
    )
    .map(Some)
}

fn require_transfer_json<W>(
    json: bool,
    stdout: &mut W,
    command: &'static str,
) -> Result<Option<i32>>
where
    W: Write,
{
    require_json_flag(
        json,
        stdout,
        command,
        transfer_error_response,
        write_transfer_review_response,
    )
}

fn require_manual_json<W>(json: bool, stdout: &mut W, command: &'static str) -> Result<Option<i32>>
where
    W: Write,
{
    require_json_flag(
        json,
        stdout,
        command,
        manual_json_error,
        write_manual_transaction_response,
    )
}

fn require_json_flag<W, Response, BuildError, WriteResponse>(
    json: bool,
    stdout: &mut W,
    command: &'static str,
    build_error: BuildError,
    write_response: WriteResponse,
) -> Result<Option<i32>>
where
    W: Write,
    BuildError: FnOnce(
        &'static str,
        &'static str,
        &'static str,
        String,
        &'static str,
        bool,
        serde_json::Value,
    ) -> Response,
    WriteResponse: FnOnce(&mut W, Response) -> Result<i32>,
{
    if json {
        return Ok(None);
    }
    let response = build_error(
        command,
        "validation_failure",
        "json_output_required",
        format!("The {command} command currently requires --json."),
        "command",
        true,
        serde_json::json!({ "flag": "--json" }),
    );
    write_response(stdout, response).map(Some)
}

fn open_review_database(db: &Path) -> Result<Connection> {
    let connection = Connection::open(db)
        .with_context(|| format!("opening SQLite database {}", db.display()))?;
    apply_migrations(&connection).context("applying SQLite migrations")?;
    Ok(connection)
}

fn open_readonly_database(db: &Path) -> Result<Connection> {
    Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening read-only SQLite database {}", db.display()))
}

fn database_open_batch_error(
    command: &'static str,
    import_batch_id: Option<&str>,
) -> BatchReviewResponse {
    batch_review_error_response(
        command,
        import_batch_id,
        "storage_failure",
        "database_open_failed",
        "SQLite database could not be opened.",
        "db",
        serde_json::json!({}),
    )
}

fn database_open_batch_error_with_dry_run(dry_run: bool) -> BatchReviewResponse {
    batch_review_error_response_with_dry_run(
        "candidates apply-actions",
        "storage_failure",
        "database_open_failed",
        "SQLite database could not be opened.",
        "db",
        serde_json::json!({}),
        dry_run,
    )
}

fn database_operation_batch_error(
    command: &'static str,
    import_batch_id: Option<&str>,
    dry_run: Option<bool>,
) -> BatchReviewResponse {
    let mut response = batch_review_error_response(
        command,
        import_batch_id,
        "storage_failure",
        "database_operation_failed",
        "SQLite could not complete the batch review operation.",
        "db",
        serde_json::json!({}),
    );
    response.dry_run = dry_run;
    response
}

fn write_account_registry_response<W: Write>(
    stdout: &mut W,
    response: AccountRegistryResponse,
) -> Result<i32> {
    write_json_response(
        stdout,
        response.ok,
        response,
        "writing account registry JSON",
    )
}

fn write_income_source_registry_response<W: Write>(
    stdout: &mut W,
    response: IncomeSourceRegistryResponse,
) -> Result<i32> {
    write_json_response(
        stdout,
        response.ok,
        response,
        "writing income source registry JSON",
    )
}

fn write_category_registry_response<W: Write>(
    stdout: &mut W,
    response: CategoryRegistryResponse,
) -> Result<i32> {
    write_json_response(
        stdout,
        response.ok,
        response,
        "writing category registry JSON",
    )
}

fn write_candidate_review_response<W: Write>(
    stdout: &mut W,
    response: CandidateReviewResponse,
) -> Result<i32> {
    write_json_response(
        stdout,
        response.ok,
        response,
        "writing candidate review JSON",
    )
}

fn write_batch_review_response<W: Write>(
    stdout: &mut W,
    response: BatchReviewResponse,
) -> Result<i32> {
    write_json_response(stdout, response.ok, response, "writing batch review JSON")
}

fn write_transfer_review_response<W: Write>(
    stdout: &mut W,
    response: TransferReviewResponse,
) -> Result<i32> {
    write_json_response(
        stdout,
        response.ok,
        response,
        "writing transfer review JSON",
    )
}

fn write_manual_transaction_response<W: Write>(
    stdout: &mut W,
    response: ManualTransactionResponse,
) -> Result<i32> {
    write_json_response(
        stdout,
        response.ok,
        response,
        "writing manual transaction JSON",
    )
}

fn write_transaction_ledger_response<W: Write>(
    stdout: &mut W,
    response: TransactionLedgerResponse,
) -> Result<i32> {
    write_json_response(
        stdout,
        response.ok,
        response,
        "writing transaction ledger JSON",
    )
}

fn write_finance_report_response<W: Write>(
    stdout: &mut W,
    response: FinanceReportResponse,
) -> Result<i32> {
    write_json_response(stdout, response.ok, response, "writing finance report JSON")
}

fn transaction_ledger_cli_error(
    command: &'static str,
    code: &'static str,
    message: &'static str,
) -> TransactionLedgerResponse {
    TransactionLedgerResponse {
        schema_version: crate::storage::TRANSACTION_LEDGER_SCHEMA_VERSION,
        command,
        ok: false,
        canonical_transaction: None,
        canonical_transactions: Vec::new(),
        candidate: None,
        transaction_lines: Vec::new(),
        provenance: Vec::new(),
        edits: Vec::new(),
        transfer: None,
        errors: vec![crate::storage::ReviewError {
            category: "validation_failure",
            code,
            message: message.to_string(),
            path: "command",
            recoverable: true,
            details: serde_json::json!({}),
        }],
    }
}

fn transaction_ledger_from_candidate_response(
    response: CandidateReviewResponse,
) -> TransactionLedgerResponse {
    TransactionLedgerResponse {
        schema_version: crate::storage::TRANSACTION_LEDGER_SCHEMA_VERSION,
        command: "transactions update",
        ok: false,
        canonical_transaction: None,
        canonical_transactions: Vec::new(),
        candidate: None,
        transaction_lines: Vec::new(),
        provenance: Vec::new(),
        edits: Vec::new(),
        transfer: None,
        errors: response.errors,
    }
}

fn manual_from_candidate_response(response: CandidateReviewResponse) -> ManualTransactionResponse {
    ManualTransactionResponse {
        schema_version: crate::storage::MANUAL_TRANSACTIONS_SCHEMA_VERSION,
        command: response.command,
        ok: false,
        canonical_transactions: Vec::new(),
        transaction_lines: Vec::new(),
        transfer_pair: None,
        provenance: Vec::new(),
        errors: response.errors,
    }
}

fn manual_json_error(
    command: &'static str,
    category: &'static str,
    code: &'static str,
    message: String,
    path: &'static str,
    _recoverable: bool,
    details: serde_json::Value,
) -> ManualTransactionResponse {
    ManualTransactionResponse {
        schema_version: crate::storage::MANUAL_TRANSACTIONS_SCHEMA_VERSION,
        command,
        ok: false,
        canonical_transactions: Vec::new(),
        transaction_lines: Vec::new(),
        transfer_pair: None,
        provenance: Vec::new(),
        errors: vec![crate::storage::ReviewError {
            category,
            code,
            message,
            path,
            recoverable: true,
            details,
        }],
    }
}

fn write_json_response<W, Response>(
    stdout: &mut W,
    ok: bool,
    response: Response,
    context: &'static str,
) -> Result<i32>
where
    W: Write,
    Response: serde::Serialize,
{
    let exit_code = if ok { 0 } else { 1 };
    serde_json::to_writer(&mut *stdout, &response).context(context)?;
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
