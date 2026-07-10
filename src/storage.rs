use crate::pdf::{
    CandidateStatus, CandidateTransaction, DirectionHint, DocumentDuplicateState,
    DocumentDuplicateStatus, DuplicateStatus, DuplicateStatusState, PdfInspectResponse,
    SemanticHint, SourceDocument, TrackyError,
};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

const REVIEW_FIRST_SCHEMA: &str = include_str!("../migrations/0001_review_first_schema.sql");
pub const IMPORT_PDF_SCHEMA_VERSION: &str = "tracky.import-pdf.v1";
pub const ACCOUNT_REGISTRY_SCHEMA_VERSION: &str = "tracky.accounts.v1";
pub const INCOME_SOURCE_REGISTRY_SCHEMA_VERSION: &str = "tracky.income-sources.v1";
pub const CATEGORY_REGISTRY_SCHEMA_VERSION: &str = "tracky.categories.v1";
pub const MANUAL_TRANSACTIONS_SCHEMA_VERSION: &str = "tracky.manual-transactions.v1";
pub const TRANSACTION_LEDGER_SCHEMA_VERSION: &str = "tracky.transactions.v1";
pub const FINANCE_REPORT_SCHEMA_VERSION: &str = "tracky.finance-report.v1";
pub const BATCH_REVIEW_SCHEMA_VERSION: &str = "tracky.batch-review.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRegisterInput {
    pub institution: String,
    pub label: String,
    pub account_type: String,
    pub currency: String,
    pub masked_identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomeSourceCreateInput {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryCreateInput {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AccountRegistryResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<OwnedAccount>,
    pub accounts: Vec<OwnedAccount>,
    pub errors: Vec<ReviewError>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IncomeSourceRegistryResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub income_source: Option<IncomeSource>,
    pub income_sources: Vec<IncomeSource>,
    pub errors: Vec<ReviewError>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CategoryRegistryResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<Category>,
    pub categories: Vec<Category>,
    pub errors: Vec<ReviewError>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OwnedAccount {
    pub id: String,
    pub institution_id: String,
    pub institution: String,
    pub label: String,
    pub account_type: String,
    pub currency: String,
    pub masked_identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IncomeSource {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Category {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CandidateReviewResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate: Option<ReviewCandidate>,
    pub candidates: Vec<ReviewCandidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_transaction: Option<CanonicalTransaction>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub transaction_lines: Vec<TransactionLine>,
    pub errors: Vec<ReviewError>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TransferReviewResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer_pair: Option<ReviewTransferPair>,
    pub transfer_pairs: Vec<ReviewTransferPair>,
    pub canonical_transactions: Vec<CanonicalTransaction>,
    pub errors: Vec<ReviewError>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReviewTransferPair {
    pub id: String,
    pub transfer_kind: &'static str,
    pub posted_date: String,
    pub amount_minor: i64,
    pub currency: String,
    pub from_account: OwnedAccount,
    pub to_account: OwnedAccount,
    pub from_candidate: ReviewCandidate,
    pub to_candidate: ReviewCandidate,
    pub canonical_transaction_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReviewCandidate {
    pub id: String,
    pub import_batch_id: String,
    pub source_document_id: String,
    pub status: CandidateStatus,
    pub duplicate_status: ReviewDuplicateStatus,
    pub institution_id: Option<String>,
    pub institution_hint: Option<String>,
    pub account_id: Option<String>,
    pub account_hint: ReviewAccountHint,
    pub posted_date: String,
    pub description: String,
    pub amount_minor: i64,
    pub currency: String,
    pub balance_minor: Option<i64>,
    pub direction_hint: Option<String>,
    pub semantic_hint: Option<String>,
    pub confidence: f64,
    pub provenance: ReviewProvenance,
    pub validation_warnings: Vec<String>,
    pub canonical_transaction_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReviewAccountHint {
    pub label: Option<String>,
    pub currency: Option<String>,
    pub masked_identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReviewDuplicateStatus {
    pub status: DuplicateStatusState,
    pub fingerprint: Option<String>,
    pub matched_candidate_ids: Vec<String>,
    pub matched_canonical_transaction_ids: Vec<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReviewProvenance {
    pub candidate_transaction_id: Option<String>,
    pub source_document_id: String,
    pub import_batch_id: Option<String>,
    pub page_number: Option<i64>,
    pub row_index: Option<i64>,
    pub evidence_redaction: String,
    pub evidence_text_redacted: String,
    pub raw_storage_policy: String,
    pub extractor_name: String,
    pub extractor_version: Option<String>,
    pub parser_id: String,
    pub parser_version: String,
    pub confidence: f64,
    pub canonical_transaction_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CanonicalTransaction {
    pub id: String,
    pub account_id: Option<String>,
    pub posted_date: String,
    pub description: String,
    pub amount_minor: i64,
    pub currency: String,
    pub balance_minor: Option<i64>,
    pub transaction_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub income_source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub income_kind: Option<String>,
    pub created_from_candidate_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TransactionLine {
    pub id: String,
    pub canonical_transaction_id: String,
    pub category_id: String,
    pub category_name: String,
    pub amount_minor: i64,
    pub currency: String,
    pub line_kind: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TransactionProvenance {
    pub source: String,
    pub entry_id: Option<String>,
    pub candidate_provenance: Option<ReviewProvenance>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TransactionTransferMetadata {
    pub id: String,
    pub transfer_kind: String,
    pub from_account_id: String,
    pub to_account_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TransactionEdit {
    pub id: String,
    pub changed_fields: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TransactionLedgerResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_transaction: Option<CanonicalTransaction>,
    pub canonical_transactions: Vec<CanonicalTransaction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate: Option<ReviewCandidate>,
    pub transaction_lines: Vec<TransactionLine>,
    pub provenance: Vec<TransactionProvenance>,
    pub edits: Vec<TransactionEdit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer: Option<TransactionTransferMetadata>,
    pub errors: Vec<ReviewError>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReportDateRange {
    pub start_date: String,
    pub end_date: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FinanceCurrencyTotal {
    pub currency: String,
    pub total_income_minor: i64,
    pub total_expenses_minor: i64,
    pub net_cash_flow_minor: i64,
    pub excluded_transfer_total_minor: i64,
    pub excluded_transfer_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CategoryTotal {
    pub category_id: String,
    pub category_name: String,
    pub currency: String,
    pub total_expenses_minor: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IncomeSourceTotal {
    pub income_source_id: String,
    pub income_source_name: String,
    pub currency: String,
    pub total_income_minor: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExcludedTransferTotal {
    pub transfer_kind: String,
    pub currency: String,
    pub total_amount_minor: i64,
    pub transfer_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FinanceReportResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    pub date_range: ReportDateRange,
    pub totals: Vec<FinanceCurrencyTotal>,
    pub category_totals: Vec<CategoryTotal>,
    pub income_source_totals: Vec<IncomeSourceTotal>,
    pub excluded_transfer_totals: Vec<ExcludedTransferTotal>,
    pub errors: Vec<ReviewError>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BatchReviewResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_batch_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<BatchSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison: Option<DuplicateComparison>,
    pub suggestions: Vec<ReviewSuggestion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    pub action_results: Vec<BatchActionResult>,
    pub errors: Vec<ReviewError>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BatchSummary {
    pub total_candidates: usize,
    pub by_status: Vec<GroupCount>,
    pub by_duplicate_status: Vec<GroupCount>,
    pub by_institution: Vec<GroupCount>,
    pub by_account_resolution: Vec<GroupCount>,
    pub by_direction_hint: Vec<GroupCount>,
    pub by_semantic_hint: Vec<GroupCount>,
    pub largest_amounts: Vec<LargestCandidateAmount>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GroupCount {
    pub key: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LargestCandidateAmount {
    pub candidate_id: String,
    pub posted_date: String,
    pub description: String,
    pub amount_minor: i64,
    pub absolute_amount_minor: u64,
    pub currency: String,
    pub status: CandidateStatus,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DuplicateComparison {
    pub candidate: ReviewCandidate,
    pub matched_candidates: Vec<ReviewCandidate>,
    pub matched_canonical_transactions: Vec<MatchedCanonicalTransaction>,
    pub fingerprints: Vec<ReviewFingerprint>,
    pub duplicate_markers: Vec<ReviewDuplicateMarker>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MatchedCanonicalTransaction {
    pub transaction: CanonicalTransaction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate: Option<ReviewCandidate>,
    pub provenance: Vec<TransactionProvenance>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReviewFingerprint {
    pub id: String,
    pub fingerprint: String,
    pub candidate_transaction_id: Option<String>,
    pub canonical_transaction_id: Option<String>,
    pub duplicate_status: String,
    pub normalized_account_key: Option<String>,
    pub normalized_posted_date: String,
    pub normalized_amount_minor: i64,
    pub normalized_currency: String,
    pub normalized_description: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReviewDuplicateMarker {
    pub id: String,
    pub candidate_transaction_id: String,
    pub matched_candidate_transaction_id: Option<String>,
    pub matched_canonical_transaction_id: Option<String>,
    pub duplicate_status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReviewSuggestion {
    pub id: String,
    pub proposed_action: &'static str,
    pub candidate_ids: Vec<String>,
    pub import_batch_ids: Vec<String>,
    pub reason: &'static str,
    pub evidence: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BatchActionKind {
    RejectDuplicate,
    AcceptTransferPair,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchActionRequest {
    kind: BatchActionKind,
    candidate_ids: Vec<String>,
}

impl BatchActionRequest {
    pub fn reject_duplicate(candidate_id: String) -> Self {
        Self {
            kind: BatchActionKind::RejectDuplicate,
            candidate_ids: vec![candidate_id],
        }
    }

    pub fn accept_transfer_pair(from_candidate_id: String, to_candidate_id: String) -> Self {
        Self {
            kind: BatchActionKind::AcceptTransferPair,
            candidate_ids: vec![from_candidate_id, to_candidate_id],
        }
    }

    fn action(&self) -> &'static str {
        match self.kind {
            BatchActionKind::RejectDuplicate => "reject_duplicate",
            BatchActionKind::AcceptTransferPair => "accept_transfer_pair",
        }
    }

    fn candidate_ids(&self) -> &[String] {
        &self.candidate_ids
    }
}

struct PreparedBatchAction {
    action: &'static str,
    candidate_ids: Vec<String>,
    mutation: BatchActionMutation,
}

enum BatchActionMutation {
    RejectDuplicate { candidate_id: String },
    AcceptTransferPair { pair: ReviewTransferPair },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BatchActionResult {
    pub action: &'static str,
    pub candidate_ids: Vec<String>,
    pub status: &'static str,
    pub canonical_transaction_ids: Vec<String>,
    pub errors: Vec<ReviewError>,
}

#[derive(Debug, Clone, Default)]
pub struct TransactionListFilter<'a> {
    pub start_date: Option<&'a str>,
    pub end_date: Option<&'a str>,
    pub account_id: Option<&'a str>,
    pub category_id: Option<&'a str>,
    pub income_source_id: Option<&'a str>,
    pub transaction_kind: Option<&'a str>,
}

#[derive(Debug, Clone, Default)]
pub struct TransactionUpdateInput {
    pub description: Option<String>,
    pub income_source_id: Option<String>,
    pub income_kind: Option<String>,
    pub expense_lines: Option<Vec<ExpenseLineInput>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExpenseLineInput {
    pub category_id: String,
    pub amount_minor: i64,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManualExpenseInput {
    pub account_id: String,
    pub posted_date: String,
    pub description: String,
    pub amount_minor: i64,
    pub currency: String,
    pub lines: Vec<ExpenseLineInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualIncomeInput {
    pub account_id: String,
    pub posted_date: String,
    pub description: String,
    pub amount_minor: i64,
    pub currency: String,
    pub income_source_id: String,
    pub income_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualTransferInput {
    pub from_account_id: String,
    pub to_account_id: String,
    pub posted_date: String,
    pub description: String,
    pub amount_minor: i64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ManualProvenance {
    pub source: &'static str,
    pub entry_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ManualTransferPair {
    pub id: String,
    pub transfer_kind: &'static str,
    pub posted_date: String,
    pub amount_minor: i64,
    pub currency: String,
    pub from_account: OwnedAccount,
    pub to_account: OwnedAccount,
    pub canonical_transaction_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ManualTransactionResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    pub canonical_transactions: Vec<CanonicalTransaction>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub transaction_lines: Vec<TransactionLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer_pair: Option<ManualTransferPair>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<ManualProvenance>,
    pub errors: Vec<ReviewError>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReviewError {
    pub category: &'static str,
    pub code: &'static str,
    pub message: String,
    pub path: &'static str,
    pub recoverable: bool,
    pub details: serde_json::Value,
}

pub const CANDIDATE_REVIEW_SCHEMA_VERSION: &str = "tracky.candidate-review.v1";
pub const TRANSFER_REVIEW_SCHEMA_VERSION: &str = "tracky.transfer-review.v1";
const OWN_ACCOUNT_TRANSFER_KIND: &str = "own_account_transfer";
const CARD_PAYMENT_TRANSFER_KIND: &str = "card_payment";
const INCOME_TRANSACTION_KIND: &str = "income";
const EXPENSE_TRANSACTION_KIND: &str = "expense";
const EXPENSE_LINE_KIND: &str = "expense";
const INCOME_KINDS: &[&str] = &[
    "salary",
    "freelance",
    "client_payment",
    "sale",
    "interest",
    "reimbursement",
    "other",
];

/// Apply Tracky's SQLite migrations needed for the review-first import store.
pub fn apply_migrations(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(REVIEW_FIRST_SCHEMA)?;
    add_column_if_missing(
        connection,
        "import_batches",
        "duplicate_count",
        "ALTER TABLE import_batches ADD COLUMN duplicate_count INTEGER NOT NULL DEFAULT 0 CHECK (duplicate_count >= 0)",
    )?;
    add_column_if_missing(
        connection,
        "candidate_transactions",
        "semantic_hint",
        "ALTER TABLE candidate_transactions ADD COLUMN semantic_hint TEXT CHECK (semantic_hint IN ('bank_movement', 'card_charge', 'card_payment'))",
    )?;
    add_column_if_missing(
        connection,
        "accounts",
        "is_owned",
        "ALTER TABLE accounts ADD COLUMN is_owned INTEGER NOT NULL DEFAULT 0 CHECK (is_owned IN (0, 1))",
    )?;
    add_column_if_missing(
        connection,
        "canonical_transactions",
        "income_source_id",
        "ALTER TABLE canonical_transactions ADD COLUMN income_source_id TEXT REFERENCES income_sources(id)",
    )?;
    add_column_if_missing(
        connection,
        "canonical_transactions",
        "income_kind",
        "ALTER TABLE canonical_transactions ADD COLUMN income_kind TEXT CHECK (income_kind IN ('salary', 'freelance', 'client_payment', 'sale', 'interest', 'reimbursement', 'other'))",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS income_sources (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );
        CREATE INDEX IF NOT EXISTS idx_income_sources_name ON income_sources(name);
        CREATE TABLE IF NOT EXISTS categories (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );
        CREATE INDEX IF NOT EXISTS idx_categories_name ON categories(name);
        CREATE TABLE IF NOT EXISTS transaction_lines (
            id TEXT PRIMARY KEY,
            canonical_transaction_id TEXT NOT NULL REFERENCES canonical_transactions(id),
            category_id TEXT NOT NULL REFERENCES categories(id),
            amount_minor INTEGER NOT NULL CHECK (amount_minor <> 0),
            currency TEXT NOT NULL,
            line_kind TEXT NOT NULL CHECK (line_kind IN ('expense')),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );
        CREATE INDEX IF NOT EXISTS idx_transaction_lines_canonical ON transaction_lines(canonical_transaction_id);
        CREATE INDEX IF NOT EXISTS idx_transaction_lines_category ON transaction_lines(category_id);
        CREATE TABLE IF NOT EXISTS canonical_transfer_pairs (
            id TEXT PRIMARY KEY,
            transfer_kind TEXT NOT NULL CHECK (transfer_kind IN ('card_payment')),
            posted_date TEXT NOT NULL,
            amount_minor INTEGER NOT NULL CHECK (amount_minor > 0),
            currency TEXT NOT NULL,
            from_account_id TEXT NOT NULL REFERENCES accounts(id),
            to_account_id TEXT NOT NULL REFERENCES accounts(id),
            from_candidate_id TEXT NOT NULL UNIQUE REFERENCES candidate_transactions(id),
            to_candidate_id TEXT NOT NULL UNIQUE REFERENCES candidate_transactions(id),
            from_canonical_transaction_id TEXT NOT NULL UNIQUE REFERENCES canonical_transactions(id),
            to_canonical_transaction_id TEXT NOT NULL UNIQUE REFERENCES canonical_transactions(id),
            accepted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );
        CREATE INDEX IF NOT EXISTS idx_canonical_transfer_pairs_from_candidate ON canonical_transfer_pairs(from_candidate_id);
        CREATE INDEX IF NOT EXISTS idx_canonical_transfer_pairs_to_candidate ON canonical_transfer_pairs(to_candidate_id);
        CREATE TABLE IF NOT EXISTS manual_transaction_provenance (
            canonical_transaction_id TEXT PRIMARY KEY REFERENCES canonical_transactions(id),
            entry_id TEXT NOT NULL UNIQUE,
            source TEXT NOT NULL CHECK (source = 'manual_entry'),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );
        CREATE TABLE IF NOT EXISTS manual_transfer_pairs (
            id TEXT PRIMARY KEY,
            posted_date TEXT NOT NULL,
            amount_minor INTEGER NOT NULL CHECK (amount_minor > 0),
            currency TEXT NOT NULL,
            from_account_id TEXT NOT NULL REFERENCES accounts(id),
            to_account_id TEXT NOT NULL REFERENCES accounts(id),
            from_canonical_transaction_id TEXT NOT NULL UNIQUE REFERENCES canonical_transactions(id),
            to_canonical_transaction_id TEXT NOT NULL UNIQUE REFERENCES canonical_transactions(id),
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );
        CREATE TABLE IF NOT EXISTS canonical_transaction_edits (
            id TEXT PRIMARY KEY,
            canonical_transaction_id TEXT NOT NULL REFERENCES canonical_transactions(id),
            changed_fields_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );
        CREATE INDEX IF NOT EXISTS idx_canonical_transaction_edits_canonical ON canonical_transaction_edits(canonical_transaction_id);",
    )
}

fn add_column_if_missing(
    connection: &Connection,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> rusqlite::Result<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for existing in columns {
        if existing? == column {
            return Ok(());
        }
    }
    connection.execute(alter_sql, [])?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ImportBatch {
    pub id: String,
    pub source_document_id: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub status: ImportBatchStatus,
    pub candidate_count: usize,
    pub error_count: usize,
    pub duplicate_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImportBatchStatus {
    Completed,
    CompletedWithErrors,
    Failed,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ImportPdfResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_batch: Option<ImportBatch>,
    pub source_document: SourceDocument,
    pub extractor_status: crate::pdf::ExtractorStatus,
    pub parser_status: crate::pdf::ParserStatus,
    pub candidates: Vec<CandidateTransaction>,
    pub errors: Vec<TrackyError>,
}

pub fn register_owned_account(
    connection: &Connection,
    input: AccountRegisterInput,
) -> Result<AccountRegistryResponse> {
    validate_account_input(&input)?;
    let institution_id = institution_id(&input.institution);
    let account_id = account_id(&input);
    connection.execute(
        "INSERT OR IGNORE INTO institutions (id, name) VALUES (?1, ?2)",
        params![institution_id, input.institution.trim()],
    )?;
    connection.execute(
        "INSERT INTO accounts (
            id, institution_id, label, currency, masked_identifier, kind, is_owned
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)
         ON CONFLICT(id) DO UPDATE SET
            institution_id = excluded.institution_id,
            label = excluded.label,
            currency = excluded.currency,
            masked_identifier = excluded.masked_identifier,
            kind = excluded.kind,
            is_owned = 1",
        params![
            account_id,
            institution_id,
            input.label.trim(),
            normalized_currency(&input.currency),
            input.masked_identifier.as_deref().map(str::trim),
            input.account_type.trim(),
        ],
    )?;
    let account = owned_account_by_id(connection, &account_id)?
        .expect("registered owned account remains queryable");
    Ok(AccountRegistryResponse {
        schema_version: ACCOUNT_REGISTRY_SCHEMA_VERSION,
        command: "accounts register",
        ok: true,
        account: Some(account),
        accounts: Vec::new(),
        errors: Vec::new(),
    })
}

pub fn list_owned_accounts(connection: &Connection) -> Result<AccountRegistryResponse> {
    let mut statement = connection.prepare(
        "SELECT a.id, i.id, i.name, a.label, a.kind, a.currency, a.masked_identifier
         FROM accounts a
         JOIN institutions i ON i.id = a.institution_id
         WHERE a.is_owned = 1
         ORDER BY LOWER(i.name), LOWER(a.label), a.id",
    )?;
    let rows = statement.query_map([], owned_account_from_row)?;
    let mut accounts = Vec::new();
    for row in rows {
        accounts.push(row?);
    }
    Ok(AccountRegistryResponse {
        schema_version: ACCOUNT_REGISTRY_SCHEMA_VERSION,
        command: "accounts list",
        ok: true,
        account: None,
        accounts,
        errors: Vec::new(),
    })
}

pub fn account_registry_error_response(
    command: &'static str,
    category: &'static str,
    code: &'static str,
    message: String,
    path: &'static str,
    recoverable: bool,
    details: serde_json::Value,
) -> AccountRegistryResponse {
    AccountRegistryResponse {
        schema_version: ACCOUNT_REGISTRY_SCHEMA_VERSION,
        command,
        ok: false,
        account: None,
        accounts: Vec::new(),
        errors: vec![ReviewError {
            category,
            code,
            message,
            path,
            recoverable,
            details,
        }],
    }
}

pub fn create_income_source(
    connection: &Connection,
    input: IncomeSourceCreateInput,
) -> Result<IncomeSourceRegistryResponse> {
    validate_income_source_input(&input)?;
    let id = income_source_id(&input.name);
    connection.execute(
        "INSERT INTO income_sources (id, name) VALUES (?1, ?2)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name",
        params![id, input.name.trim()],
    )?;
    let income_source =
        income_source_by_id(connection, &id)?.expect("created income source remains queryable");
    Ok(IncomeSourceRegistryResponse {
        schema_version: INCOME_SOURCE_REGISTRY_SCHEMA_VERSION,
        command: "income-sources create",
        ok: true,
        income_source: Some(income_source),
        income_sources: Vec::new(),
        errors: Vec::new(),
    })
}

pub fn list_income_sources(connection: &Connection) -> Result<IncomeSourceRegistryResponse> {
    let mut statement = connection.prepare(
        "SELECT id, name
         FROM income_sources
         ORDER BY LOWER(name), id",
    )?;
    let rows = statement.query_map([], income_source_from_row)?;
    let mut income_sources = Vec::new();
    for row in rows {
        income_sources.push(row?);
    }
    Ok(IncomeSourceRegistryResponse {
        schema_version: INCOME_SOURCE_REGISTRY_SCHEMA_VERSION,
        command: "income-sources list",
        ok: true,
        income_source: None,
        income_sources,
        errors: Vec::new(),
    })
}

pub fn income_source_registry_error_response(
    command: &'static str,
    category: &'static str,
    code: &'static str,
    message: String,
    path: &'static str,
    recoverable: bool,
    details: serde_json::Value,
) -> IncomeSourceRegistryResponse {
    IncomeSourceRegistryResponse {
        schema_version: INCOME_SOURCE_REGISTRY_SCHEMA_VERSION,
        command,
        ok: false,
        income_source: None,
        income_sources: Vec::new(),
        errors: vec![ReviewError {
            category,
            code,
            message,
            path,
            recoverable,
            details,
        }],
    }
}

pub fn create_category(
    connection: &Connection,
    input: CategoryCreateInput,
) -> Result<CategoryRegistryResponse> {
    validate_category_input(&input)?;
    let id = category_id(&input.name);
    connection.execute(
        "INSERT INTO categories (id, name) VALUES (?1, ?2)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name",
        params![id, input.name.trim()],
    )?;
    let category = category_by_id(connection, &id)?.expect("created category remains queryable");
    Ok(CategoryRegistryResponse {
        schema_version: CATEGORY_REGISTRY_SCHEMA_VERSION,
        command: "categories create",
        ok: true,
        category: Some(category),
        categories: Vec::new(),
        errors: Vec::new(),
    })
}

pub fn list_categories(connection: &Connection) -> Result<CategoryRegistryResponse> {
    let mut statement = connection.prepare(
        "SELECT id, name
         FROM categories
         ORDER BY LOWER(name), id",
    )?;
    let rows = statement.query_map([], category_from_row)?;
    let mut categories = Vec::new();
    for row in rows {
        categories.push(row?);
    }
    Ok(CategoryRegistryResponse {
        schema_version: CATEGORY_REGISTRY_SCHEMA_VERSION,
        command: "categories list",
        ok: true,
        category: None,
        categories,
        errors: Vec::new(),
    })
}

pub fn category_registry_error_response(
    command: &'static str,
    category: &'static str,
    code: &'static str,
    message: String,
    path: &'static str,
    recoverable: bool,
    details: serde_json::Value,
) -> CategoryRegistryResponse {
    CategoryRegistryResponse {
        schema_version: CATEGORY_REGISTRY_SCHEMA_VERSION,
        command,
        ok: false,
        category: None,
        categories: Vec::new(),
        errors: vec![ReviewError {
            category,
            code,
            message,
            path,
            recoverable,
            details,
        }],
    }
}

fn validate_account_input(input: &AccountRegisterInput) -> Result<()> {
    if input.institution.trim().is_empty() {
        anyhow::bail!("institution is required");
    }
    if input.label.trim().is_empty() {
        anyhow::bail!("account label is required");
    }
    if input.account_type.trim().is_empty() {
        anyhow::bail!("account type is required");
    }
    if input.currency.trim().is_empty() {
        anyhow::bail!("currency is required");
    }
    Ok(())
}

fn owned_account_by_id(connection: &Connection, id: &str) -> Result<Option<OwnedAccount>> {
    connection
        .query_row(
            "SELECT a.id, i.id, i.name, a.label, a.kind, a.currency, a.masked_identifier
             FROM accounts a
             JOIN institutions i ON i.id = a.institution_id
             WHERE a.id = ?1 AND a.is_owned = 1",
            params![id],
            owned_account_from_row,
        )
        .optional()
        .map_err(Into::into)
}

fn owned_account_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OwnedAccount> {
    Ok(OwnedAccount {
        id: row.get(0)?,
        institution_id: row.get(1)?,
        institution: row.get(2)?,
        label: row.get(3)?,
        account_type: row.get(4)?,
        currency: row.get(5)?,
        masked_identifier: row.get(6)?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AccountResolution {
    institution_id: String,
    account_id: String,
}

fn resolve_owned_account(
    connection: &Connection,
    institution_hint: &str,
    account_hint: &crate::pdf::AccountHint,
) -> Result<Option<AccountResolution>> {
    let mut statement = connection.prepare(
        "SELECT a.id, a.institution_id, i.name, a.label, a.kind, a.masked_identifier
         FROM accounts a
         JOIN institutions i ON i.id = a.institution_id
         WHERE a.is_owned = 1 AND UPPER(a.currency) = ?1",
    )?;
    let rows = statement.query_map(params![normalized_currency(account_hint.currency)], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    })?;
    let institution_key = normalize_match_key(institution_hint);
    let label_key = normalize_match_key(&account_hint.label);
    let masked_key = account_hint
        .masked_identifier
        .as_deref()
        .map(normalize_match_key);
    let mut matches = Vec::new();
    for row in rows {
        let (account_id, institution_id, institution, label, kind, masked_identifier) = row?;
        let institution_matches = normalize_match_key(&institution) == institution_key
            || normalize_match_key(&institution_id) == institution_key;
        let label_or_type_matches =
            normalize_match_key(&label) == label_key || normalize_match_key(&kind) == label_key;
        let masked_matches = match (&masked_key, masked_identifier.as_deref()) {
            (Some(expected), Some(actual)) => normalize_match_key(actual) == *expected,
            (Some(_), None) => false,
            (None, _) => true,
        };
        if institution_matches && label_or_type_matches && masked_matches {
            matches.push(AccountResolution {
                institution_id,
                account_id,
            });
        }
    }
    if matches.len() == 1 {
        Ok(matches.pop())
    } else {
        Ok(None)
    }
}

pub fn find_source_document_by_hash(
    connection: &Connection,
    content_sha256: &str,
) -> rusqlite::Result<Option<String>> {
    connection
        .query_row(
            "SELECT id FROM source_documents WHERE content_sha256 = ?1",
            params![content_sha256],
            |row| row.get(0),
        )
        .optional()
}

pub fn persist_pdf_import(
    connection: &mut Connection,
    inspect: PdfInspectResponse,
) -> Result<ImportPdfResponse> {
    if let Some(existing_id) =
        find_source_document_by_hash(connection, &inspect.source_document.content_sha256)?
    {
        let source_document = duplicate_source_document(inspect.source_document, existing_id);
        return Ok(ImportPdfResponse {
            schema_version: IMPORT_PDF_SCHEMA_VERSION,
            command: "import pdf",
            ok: false,
            import_batch: None,
            source_document,
            extractor_status: inspect.extractor_status,
            parser_status: inspect.parser_status,
            candidates: Vec::new(),
            errors: vec![duplicate_source_document_error()],
        });
    }

    let started_at = sqlite_now(connection)?;
    let completed_at = sqlite_now(connection)?;
    let status = if inspect.errors.is_empty() {
        ImportBatchStatus::Completed
    } else if inspect.candidates.is_empty() {
        ImportBatchStatus::Failed
    } else {
        ImportBatchStatus::CompletedWithErrors
    };
    let mut source_document = inspect.source_document;
    source_document.document_duplicate_status = DocumentDuplicateStatus {
        status: DocumentDuplicateState::New,
        matched_source_document_id: None,
        reason: None,
    };
    let batch = ImportBatch {
        id: import_batch_id(&source_document.content_sha256),
        source_document_id: source_document.id.clone(),
        started_at,
        completed_at: Some(completed_at),
        status,
        candidate_count: inspect.candidates.len(),
        error_count: inspect.errors.len(),
        duplicate_count: 0,
    };
    let mut candidates = inspect.candidates;
    mark_candidate_duplicates(connection, &mut candidates)?;
    let duplicate_count = candidates
        .iter()
        .filter(|candidate| is_duplicate_status(&candidate.duplicate_status.status))
        .count();
    for candidate in &mut candidates {
        candidate.import_batch_id = Some(batch.id.clone());
        candidate.source_document_id = source_document.id.clone();
        candidate.provenance.source_document_id = source_document.id.clone();
    }
    let batch = ImportBatch {
        duplicate_count,
        ..batch
    };
    let source_account_resolution = resolve_owned_account(
        connection,
        &source_document.institution_hint,
        &source_document.account_hint,
    )?;
    let candidate_account_resolutions = candidates
        .iter()
        .map(|candidate| {
            resolve_owned_account(
                connection,
                &candidate.institution_hint,
                &candidate.account_hint,
            )
            .map(|resolution| (candidate.id.clone(), resolution))
        })
        .collect::<Result<std::collections::HashMap<_, _>>>()?;

    let tx = connection
        .transaction()
        .context("starting import transaction")?;
    insert_source_document(&tx, &source_document, source_account_resolution.as_ref())?;
    insert_import_batch(&tx, &batch, &inspect.errors)?;
    for candidate in &candidates {
        insert_candidate(
            &tx,
            candidate,
            &source_document,
            &batch.id,
            candidate_account_resolutions
                .get(&candidate.id)
                .and_then(Option::as_ref),
        )?;
        insert_provenance(&tx, candidate, &source_document, &batch.id)?;
        insert_fingerprint(&tx, candidate)?;
    }
    for candidate in &candidates {
        insert_duplicate_markers(&tx, candidate)?;
    }
    tx.commit().context("committing pdf import")?;

    Ok(ImportPdfResponse {
        schema_version: IMPORT_PDF_SCHEMA_VERSION,
        command: "import pdf",
        ok: inspect.errors.is_empty(),
        import_batch: Some(batch),
        source_document,
        extractor_status: inspect.extractor_status,
        parser_status: inspect.parser_status,
        candidates,
        errors: inspect.errors,
    })
}

pub fn duplicate_import_response(
    source_document: SourceDocument,
    existing_source_document_id: String,
    extractor_status: crate::pdf::ExtractorStatus,
    parser_status: crate::pdf::ParserStatus,
) -> ImportPdfResponse {
    ImportPdfResponse {
        schema_version: IMPORT_PDF_SCHEMA_VERSION,
        command: "import pdf",
        ok: false,
        import_batch: None,
        source_document: duplicate_source_document(source_document, existing_source_document_id),
        extractor_status,
        parser_status,
        candidates: Vec::new(),
        errors: vec![duplicate_source_document_error()],
    }
}

fn duplicate_source_document(
    mut source_document: SourceDocument,
    existing_id: String,
) -> SourceDocument {
    source_document.document_duplicate_status = DocumentDuplicateStatus {
        status: DocumentDuplicateState::DuplicateSourceDocument,
        matched_source_document_id: Some(existing_id),
        reason: Some("source_document_already_imported".to_string()),
    };
    source_document
}

pub fn duplicate_source_document_error() -> TrackyError {
    TrackyError {
        category: crate::pdf::TrackyErrorCategory::ValidationFailure,
        code: crate::pdf::TrackyErrorCode::DuplicateSourceDocument,
        message: "Source document already imported.".to_string(),
        path: crate::pdf::TrackyErrorPath::SourceDocumentDuplicateStatus,
        recoverable: true,
        details: serde_json::json!({ "reason": "source_document_already_imported" }),
    }
}

fn sqlite_now(connection: &Connection) -> rusqlite::Result<String> {
    connection.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
        row.get(0)
    })
}

fn insert_source_document(
    connection: &Connection,
    source: &SourceDocument,
    resolution: Option<&AccountResolution>,
) -> Result<()> {
    connection.execute(
        "INSERT INTO source_documents (
            id, input_name, content_sha256, mime_type, byte_size,
            institution_id, institution_hint, account_id, account_label_hint, account_currency_hint,
            account_masked_identifier_hint
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            source.id,
            source.input_name,
            source.content_sha256,
            source.mime_type,
            source.byte_size as i64,
            resolution.map(|value| value.institution_id.as_str()),
            source.institution_hint,
            resolution.map(|value| value.account_id.as_str()),
            source.account_hint.label,
            source.account_hint.currency,
            source.account_hint.masked_identifier,
        ],
    )?;
    Ok(())
}

fn insert_import_batch(
    connection: &Connection,
    batch: &ImportBatch,
    errors: &[TrackyError],
) -> Result<()> {
    connection.execute(
        "INSERT INTO import_batches (
            id, source_document_id, started_at, completed_at, status,
            candidate_count, error_count, duplicate_count, error_details_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            batch.id,
            batch.source_document_id,
            batch.started_at,
            batch.completed_at,
            import_batch_status_value(&batch.status),
            batch.candidate_count as i64,
            batch.error_count as i64,
            batch.duplicate_count as i64,
            serde_json::to_string(errors).context("serializing import errors")?,
        ],
    )?;
    Ok(())
}

fn mark_candidate_duplicates(
    connection: &Connection,
    candidates: &mut [CandidateTransaction],
) -> Result<()> {
    let exact_matches = in_batch_exact_matches(candidates);
    let near_matches = in_batch_near_matches(candidates);
    for candidate in candidates {
        let duplicate_status =
            duplicate_status_for_candidate(connection, candidate, &exact_matches, &near_matches)?;
        if should_mark_candidate_possible_duplicate(candidate, &duplicate_status.status) {
            candidate.status = CandidateStatus::PossibleDuplicate;
        }
        candidate.duplicate_status = duplicate_status;
    }
    Ok(())
}

fn in_batch_exact_matches(
    candidates: &[CandidateTransaction],
) -> std::collections::HashMap<String, Vec<String>> {
    let mut by_fingerprint: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for candidate in candidates {
        by_fingerprint
            .entry(candidate.duplicate_status.fingerprint.clone())
            .or_default()
            .push(candidate.id.clone());
    }
    by_fingerprint
}

fn in_batch_near_matches(
    candidates: &[CandidateTransaction],
) -> std::collections::HashMap<DuplicateNearKey, Vec<String>> {
    let mut by_near_key: std::collections::HashMap<DuplicateNearKey, Vec<String>> =
        std::collections::HashMap::new();
    for candidate in candidates {
        by_near_key
            .entry(DuplicateNearKey::from(candidate))
            .or_default()
            .push(candidate.id.clone());
    }
    by_near_key
}

fn duplicate_status_for_candidate(
    connection: &Connection,
    candidate: &CandidateTransaction,
    exact_matches: &std::collections::HashMap<String, Vec<String>>,
    near_matches: &std::collections::HashMap<DuplicateNearKey, Vec<String>>,
) -> Result<DuplicateStatus> {
    let mut matched_candidate_ids = Vec::new();
    let mut matched_canonical_transaction_ids = Vec::new();
    if let Some(candidate_ids) = exact_matches.get(&candidate.duplicate_status.fingerprint) {
        matched_candidate_ids.extend(
            candidate_ids
                .iter()
                .filter(|candidate_id| *candidate_id != &candidate.id)
                .cloned(),
        );
    }
    let existing_matches =
        existing_fingerprint_matches(connection, &candidate.duplicate_status.fingerprint)?;
    matched_candidate_ids.extend(existing_matches.matched_candidate_ids);
    matched_canonical_transaction_ids.extend(existing_matches.matched_canonical_transaction_ids);

    let mut status =
        if matched_candidate_ids.is_empty() && matched_canonical_transaction_ids.is_empty() {
            DuplicateStatusState::Unique
        } else {
            DuplicateStatusState::ExactDuplicate
        };

    if status == DuplicateStatusState::Unique {
        if let Some(candidate_ids) = near_matches.get(&DuplicateNearKey::from(candidate)) {
            matched_candidate_ids.extend(
                candidate_ids
                    .iter()
                    .filter(|candidate_id| *candidate_id != &candidate.id)
                    .cloned(),
            );
        }
        let near_existing_matches = existing_near_matches(connection, candidate)?;
        matched_candidate_ids.extend(near_existing_matches.matched_candidate_ids);
        matched_canonical_transaction_ids
            .extend(near_existing_matches.matched_canonical_transaction_ids);
        if !matched_candidate_ids.is_empty() || !matched_canonical_transaction_ids.is_empty() {
            status = DuplicateStatusState::PossibleDuplicate;
        }
    }

    let reason = match status {
        DuplicateStatusState::ExactDuplicate => {
            Some("normalized_transaction_fingerprint_matched".to_string())
        }
        DuplicateStatusState::PossibleDuplicate => {
            Some("normalized_transaction_fields_matched".to_string())
        }
        DuplicateStatusState::NotChecked | DuplicateStatusState::Unique => None,
    };

    Ok(DuplicateStatus {
        status,
        fingerprint: candidate.duplicate_status.fingerprint.clone(),
        matched_candidate_ids,
        matched_canonical_transaction_ids,
        reason,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DuplicateNearKey {
    account_key: String,
    posted_date: String,
    amount_minor: i64,
    currency: String,
}

impl From<&CandidateTransaction> for DuplicateNearKey {
    fn from(candidate: &CandidateTransaction) -> Self {
        Self {
            account_key: duplicate_account_key(candidate),
            posted_date: candidate.posted_date.clone(),
            amount_minor: candidate.amount_minor,
            currency: normalized_currency(candidate.currency),
        }
    }
}

struct ExistingFingerprintMatches {
    matched_candidate_ids: Vec<String>,
    matched_canonical_transaction_ids: Vec<String>,
}

fn existing_fingerprint_matches(
    connection: &Connection,
    fingerprint: &str,
) -> Result<ExistingFingerprintMatches> {
    let mut statement = connection.prepare(
        "SELECT candidate_transaction_id, canonical_transaction_id
         FROM transaction_fingerprints
         WHERE fingerprint = ?1",
    )?;
    let rows = statement.query_map(params![fingerprint], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, Option<String>>(1)?,
        ))
    })?;
    let mut matched_candidate_ids = Vec::new();
    let mut matched_canonical_transaction_ids = Vec::new();
    for row in rows {
        let (candidate_id, canonical_id) = row?;
        if let Some(candidate_id) = candidate_id {
            matched_candidate_ids.push(candidate_id);
        }
        if let Some(canonical_id) = canonical_id {
            matched_canonical_transaction_ids.push(canonical_id);
        }
    }
    Ok(ExistingFingerprintMatches {
        matched_candidate_ids,
        matched_canonical_transaction_ids,
    })
}

fn existing_near_matches(
    connection: &Connection,
    candidate: &CandidateTransaction,
) -> Result<ExistingFingerprintMatches> {
    let mut statement = connection.prepare(
        "SELECT tf.candidate_transaction_id, tf.canonical_transaction_id
         FROM transaction_fingerprints tf
         LEFT JOIN candidate_transactions c ON c.id = tf.candidate_transaction_id
         LEFT JOIN source_documents sd ON sd.id = c.source_document_id
         LEFT JOIN canonical_transactions ct ON ct.id = tf.canonical_transaction_id
         LEFT JOIN accounts candidate_account ON candidate_account.id = c.account_id
         LEFT JOIN institutions candidate_account_institution ON candidate_account_institution.id = candidate_account.institution_id
         LEFT JOIN accounts canonical_account ON canonical_account.id = ct.account_id
         LEFT JOIN institutions canonical_account_institution ON canonical_account_institution.id = canonical_account.institution_id
         WHERE (
             tf.normalized_account_key = ?1
             OR (
                 tf.normalized_account_key = ?2
                 AND (
                     LOWER(c.institution_hint) = ?3
                     OR LOWER(sd.institution_hint) = ?3
                     OR LOWER(candidate_account_institution.name) = ?3
                     OR LOWER(canonical_account_institution.name) = ?3
                 )
             )
         )
           AND tf.normalized_posted_date = ?4
           AND tf.normalized_amount_minor = ?5
           AND UPPER(tf.normalized_currency) = ?6
           AND tf.fingerprint != ?7",
    )?;
    let rows = statement.query_map(
        params![
            duplicate_account_key(candidate),
            legacy_duplicate_account_key(candidate),
            normalized_institution_key(&candidate.institution_hint),
            candidate.posted_date,
            candidate.amount_minor,
            normalized_currency(candidate.currency),
            candidate.duplicate_status.fingerprint,
        ],
        |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        },
    )?;
    let mut matched_candidate_ids = Vec::new();
    let mut matched_canonical_transaction_ids = Vec::new();
    for row in rows {
        let (candidate_id, canonical_id) = row?;
        if let Some(candidate_id) = candidate_id {
            matched_candidate_ids.push(candidate_id);
        }
        if let Some(canonical_id) = canonical_id {
            matched_canonical_transaction_ids.push(canonical_id);
        }
    }
    Ok(ExistingFingerprintMatches {
        matched_candidate_ids,
        matched_canonical_transaction_ids,
    })
}

fn is_duplicate_status(status: &DuplicateStatusState) -> bool {
    matches!(
        status,
        DuplicateStatusState::PossibleDuplicate | DuplicateStatusState::ExactDuplicate
    )
}

fn should_mark_candidate_possible_duplicate(
    candidate: &CandidateTransaction,
    duplicate_status: &DuplicateStatusState,
) -> bool {
    is_duplicate_status(duplicate_status)
        && !matches!(
            candidate.status,
            CandidateStatus::Accepted | CandidateStatus::Rejected
        )
}

fn insert_candidate(
    connection: &Connection,
    candidate: &CandidateTransaction,
    source: &SourceDocument,
    batch_id: &str,
    resolution: Option<&AccountResolution>,
) -> Result<()> {
    connection.execute(
        "INSERT INTO candidate_transactions (
            id, import_batch_id, source_document_id, institution_id, institution_hint,
            account_id, account_label_hint, account_currency_hint, account_masked_identifier_hint,
            posted_date, description, amount_minor, currency, balance_minor,
            direction_hint, semantic_hint, confidence, status, duplicate_status, fingerprint,
            validation_warnings_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
        params![
            candidate.id,
            batch_id,
            source.id,
            resolution.map(|value| value.institution_id.as_str()),
            candidate.institution_hint,
            resolution.map(|value| value.account_id.as_str()),
            candidate.account_hint.label,
            candidate.account_hint.currency,
            candidate.account_hint.masked_identifier,
            candidate.posted_date,
            candidate.description,
            candidate.amount_minor,
            candidate.currency,
            candidate.balance_minor,
            direction_hint_value(&candidate.direction_hint),
            semantic_hint_value(&candidate.semantic_hint),
            candidate.confidence as f64,
            candidate_status_value(&candidate.status),
            duplicate_status_value(&candidate.duplicate_status.status),
            candidate.duplicate_status.fingerprint,
            serde_json::to_string(&candidate.validation_warnings)
                .context("serializing validation warnings")?,
        ],
    )?;
    Ok(())
}

fn insert_provenance(
    connection: &Connection,
    candidate: &CandidateTransaction,
    source: &SourceDocument,
    batch_id: &str,
) -> Result<()> {
    let bbox = candidate.provenance.bbox;
    connection.execute(
        "INSERT INTO provenance (
            id, candidate_transaction_id, source_document_id, import_batch_id,
            page_number, row_index, bbox_x, bbox_y, bbox_width, bbox_height, bbox_unit,
            extractor_name, extractor_version, parser_id, parser_version,
            evidence_redaction, evidence_text_redacted, raw_storage_policy, confidence
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
        params![
            provenance_id(&candidate.id),
            candidate.id,
            source.id,
            batch_id,
            candidate.provenance.page_number as i64,
            candidate.provenance.row_index as i64,
            bbox.map(|value| value.x as f64),
            bbox.map(|value| value.y as f64),
            bbox.map(|value| value.width as f64),
            bbox.map(|value| value.height as f64),
            bbox.map(|value| value.unit),
            candidate.provenance.extractor.name,
            candidate.provenance.extractor.version,
            candidate.provenance.parser.id,
            candidate.provenance.parser.version,
            candidate.provenance.evidence.redaction,
            candidate.provenance.evidence.text,
            candidate.provenance.evidence.raw_storage_policy,
            candidate.provenance.confidence as f64,
        ],
    )?;
    Ok(())
}

fn insert_duplicate_markers(
    connection: &Connection,
    candidate: &CandidateTransaction,
) -> Result<()> {
    for matched_candidate_id in &candidate.duplicate_status.matched_candidate_ids {
        connection.execute(
            "INSERT INTO transaction_duplicate_markers (
                id, candidate_transaction_id, matched_candidate_transaction_id,
                duplicate_status, reason
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                duplicate_marker_id(&candidate.id, matched_candidate_id),
                candidate.id,
                matched_candidate_id,
                duplicate_status_value(&candidate.duplicate_status.status),
                candidate.duplicate_status.reason,
            ],
        )?;
    }
    for matched_canonical_id in &candidate.duplicate_status.matched_canonical_transaction_ids {
        connection.execute(
            "INSERT INTO transaction_duplicate_markers (
                id, candidate_transaction_id, matched_canonical_transaction_id,
                duplicate_status, reason
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                duplicate_marker_id(&candidate.id, matched_canonical_id),
                candidate.id,
                matched_canonical_id,
                duplicate_status_value(&candidate.duplicate_status.status),
                candidate.duplicate_status.reason,
            ],
        )?;
    }
    Ok(())
}

fn insert_fingerprint(connection: &Connection, candidate: &CandidateTransaction) -> Result<()> {
    connection.execute(
        "INSERT OR IGNORE INTO transaction_fingerprints (
            id, fingerprint, candidate_transaction_id, duplicate_status,
            normalized_account_key, normalized_posted_date, normalized_amount_minor,
            normalized_currency, normalized_description
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            fingerprint_row_id(&candidate.id),
            candidate.duplicate_status.fingerprint,
            candidate.id,
            duplicate_status_value(&candidate.duplicate_status.status),
            duplicate_account_key(candidate),
            candidate.posted_date,
            candidate.amount_minor,
            normalized_currency(candidate.currency),
            candidate.description.to_ascii_lowercase(),
        ],
    )?;
    Ok(())
}

fn institution_id(institution: &str) -> String {
    format!("inst_{}", stable_slug(institution))
}

fn account_id(input: &AccountRegisterInput) -> String {
    let digest = Sha256::digest(
        format!(
            "{}|{}|{}|{}|{}",
            normalize_match_key(&input.institution),
            normalize_match_key(&input.label),
            normalize_match_key(&input.account_type),
            normalized_currency(&input.currency),
            input
                .masked_identifier
                .as_deref()
                .map(normalize_match_key)
                .unwrap_or_default()
        )
        .as_bytes(),
    );
    format!(
        "acct_{}",
        digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn income_source_id(name: &str) -> String {
    format!("incsrc_{}", stable_slug(name))
}

fn category_id(name: &str) -> String {
    format!("cat_{}", stable_slug(name))
}

fn validate_income_source_input(input: &IncomeSourceCreateInput) -> Result<()> {
    if input.name.trim().is_empty() {
        anyhow::bail!("income source name is required");
    }
    Ok(())
}

fn validate_category_input(input: &CategoryCreateInput) -> Result<()> {
    if input.name.trim().is_empty() {
        anyhow::bail!("category name is required");
    }
    Ok(())
}

fn income_source_by_id(connection: &Connection, id: &str) -> Result<Option<IncomeSource>> {
    connection
        .query_row(
            "SELECT id, name FROM income_sources WHERE id = ?1",
            params![id],
            income_source_from_row,
        )
        .optional()
        .map_err(Into::into)
}

fn category_by_id(connection: &Connection, id: &str) -> Result<Option<Category>> {
    connection
        .query_row(
            "SELECT id, name FROM categories WHERE id = ?1",
            params![id],
            category_from_row,
        )
        .optional()
        .map_err(Into::into)
}

fn income_source_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<IncomeSource> {
    Ok(IncomeSource {
        id: row.get(0)?,
        name: row.get(1)?,
    })
}

fn category_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Category> {
    Ok(Category {
        id: row.get(0)?,
        name: row.get(1)?,
    })
}

fn stable_slug(value: &str) -> String {
    let slug = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if slug.is_empty() {
        "unknown".to_string()
    } else {
        slug
    }
}

fn normalize_match_key(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}

fn import_batch_id(content_sha256: &str) -> String {
    format!("batch_{}", &content_sha256[..26.min(content_sha256.len())])
}

fn provenance_id(candidate_id: &str) -> String {
    format!("prov_{}", candidate_id.trim_start_matches("cand_"))
}

fn duplicate_marker_id(candidate_id: &str, matched_id: &str) -> String {
    let digest = Sha256::digest(format!("{candidate_id}|{matched_id}").as_bytes());
    format!(
        "dup_{}",
        digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn duplicate_account_key(candidate: &CandidateTransaction) -> String {
    format!(
        "{}|{}",
        candidate.institution_hint.to_ascii_lowercase(),
        candidate.account_hint.label.to_ascii_lowercase()
    )
}

fn legacy_duplicate_account_key(candidate: &CandidateTransaction) -> String {
    candidate.account_hint.label.to_ascii_lowercase()
}

fn normalized_institution_key(institution: &str) -> String {
    institution.to_ascii_lowercase()
}

fn normalized_currency(currency: &str) -> String {
    currency.to_ascii_uppercase()
}

fn fingerprint_row_id(candidate_id: &str) -> String {
    let digest = Sha256::digest(candidate_id.as_bytes());
    format!(
        "fp_{}",
        digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn import_batch_status_value(status: &ImportBatchStatus) -> &'static str {
    match status {
        ImportBatchStatus::Completed => "completed",
        ImportBatchStatus::CompletedWithErrors => "completed_with_errors",
        ImportBatchStatus::Failed => "failed",
    }
}

fn candidate_status_value(status: &CandidateStatus) -> &'static str {
    match status {
        CandidateStatus::PendingReview => "pending_review",
        CandidateStatus::PossibleDuplicate => "possible_duplicate",
        CandidateStatus::Accepted => "accepted",
        CandidateStatus::Rejected => "rejected",
    }
}

fn duplicate_status_value(status: &DuplicateStatusState) -> &'static str {
    match status {
        DuplicateStatusState::NotChecked => "not_checked",
        DuplicateStatusState::Unique => "unique",
        DuplicateStatusState::PossibleDuplicate => "possible_duplicate",
        DuplicateStatusState::ExactDuplicate => "exact_duplicate",
    }
}

fn direction_hint_value(direction_hint: &DirectionHint) -> &'static str {
    match direction_hint {
        DirectionHint::Inflow => "inflow",
        DirectionHint::Outflow => "outflow",
    }
}

fn semantic_hint_value(semantic_hint: &SemanticHint) -> &'static str {
    match semantic_hint {
        SemanticHint::BankMovement => "bank_movement",
        SemanticHint::CardCharge => "card_charge",
        SemanticHint::CardPayment => "card_payment",
    }
}

pub fn list_review_candidates(
    connection: &Connection,
    import_batch_id: Option<&str>,
    status: Option<&str>,
) -> Result<Vec<ReviewCandidate>> {
    if let Some(status) = status {
        validate_candidate_status_filter(status)?;
    }
    let mut sql = review_candidate_select_sql();
    let mut filters = Vec::new();
    if import_batch_id.is_some() {
        filters.push("c.import_batch_id = ?");
    }
    if status.is_some() {
        filters.push("c.status = ?");
    }
    if !filters.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&filters.join(" AND "));
    }
    sql.push_str(" ORDER BY c.import_batch_id, p.row_index, c.id");

    let mut statement = connection.prepare(&sql)?;
    let rows = match (import_batch_id, status) {
        (Some(import_batch_id), Some(status)) => {
            statement.query_map(params![import_batch_id, status], review_candidate_from_row)?
        }
        (Some(import_batch_id), None) => {
            statement.query_map(params![import_batch_id], review_candidate_from_row)?
        }
        (None, Some(status)) => statement.query_map(params![status], review_candidate_from_row)?,
        (None, None) => statement.query_map([], review_candidate_from_row)?,
    };
    let mut candidates = Vec::new();
    for row in rows {
        let mut candidate = row?;
        hydrate_duplicate_matches(connection, &mut candidate)?;
        candidates.push(candidate);
    }
    Ok(candidates)
}

pub fn summarize_import_batch(
    connection: &Connection,
    import_batch_id: &str,
    largest_limit: usize,
) -> Result<BatchReviewResponse> {
    if largest_limit == 0 {
        return Ok(batch_review_error_response(
            "candidates batch-summary",
            Some(import_batch_id),
            "validation_failure",
            "invalid_largest_limit",
            "Largest amount limit must be greater than zero.",
            "largest_limit",
            serde_json::json!({ "largest_limit": largest_limit }),
        ));
    }
    if !import_batch_exists(connection, import_batch_id)? {
        return Ok(batch_not_found_response(
            "candidates batch-summary",
            import_batch_id,
        ));
    }
    let candidates = list_review_candidates(connection, Some(import_batch_id), None)?;
    let mut by_status = BTreeMap::new();
    let mut by_duplicate_status = BTreeMap::new();
    let mut by_institution = BTreeMap::new();
    let mut by_account_resolution = BTreeMap::new();
    let mut by_direction_hint = BTreeMap::new();
    let mut by_semantic_hint = BTreeMap::new();
    for candidate in &candidates {
        increment_group(&mut by_status, candidate_status_value(&candidate.status));
        increment_group(
            &mut by_duplicate_status,
            duplicate_status_value(&candidate.duplicate_status.status),
        );
        increment_group(
            &mut by_institution,
            candidate
                .institution_hint
                .as_deref()
                .or(candidate.institution_id.as_deref())
                .unwrap_or("unresolved"),
        );
        increment_group(
            &mut by_account_resolution,
            if candidate.account_id.is_some() {
                "resolved"
            } else {
                "unresolved"
            },
        );
        increment_group(
            &mut by_direction_hint,
            candidate.direction_hint.as_deref().unwrap_or("unknown"),
        );
        increment_group(
            &mut by_semantic_hint,
            candidate.semantic_hint.as_deref().unwrap_or("unknown"),
        );
    }
    let mut largest_amounts = candidates
        .iter()
        .map(|candidate| LargestCandidateAmount {
            candidate_id: candidate.id.clone(),
            posted_date: candidate.posted_date.clone(),
            description: candidate.description.clone(),
            amount_minor: candidate.amount_minor,
            absolute_amount_minor: candidate.amount_minor.unsigned_abs(),
            currency: candidate.currency.clone(),
            status: candidate.status.clone(),
        })
        .collect::<Vec<_>>();
    largest_amounts.sort_by(|left, right| {
        right
            .absolute_amount_minor
            .cmp(&left.absolute_amount_minor)
            .then(left.candidate_id.cmp(&right.candidate_id))
    });
    largest_amounts.truncate(largest_limit);
    Ok(BatchReviewResponse {
        schema_version: BATCH_REVIEW_SCHEMA_VERSION,
        command: "candidates batch-summary",
        ok: true,
        import_batch_id: Some(import_batch_id.to_string()),
        summary: Some(BatchSummary {
            total_candidates: candidates.len(),
            by_status: group_counts(by_status),
            by_duplicate_status: group_counts(by_duplicate_status),
            by_institution: group_counts(by_institution),
            by_account_resolution: group_counts(by_account_resolution),
            by_direction_hint: group_counts(by_direction_hint),
            by_semantic_hint: group_counts(by_semantic_hint),
            largest_amounts,
        }),
        comparison: None,
        suggestions: Vec::new(),
        dry_run: None,
        action_results: Vec::new(),
        errors: Vec::new(),
    })
}

pub fn compare_duplicate_candidate(
    connection: &Connection,
    candidate_id: &str,
) -> Result<BatchReviewResponse> {
    let Some(candidate) = find_review_candidate(connection, candidate_id)? else {
        return Ok(batch_review_error_response(
            "candidates compare-duplicate",
            None,
            "not_found",
            "candidate_not_found",
            "Candidate transaction was not found.",
            "candidate_id",
            serde_json::json!({ "candidate_id": candidate_id }),
        ));
    };
    let mut matched_candidates = Vec::new();
    for matched_id in &candidate.duplicate_status.matched_candidate_ids {
        if let Some(matched) = find_review_candidate(connection, matched_id)? {
            matched_candidates.push(matched);
        }
    }
    matched_candidates.sort_by(|left, right| left.id.cmp(&right.id));

    let mut matched_canonical_transactions = Vec::new();
    for matched_id in &candidate.duplicate_status.matched_canonical_transaction_ids {
        let Some(transaction) = canonical_transaction(connection, matched_id)? else {
            continue;
        };
        let matched_candidate = transaction
            .created_from_candidate_id
            .as_deref()
            .map(|id| find_review_candidate(connection, id))
            .transpose()?
            .flatten();
        let provenance =
            transaction_provenance_for(connection, &transaction.id, matched_candidate.as_ref())?;
        matched_canonical_transactions.push(MatchedCanonicalTransaction {
            transaction,
            candidate: matched_candidate,
            provenance,
        });
    }
    matched_canonical_transactions
        .sort_by(|left, right| left.transaction.id.cmp(&right.transaction.id));
    let fingerprints = relevant_fingerprints(
        connection,
        &candidate,
        &matched_candidates,
        &matched_canonical_transactions,
    )?;
    let duplicate_markers = relevant_duplicate_markers(connection, candidate_id)?;
    Ok(BatchReviewResponse {
        schema_version: BATCH_REVIEW_SCHEMA_VERSION,
        command: "candidates compare-duplicate",
        ok: true,
        import_batch_id: Some(candidate.import_batch_id.clone()),
        summary: None,
        comparison: Some(DuplicateComparison {
            candidate,
            matched_candidates,
            matched_canonical_transactions,
            fingerprints,
            duplicate_markers,
        }),
        suggestions: Vec::new(),
        dry_run: None,
        action_results: Vec::new(),
        errors: Vec::new(),
    })
}

pub fn suggest_batch_actions(
    connection: &Connection,
    import_batch_id: &str,
) -> Result<BatchReviewResponse> {
    if !import_batch_exists(connection, import_batch_id)? {
        return Ok(batch_not_found_response(
            "candidates suggest-actions",
            import_batch_id,
        ));
    }
    let candidates = list_review_candidates(connection, Some(import_batch_id), None)?;
    let all_candidates = list_review_candidates(connection, None, None)?;
    let mut suggestions = Vec::new();
    for candidate in &candidates {
        if is_obvious_unreviewed_duplicate(connection, candidate)? {
            let candidate_ids = vec![candidate.id.clone()];
            suggestions.push(ReviewSuggestion {
                id: review_suggestion_id("reject_duplicate", &candidate_ids),
                proposed_action: "reject_duplicate",
                candidate_ids,
                import_batch_ids: vec![candidate.import_batch_id.clone()],
                reason: "exact_fingerprint_matches_reviewed_record",
                evidence: serde_json::json!({
                    "duplicate_status": candidate.duplicate_status.status,
                    "fingerprint": candidate.duplicate_status.fingerprint,
                    "matched_candidate_ids": candidate.duplicate_status.matched_candidate_ids,
                    "matched_canonical_transaction_ids": candidate.duplicate_status.matched_canonical_transaction_ids,
                    "provenance": candidate.provenance,
                }),
            });
        }
    }
    for pair in likely_transfer_pairs_from_candidates(connection, &all_candidates)? {
        if pair.from_candidate.import_batch_id != import_batch_id
            && pair.to_candidate.import_batch_id != import_batch_id
        {
            continue;
        }
        let candidate_ids = vec![pair.from_candidate.id.clone(), pair.to_candidate.id.clone()];
        suggestions.push(ReviewSuggestion {
            id: review_suggestion_id("accept_transfer_pair", &candidate_ids),
            proposed_action: "accept_transfer_pair",
            candidate_ids,
            import_batch_ids: vec![
                pair.from_candidate.import_batch_id.clone(),
                pair.to_candidate.import_batch_id.clone(),
            ],
            reason: "owned_accounts_and_transfer_fields_match",
            evidence: serde_json::json!({
                "posted_date": pair.posted_date,
                "amount_minor": pair.amount_minor,
                "currency": pair.currency,
                "from_account_id": pair.from_account.id,
                "to_account_id": pair.to_account.id,
                "from_semantic_hint": pair.from_candidate.semantic_hint,
                "to_semantic_hint": pair.to_candidate.semantic_hint,
                "from_provenance": pair.from_candidate.provenance,
                "to_provenance": pair.to_candidate.provenance,
            }),
        });
    }
    suggestions.sort_by(|left, right| {
        left.proposed_action
            .cmp(right.proposed_action)
            .then(left.candidate_ids.cmp(&right.candidate_ids))
    });
    suggestions.dedup_by(|left, right| left.id == right.id);
    Ok(BatchReviewResponse {
        schema_version: BATCH_REVIEW_SCHEMA_VERSION,
        command: "candidates suggest-actions",
        ok: true,
        import_batch_id: Some(import_batch_id.to_string()),
        summary: None,
        comparison: None,
        suggestions,
        dry_run: None,
        action_results: Vec::new(),
        errors: Vec::new(),
    })
}

pub fn apply_batch_actions(
    connection: &mut Connection,
    actions: &[BatchActionRequest],
    dry_run: bool,
) -> Result<BatchReviewResponse> {
    if actions.is_empty() {
        return Ok(batch_review_error_response_with_dry_run(
            "candidates apply-actions",
            "validation_failure",
            "actions_required",
            "At least one explicit --action with candidate ids is required.",
            "actions",
            serde_json::json!({}),
            dry_run,
        ));
    }
    if dry_run {
        let (_, action_results) = prepare_batch_actions(connection, actions)?;
        return Ok(batch_action_response(true, action_results));
    }

    let tx = connection
        .transaction()
        .context("starting atomic batch review transaction")?;
    let (prepared, mut action_results) = prepare_batch_actions(&tx, actions)?;
    if action_results
        .iter()
        .any(|result| !result.errors.is_empty())
    {
        return Ok(batch_action_response(false, action_results));
    }
    for (prepared, result) in prepared.into_iter().zip(action_results.iter_mut()) {
        match prepared.mutation {
            BatchActionMutation::RejectDuplicate { candidate_id } => {
                tx.execute(
                    "UPDATE candidate_transactions SET status = 'rejected' WHERE id = ?1",
                    params![candidate_id],
                )?;
            }
            BatchActionMutation::AcceptTransferPair { pair } => {
                result.canonical_transaction_ids = apply_transfer_pair_rows(&tx, &pair)?;
            }
        }
        result.status = "applied";
    }
    tx.commit().context("committing atomic batch review")?;
    Ok(batch_action_response(false, action_results))
}

fn prepare_batch_actions(
    connection: &Connection,
    actions: &[BatchActionRequest],
) -> Result<(Vec<PreparedBatchAction>, Vec<BatchActionResult>)> {
    let mut seen_candidate_ids = HashSet::new();
    let mut prepared_actions = Vec::new();
    let mut action_results = Vec::new();
    for action in actions {
        let candidate_ids = action.candidate_ids().to_vec();
        let reused_id = candidate_ids
            .iter()
            .find(|candidate_id| !seen_candidate_ids.insert((*candidate_id).clone()))
            .cloned();
        let prepared = if let Some(candidate_id) = reused_id {
            Err(vec![ReviewError {
                category: "validation_failure",
                code: "candidate_reused_in_batch",
                message: "A candidate id may appear in only one batch action.".to_string(),
                path: "actions",
                recoverable: true,
                details: serde_json::json!({ "candidate_id": candidate_id }),
            }])
        } else {
            preflight_batch_action(connection, action)?
        };
        let (result_action, result_candidate_ids, errors) = match prepared {
            Ok(prepared) => {
                let result_action = prepared.action;
                let result_candidate_ids = prepared.candidate_ids.clone();
                prepared_actions.push(prepared);
                (result_action, result_candidate_ids, Vec::new())
            }
            Err(errors) => (action.action(), candidate_ids, errors),
        };
        action_results.push(BatchActionResult {
            action: result_action,
            candidate_ids: result_candidate_ids,
            status: if errors.is_empty() {
                "validated"
            } else {
                "failed"
            },
            canonical_transaction_ids: Vec::new(),
            errors,
        });
    }
    Ok((prepared_actions, action_results))
}

fn batch_action_response(
    dry_run: bool,
    action_results: Vec<BatchActionResult>,
) -> BatchReviewResponse {
    if action_results
        .iter()
        .any(|result| !result.errors.is_empty())
    {
        return BatchReviewResponse {
            schema_version: BATCH_REVIEW_SCHEMA_VERSION,
            command: "candidates apply-actions",
            ok: false,
            import_batch_id: None,
            summary: None,
            comparison: None,
            suggestions: Vec::new(),
            dry_run: Some(dry_run),
            action_results,
            errors: vec![ReviewError {
                category: "conflict",
                code: "batch_preflight_failed",
                message: "No actions were applied because at least one action failed validation."
                    .to_string(),
                path: "actions",
                recoverable: true,
                details: serde_json::json!({ "atomic": true }),
            }],
        };
    }
    BatchReviewResponse {
        schema_version: BATCH_REVIEW_SCHEMA_VERSION,
        command: "candidates apply-actions",
        ok: true,
        import_batch_id: None,
        summary: None,
        comparison: None,
        suggestions: Vec::new(),
        dry_run: Some(dry_run),
        action_results,
        errors: Vec::new(),
    }
}

fn import_batch_exists(connection: &Connection, import_batch_id: &str) -> Result<bool> {
    connection
        .query_row(
            "SELECT 1 FROM import_batches WHERE id = ?1",
            params![import_batch_id],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(Into::into)
}

fn increment_group(groups: &mut BTreeMap<String, usize>, key: &str) {
    *groups.entry(key.to_string()).or_default() += 1;
}

fn group_counts(groups: BTreeMap<String, usize>) -> Vec<GroupCount> {
    groups
        .into_iter()
        .map(|(key, count)| GroupCount { key, count })
        .collect()
}

fn relevant_fingerprints(
    connection: &Connection,
    candidate: &ReviewCandidate,
    matched_candidates: &[ReviewCandidate],
    matched_canonical_transactions: &[MatchedCanonicalTransaction],
) -> Result<Vec<ReviewFingerprint>> {
    let candidate_ids = matched_candidates
        .iter()
        .map(|candidate| candidate.id.as_str())
        .chain(std::iter::once(candidate.id.as_str()))
        .collect::<HashSet<_>>();
    let canonical_ids = matched_canonical_transactions
        .iter()
        .map(|matched| matched.transaction.id.as_str())
        .collect::<HashSet<_>>();
    let mut statement = connection.prepare(
        "SELECT id, fingerprint, candidate_transaction_id, canonical_transaction_id,
                duplicate_status, normalized_account_key, normalized_posted_date,
                normalized_amount_minor, normalized_currency, normalized_description
         FROM transaction_fingerprints
         ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(ReviewFingerprint {
            id: row.get(0)?,
            fingerprint: row.get(1)?,
            candidate_transaction_id: row.get(2)?,
            canonical_transaction_id: row.get(3)?,
            duplicate_status: row.get(4)?,
            normalized_account_key: row.get(5)?,
            normalized_posted_date: row.get(6)?,
            normalized_amount_minor: row.get(7)?,
            normalized_currency: row.get(8)?,
            normalized_description: row.get(9)?,
        })
    })?;
    let target_fingerprint = candidate.duplicate_status.fingerprint.as_deref();
    let mut fingerprints = Vec::new();
    for row in rows {
        let fingerprint = row?;
        if target_fingerprint == Some(fingerprint.fingerprint.as_str())
            || fingerprint
                .candidate_transaction_id
                .as_deref()
                .is_some_and(|id| candidate_ids.contains(id))
            || fingerprint
                .canonical_transaction_id
                .as_deref()
                .is_some_and(|id| canonical_ids.contains(id))
        {
            fingerprints.push(fingerprint);
        }
    }
    Ok(fingerprints)
}

fn relevant_duplicate_markers(
    connection: &Connection,
    candidate_id: &str,
) -> Result<Vec<ReviewDuplicateMarker>> {
    let mut statement = connection.prepare(
        "SELECT id, candidate_transaction_id, matched_candidate_transaction_id,
                matched_canonical_transaction_id, duplicate_status, reason
         FROM transaction_duplicate_markers
         WHERE candidate_transaction_id = ?1 OR matched_candidate_transaction_id = ?1
         ORDER BY id",
    )?;
    let rows = statement.query_map(params![candidate_id], |row| {
        Ok(ReviewDuplicateMarker {
            id: row.get(0)?,
            candidate_transaction_id: row.get(1)?,
            matched_candidate_transaction_id: row.get(2)?,
            matched_canonical_transaction_id: row.get(3)?,
            duplicate_status: row.get(4)?,
            reason: row.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn is_obvious_unreviewed_duplicate(
    connection: &Connection,
    candidate: &ReviewCandidate,
) -> Result<bool> {
    if !is_unreviewed_candidate_status(&candidate.status)
        || candidate.duplicate_status.status != DuplicateStatusState::ExactDuplicate
    {
        return Ok(false);
    }
    if !candidate
        .duplicate_status
        .matched_canonical_transaction_ids
        .is_empty()
    {
        return Ok(true);
    }
    for matched_id in &candidate.duplicate_status.matched_candidate_ids {
        if find_review_candidate(connection, matched_id)?
            .is_some_and(|matched| matched.status == CandidateStatus::Accepted)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn review_suggestion_id(action: &str, candidate_ids: &[String]) -> String {
    let digest = Sha256::digest(format!("{action}|{}", candidate_ids.join("|")).as_bytes());
    format!(
        "suggest_{}",
        digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn preflight_batch_action(
    connection: &Connection,
    action: &BatchActionRequest,
) -> Result<std::result::Result<PreparedBatchAction, Vec<ReviewError>>> {
    match action.kind {
        BatchActionKind::RejectDuplicate => {
            let candidate_id = &action.candidate_ids[0];
            let Some(candidate) = find_review_candidate(connection, candidate_id)? else {
                return Ok(Err(vec![candidate_not_found_error(
                    candidate_id,
                    "candidate_id",
                )]));
            };
            if let Some(error) = candidate_rejection_error(&candidate) {
                return Ok(Err(vec![error]));
            }
            if !is_obvious_unreviewed_duplicate(connection, &candidate)? {
                return Ok(Err(vec![ReviewError {
                    category: "conflict",
                    code: "candidate_not_obvious_duplicate",
                    message: "Reject-duplicate requires an exact fingerprint match to a canonical or accepted record."
                        .to_string(),
                    path: "candidate.duplicate_status",
                    recoverable: true,
                    details: serde_json::json!({
                        "candidate_id": candidate_id,
                        "duplicate_status": candidate.duplicate_status.status,
                        "matched_candidate_ids": candidate.duplicate_status.matched_candidate_ids,
                        "matched_canonical_transaction_ids": candidate.duplicate_status.matched_canonical_transaction_ids,
                    }),
                }]));
            }
            Ok(Ok(PreparedBatchAction {
                action: action.action(),
                candidate_ids: action.candidate_ids().to_vec(),
                mutation: BatchActionMutation::RejectDuplicate {
                    candidate_id: candidate_id.clone(),
                },
            }))
        }
        BatchActionKind::AcceptTransferPair => {
            let from_candidate_id = &action.candidate_ids[0];
            let to_candidate_id = &action.candidate_ids[1];
            let Some(from_candidate) = find_review_candidate(connection, from_candidate_id)? else {
                return Ok(Err(vec![candidate_not_found_error(
                    from_candidate_id,
                    "from_candidate_id",
                )]));
            };
            let Some(to_candidate) = find_review_candidate(connection, to_candidate_id)? else {
                return Ok(Err(vec![candidate_not_found_error(
                    to_candidate_id,
                    "to_candidate_id",
                )]));
            };
            match build_transfer_pair(connection, from_candidate, to_candidate) {
                Ok(pair) => Ok(Ok(PreparedBatchAction {
                    action: action.action(),
                    candidate_ids: action.candidate_ids().to_vec(),
                    mutation: BatchActionMutation::AcceptTransferPair { pair },
                })),
                Err(error) => Ok(Err(vec![ReviewError {
                    category: "conflict",
                    code: error.code(),
                    message: "Candidates do not form an eligible own-account card payment pair."
                        .to_string(),
                    path: "candidate_ids",
                    recoverable: true,
                    details: serde_json::json!({
                        "from_candidate_id": from_candidate_id,
                        "to_candidate_id": to_candidate_id,
                        "reason": error.reason(),
                    }),
                }])),
            }
        }
    }
}

fn candidate_not_found_error(candidate_id: &str, path: &'static str) -> ReviewError {
    ReviewError {
        category: "not_found",
        code: "candidate_not_found",
        message: "Candidate transaction was not found.".to_string(),
        path,
        recoverable: true,
        details: serde_json::json!({ "candidate_id": candidate_id }),
    }
}

fn batch_not_found_response(command: &'static str, import_batch_id: &str) -> BatchReviewResponse {
    batch_review_error_response(
        command,
        Some(import_batch_id),
        "not_found",
        "import_batch_not_found",
        "Import batch was not found.",
        "import_batch_id",
        serde_json::json!({ "import_batch_id": import_batch_id }),
    )
}

pub fn batch_review_error_response(
    command: &'static str,
    import_batch_id: Option<&str>,
    category: &'static str,
    code: &'static str,
    message: &str,
    path: &'static str,
    details: serde_json::Value,
) -> BatchReviewResponse {
    BatchReviewResponse {
        schema_version: BATCH_REVIEW_SCHEMA_VERSION,
        command,
        ok: false,
        import_batch_id: import_batch_id.map(ToString::to_string),
        summary: None,
        comparison: None,
        suggestions: Vec::new(),
        dry_run: None,
        action_results: Vec::new(),
        errors: vec![ReviewError {
            category,
            code,
            message: message.to_string(),
            path,
            recoverable: true,
            details,
        }],
    }
}

pub fn batch_review_error_response_with_dry_run(
    command: &'static str,
    category: &'static str,
    code: &'static str,
    message: &str,
    path: &'static str,
    details: serde_json::Value,
    dry_run: bool,
) -> BatchReviewResponse {
    let mut response =
        batch_review_error_response(command, None, category, code, message, path, details);
    response.dry_run = Some(dry_run);
    response
}

pub fn list_likely_transfer_pairs(connection: &Connection) -> Result<TransferReviewResponse> {
    let candidates = list_review_candidates(connection, None, None)?;
    let pairs = likely_transfer_pairs_from_candidates(connection, &candidates)?;
    Ok(TransferReviewResponse {
        schema_version: TRANSFER_REVIEW_SCHEMA_VERSION,
        command: "candidates list-transfer-pairs",
        ok: true,
        transfer_pair: None,
        transfer_pairs: pairs,
        canonical_transactions: Vec::new(),
        errors: Vec::new(),
    })
}

pub fn accept_transfer_pair(
    connection: &mut Connection,
    from_candidate_id: &str,
    to_candidate_id: &str,
) -> Result<TransferReviewResponse> {
    let tx = connection
        .transaction()
        .context("starting transfer pair accept transaction")?;
    let Some(from_candidate) = find_review_candidate(&tx, from_candidate_id)? else {
        return Ok(transfer_error_response(
            "candidates accept-transfer-pair",
            "not_found",
            "candidate_not_found",
            "From candidate transaction was not found.".to_string(),
            "from_candidate_id",
            true,
            serde_json::json!({ "candidate_id": from_candidate_id }),
        ));
    };
    let Some(to_candidate) = find_review_candidate(&tx, to_candidate_id)? else {
        return Ok(transfer_error_response(
            "candidates accept-transfer-pair",
            "not_found",
            "candidate_not_found",
            "To candidate transaction was not found.".to_string(),
            "to_candidate_id",
            true,
            serde_json::json!({ "candidate_id": to_candidate_id }),
        ));
    };
    let eligible_pair = match build_transfer_pair(&tx, from_candidate, to_candidate) {
        Ok(pair) => pair,
        Err(error) => {
            return Ok(transfer_error_response(
                "candidates accept-transfer-pair",
                "conflict",
                error.code(),
                "Candidates do not form an eligible own-account card payment pair.".to_string(),
                "candidate_ids",
                true,
                serde_json::json!({
                    "from_candidate_id": from_candidate_id,
                    "to_candidate_id": to_candidate_id,
                    "reason": error.reason()
                }),
            ));
        }
    };

    let canonical_transaction_ids = apply_transfer_pair_rows(&tx, &eligible_pair)?;
    let from_canonical_id = canonical_transaction_ids[0].clone();
    let to_canonical_id = canonical_transaction_ids[1].clone();
    tx.commit().context("committing transfer pair accept")?;

    let from_candidate = find_review_candidate(connection, from_candidate_id)?
        .expect("accepted from candidate remains queryable");
    let to_candidate = find_review_candidate(connection, to_candidate_id)?
        .expect("accepted to candidate remains queryable");
    let from_account = owned_account_by_id(
        connection,
        from_candidate
            .account_id
            .as_deref()
            .expect("accepted from candidate has account"),
    )?
    .expect("accepted from account remains owned");
    let to_account = owned_account_by_id(
        connection,
        to_candidate
            .account_id
            .as_deref()
            .expect("accepted to candidate has account"),
    )?
    .expect("accepted to account remains owned");
    let transfer_pair = ReviewTransferPair {
        id: transfer_pair_id(from_candidate_id, to_candidate_id),
        transfer_kind: CARD_PAYMENT_TRANSFER_KIND,
        posted_date: from_candidate.posted_date.clone(),
        amount_minor: from_candidate.amount_minor.abs(),
        currency: from_candidate.currency.clone(),
        from_account,
        to_account,
        from_candidate,
        to_candidate,
        canonical_transaction_ids: vec![from_canonical_id.clone(), to_canonical_id.clone()],
    };
    let canonical_transactions = vec![
        canonical_transaction(connection, &from_canonical_id)?
            .expect("from canonical transaction remains queryable"),
        canonical_transaction(connection, &to_canonical_id)?
            .expect("to canonical transaction remains queryable"),
    ];
    Ok(TransferReviewResponse {
        schema_version: TRANSFER_REVIEW_SCHEMA_VERSION,
        command: "candidates accept-transfer-pair",
        ok: true,
        transfer_pair: Some(transfer_pair),
        transfer_pairs: Vec::new(),
        canonical_transactions,
        errors: Vec::new(),
    })
}

pub fn accept_income_candidate(
    connection: &mut Connection,
    candidate_id: &str,
    income_source_id: &str,
    income_kind: &str,
) -> Result<CandidateReviewResponse> {
    let normalized_income_kind = normalize_income_kind(income_kind);
    if !INCOME_KINDS.contains(&normalized_income_kind.as_str()) {
        return Ok(review_error_response(
            "candidates accept-income",
            "validation_failure",
            "invalid_income_kind",
            "Income kind is not supported.".to_string(),
            "income_kind",
            true,
            serde_json::json!({
                "income_kind": income_kind,
                "allowed_income_kinds": INCOME_KINDS,
            }),
        ));
    }

    let tx = connection
        .transaction()
        .context("starting income candidate accept transaction")?;
    let Some(candidate) = find_review_candidate(&tx, candidate_id)? else {
        return Ok(review_error_response(
            "candidates accept-income",
            "not_found",
            "candidate_not_found",
            "Candidate transaction was not found.".to_string(),
            "candidate_id",
            true,
            serde_json::json!({ "candidate_id": candidate_id }),
        ));
    };
    if candidate.status == CandidateStatus::Accepted || candidate.canonical_transaction_id.is_some()
    {
        return Ok(review_error_response(
            "candidates accept-income",
            "conflict",
            "candidate_already_accepted",
            "Candidate transaction was already accepted.".to_string(),
            "candidate.status",
            true,
            serde_json::json!({
                "candidate_id": candidate_id,
                "canonical_transaction_id": candidate.canonical_transaction_id,
            }),
        ));
    }
    if candidate.status == CandidateStatus::Rejected {
        return Ok(review_error_response(
            "candidates accept-income",
            "conflict",
            "candidate_already_rejected",
            "Rejected candidates cannot be accepted as income.".to_string(),
            "candidate.status",
            true,
            serde_json::json!({ "candidate_id": candidate_id }),
        ));
    }
    if !is_unreviewed_candidate_status(&candidate.status) {
        return Ok(review_error_response(
            "candidates accept-income",
            "conflict",
            "candidate_not_acceptable",
            "Only pending_review or possible_duplicate candidates can be accepted as income."
                .to_string(),
            "candidate.status",
            true,
            serde_json::json!({ "candidate_id": candidate_id, "status": candidate.status }),
        ));
    }
    if income_source_by_id(&tx, income_source_id)?.is_none() {
        return Ok(review_error_response(
            "candidates accept-income",
            "not_found",
            "income_source_not_found",
            "Income source was not found.".to_string(),
            "income_source_id",
            true,
            serde_json::json!({ "income_source_id": income_source_id }),
        ));
    }
    if !is_income_candidate_shape(&candidate) {
        return Ok(review_error_response(
            "candidates accept-income",
            "conflict",
            "candidate_not_income_eligible",
            "Only explicit bank-movement inflow candidates can be accepted as income.".to_string(),
            "candidate",
            true,
            serde_json::json!({
                "candidate_id": candidate_id,
                "direction_hint": candidate.direction_hint,
                "semantic_hint": candidate.semantic_hint,
                "amount_minor": candidate.amount_minor,
            }),
        ));
    }
    if has_matching_owned_account_outflow(&tx, &candidate)? {
        return Ok(review_error_response(
            "candidates accept-income",
            "conflict",
            "candidate_possible_own_account_transfer",
            "Candidate resembles an own-account transfer and must not be accepted as income."
                .to_string(),
            "candidate",
            true,
            serde_json::json!({ "candidate_id": candidate_id }),
        ));
    }

    let canonical_id = canonical_transaction_id(candidate_id);
    tx.execute(
        "INSERT INTO canonical_transactions (
            id, account_id, posted_date, description, amount_minor, currency,
            balance_minor, transaction_kind, income_source_id, income_kind, created_from_candidate_id
         )
         SELECT ?1, account_id, posted_date, description, amount_minor, currency,
                balance_minor, ?2, ?3, ?4, id
         FROM candidate_transactions
         WHERE id = ?5",
        params![
            canonical_id,
            INCOME_TRANSACTION_KIND,
            income_source_id,
            normalized_income_kind,
            candidate_id
        ],
    )?;
    mark_candidate_accepted_with_canonical(&tx, candidate_id, &canonical_id)?;
    tx.commit().context("committing income candidate accept")?;

    let candidate = find_review_candidate(connection, candidate_id)?
        .expect("accepted income candidate remains queryable");
    let canonical_transaction = canonical_transaction(connection, &canonical_id)?
        .expect("accepted income canonical transaction remains queryable");
    Ok(CandidateReviewResponse {
        schema_version: CANDIDATE_REVIEW_SCHEMA_VERSION,
        command: "candidates accept-income",
        ok: true,
        candidate: Some(candidate),
        candidates: Vec::new(),
        canonical_transaction: Some(canonical_transaction),
        transaction_lines: Vec::new(),
        errors: Vec::new(),
    })
}

pub fn accept_expense_candidate(
    connection: &mut Connection,
    candidate_id: &str,
    expense_lines: &[ExpenseLineInput],
) -> Result<CandidateReviewResponse> {
    let tx = connection
        .transaction()
        .context("starting expense candidate accept transaction")?;
    let Some(candidate) = find_review_candidate(&tx, candidate_id)? else {
        return Ok(review_error_response(
            "candidates accept-expense",
            "not_found",
            "candidate_not_found",
            "Candidate transaction was not found.".to_string(),
            "candidate_id",
            true,
            serde_json::json!({ "candidate_id": candidate_id }),
        ));
    };
    if candidate.status == CandidateStatus::Accepted || candidate.canonical_transaction_id.is_some()
    {
        return Ok(review_error_response(
            "candidates accept-expense",
            "conflict",
            "candidate_already_accepted",
            "Candidate transaction was already accepted.".to_string(),
            "candidate.status",
            true,
            serde_json::json!({
                "candidate_id": candidate_id,
                "canonical_transaction_id": candidate.canonical_transaction_id,
            }),
        ));
    }
    if candidate.status == CandidateStatus::Rejected {
        return Ok(review_error_response(
            "candidates accept-expense",
            "conflict",
            "candidate_already_rejected",
            "Rejected candidates cannot be accepted as an expense.".to_string(),
            "candidate.status",
            true,
            serde_json::json!({ "candidate_id": candidate_id }),
        ));
    }
    if !is_unreviewed_candidate_status(&candidate.status) {
        return Ok(review_error_response(
            "candidates accept-expense",
            "conflict",
            "candidate_not_acceptable",
            "Only pending_review or possible_duplicate candidates can be accepted as expenses."
                .to_string(),
            "candidate.status",
            true,
            serde_json::json!({ "candidate_id": candidate_id, "status": candidate.status }),
        ));
    }
    if !is_expense_candidate_shape(&candidate) {
        return Ok(review_error_response(
            "candidates accept-expense",
            "conflict",
            "candidate_not_expense_eligible",
            "Only explicit purchase/outflow candidates can be accepted as expenses.".to_string(),
            "candidate",
            true,
            serde_json::json!({
                "candidate_id": candidate_id,
                "direction_hint": candidate.direction_hint,
                "semantic_hint": candidate.semantic_hint,
                "amount_minor": candidate.amount_minor,
            }),
        ));
    }
    if has_matching_owned_account_counterparty_candidate(&tx, &candidate)? {
        return Ok(review_error_response(
            "candidates accept-expense",
            "conflict",
            "candidate_possible_own_account_transfer",
            "Candidate resembles an own-account transfer and must not be accepted as an expense."
                .to_string(),
            "candidate",
            true,
            serde_json::json!({ "candidate_id": candidate_id }),
        ));
    }

    let canonical_id = canonical_transaction_id(candidate_id);
    let expense_amount_minor = expense_amount_minor(&candidate);
    let expense_lines = normalized_expense_lines(
        expense_lines,
        expense_amount_minor,
        candidate.currency.as_str(),
    );
    if let Some(response) = validate_expense_lines(
        &tx,
        "candidates accept-expense",
        expense_amount_minor,
        candidate.currency.as_str(),
        &expense_lines,
    )? {
        return Ok(response);
    }
    tx.execute(
        "INSERT INTO canonical_transactions (
            id, account_id, posted_date, description, amount_minor, currency,
            balance_minor, transaction_kind, created_from_candidate_id
         )
         SELECT ?1, account_id, posted_date, description, ?2, currency,
                balance_minor, ?3, id
         FROM candidate_transactions
         WHERE id = ?4",
        params![
            canonical_id,
            expense_amount_minor,
            EXPENSE_TRANSACTION_KIND,
            candidate_id
        ],
    )?;
    insert_expense_lines(&tx, &canonical_id, &expense_lines)?;
    mark_candidate_accepted_with_canonical(&tx, candidate_id, &canonical_id)?;
    tx.commit().context("committing expense candidate accept")?;

    let candidate = find_review_candidate(connection, candidate_id)?
        .expect("accepted expense candidate remains queryable");
    let canonical_transaction = canonical_transaction(connection, &canonical_id)?
        .expect("accepted expense canonical transaction remains queryable");
    let transaction_lines = transaction_lines_for_canonical(connection, &canonical_id)?;
    Ok(CandidateReviewResponse {
        schema_version: CANDIDATE_REVIEW_SCHEMA_VERSION,
        command: "candidates accept-expense",
        ok: true,
        candidate: Some(candidate),
        candidates: Vec::new(),
        canonical_transaction: Some(canonical_transaction),
        transaction_lines,
        errors: Vec::new(),
    })
}

pub fn replace_expense_transaction_lines(
    connection: &mut Connection,
    candidate_id: &str,
    expense_lines: &[ExpenseLineInput],
) -> Result<CandidateReviewResponse> {
    let tx = connection
        .transaction()
        .context("starting expense line update transaction")?;
    let Some(candidate) = find_review_candidate(&tx, candidate_id)? else {
        return Ok(review_error_response(
            "candidates set-expense-lines",
            "not_found",
            "candidate_not_found",
            "Candidate transaction was not found.".to_string(),
            "candidate_id",
            true,
            serde_json::json!({ "candidate_id": candidate_id }),
        ));
    };
    let Some(canonical_id) = candidate.canonical_transaction_id.as_deref() else {
        return Ok(review_error_response(
            "candidates set-expense-lines",
            "conflict",
            "candidate_not_accepted_as_expense",
            "Only an accepted expense candidate can have its lines updated.".to_string(),
            "candidate.status",
            true,
            serde_json::json!({ "candidate_id": candidate_id, "status": candidate.status }),
        ));
    };
    let Some(canonical) = canonical_transaction(&tx, canonical_id)? else {
        return Ok(review_error_response(
            "candidates set-expense-lines",
            "not_found",
            "canonical_transaction_not_found",
            "Canonical transaction was not found.".to_string(),
            "candidate.canonical_transaction_id",
            true,
            serde_json::json!({ "candidate_id": candidate_id, "canonical_transaction_id": canonical_id }),
        ));
    };
    if canonical.transaction_kind.as_deref() != Some(EXPENSE_TRANSACTION_KIND) {
        return Ok(review_error_response(
            "candidates set-expense-lines",
            "conflict",
            "canonical_transaction_not_expense",
            "Only canonical expense transactions can have expense lines.".to_string(),
            "canonical_transaction.transaction_kind",
            true,
            serde_json::json!({ "canonical_transaction_id": canonical_id, "transaction_kind": canonical.transaction_kind }),
        ));
    }
    if let Some(response) = validate_expense_lines(
        &tx,
        "candidates set-expense-lines",
        canonical.amount_minor,
        &canonical.currency,
        expense_lines,
    )? {
        return Ok(response);
    }
    tx.execute(
        "DELETE FROM transaction_lines WHERE canonical_transaction_id = ?1",
        params![canonical_id],
    )?;
    insert_expense_lines(&tx, canonical_id, expense_lines)?;
    tx.commit().context("committing expense line update")?;

    let candidate = find_review_candidate(connection, candidate_id)?
        .expect("expense line candidate remains queryable");
    let canonical_transaction = canonical_transaction(connection, canonical_id)?
        .expect("expense line canonical transaction remains queryable");
    let transaction_lines = transaction_lines_for_canonical(connection, canonical_id)?;
    Ok(CandidateReviewResponse {
        schema_version: CANDIDATE_REVIEW_SCHEMA_VERSION,
        command: "candidates set-expense-lines",
        ok: true,
        candidate: Some(candidate),
        candidates: Vec::new(),
        canonical_transaction: Some(canonical_transaction),
        transaction_lines,
        errors: Vec::new(),
    })
}

pub fn accept_candidate(
    connection: &mut Connection,
    candidate_id: &str,
) -> Result<CandidateReviewResponse> {
    let tx = connection
        .transaction()
        .context("starting candidate accept transaction")?;
    let Some(candidate) = find_review_candidate(&tx, candidate_id)? else {
        return Ok(review_error_response(
            "candidates accept",
            "not_found",
            "candidate_not_found",
            "Candidate transaction was not found.".to_string(),
            "candidate_id",
            true,
            serde_json::json!({ "candidate_id": candidate_id }),
        ));
    };
    if candidate.status == CandidateStatus::Accepted || candidate.canonical_transaction_id.is_some()
    {
        return Ok(review_error_response(
            "candidates accept",
            "conflict",
            "candidate_already_accepted",
            "Candidate transaction was already accepted.".to_string(),
            "candidate.status",
            true,
            serde_json::json!({
                "candidate_id": candidate_id,
                "canonical_transaction_id": candidate.canonical_transaction_id,
            }),
        ));
    }
    if !matches!(
        candidate.status,
        CandidateStatus::PendingReview | CandidateStatus::PossibleDuplicate
    ) {
        return Ok(review_error_response(
            "candidates accept",
            "conflict",
            "candidate_not_acceptable",
            "Only pending_review or possible_duplicate candidates can be accepted.".to_string(),
            "candidate.status",
            true,
            serde_json::json!({ "candidate_id": candidate_id, "status": candidate.status }),
        ));
    }
    if is_income_candidate_shape(&candidate) {
        return Ok(review_error_response(
            "candidates accept",
            "conflict",
            "candidate_requires_income_review",
            "Inflow candidates require explicit income source and kind metadata.".to_string(),
            "candidate",
            true,
            serde_json::json!({
                "candidate_id": candidate_id,
                "direction_hint": candidate.direction_hint,
                "semantic_hint": candidate.semantic_hint,
                "required_command": "candidates accept-income",
            }),
        ));
    }
    if candidate.semantic_hint.as_deref() == Some("card_payment")
        || has_matching_owned_account_counterparty_candidate(&tx, &candidate)?
    {
        return Ok(review_error_response(
            "candidates accept",
            "conflict",
            "candidate_requires_transfer_pair_review",
            "Transfer-like candidates require an explicit validated transfer pair.".to_string(),
            "candidate",
            true,
            serde_json::json!({
                "candidate_id": candidate_id,
                "direction_hint": candidate.direction_hint,
                "semantic_hint": candidate.semantic_hint,
                "required_command": "candidates accept-transfer-pair",
            }),
        ));
    }
    if is_expense_candidate_shape(&candidate) {
        return Ok(review_error_response(
            "candidates accept",
            "conflict",
            "candidate_requires_expense_category",
            "Purchase candidates must be accepted with an explicit expense category.".to_string(),
            "candidate",
            true,
            serde_json::json!({
                "candidate_id": candidate_id,
                "direction_hint": candidate.direction_hint,
                "semantic_hint": candidate.semantic_hint,
                "amount_minor": candidate.amount_minor,
                "required_command": "candidates accept-expense",
            }),
        ));
    }

    let canonical_id = canonical_transaction_id(candidate_id);
    tx.execute(
        "INSERT INTO canonical_transactions (
            id, account_id, posted_date, description, amount_minor, currency,
            balance_minor, created_from_candidate_id
         )
         SELECT ?1, account_id, posted_date, description, amount_minor, currency,
                balance_minor, id
         FROM candidate_transactions
         WHERE id = ?2",
        params![canonical_id, candidate_id],
    )?;
    mark_candidate_accepted_with_canonical(&tx, candidate_id, &canonical_id)?;
    tx.commit().context("committing candidate accept")?;

    let candidate = find_review_candidate(connection, candidate_id)?
        .expect("accepted candidate remains queryable");
    let canonical_transaction = canonical_transaction(connection, &canonical_id)?
        .expect("accepted canonical transaction remains queryable");
    Ok(CandidateReviewResponse {
        schema_version: CANDIDATE_REVIEW_SCHEMA_VERSION,
        command: "candidates accept",
        ok: true,
        candidate: Some(candidate),
        candidates: Vec::new(),
        canonical_transaction: Some(canonical_transaction),
        transaction_lines: Vec::new(),
        errors: Vec::new(),
    })
}

pub fn reject_candidate(
    connection: &mut Connection,
    candidate_id: &str,
) -> Result<CandidateReviewResponse> {
    let tx = connection
        .transaction()
        .context("starting candidate reject transaction")?;
    let Some(candidate) = find_review_candidate(&tx, candidate_id)? else {
        return Ok(review_error_response(
            "candidates reject",
            "not_found",
            "candidate_not_found",
            "Candidate transaction was not found.".to_string(),
            "candidate_id",
            true,
            serde_json::json!({ "candidate_id": candidate_id }),
        ));
    };
    if let Some(error) = candidate_rejection_error(&candidate) {
        return Ok(review_error_response(
            "candidates reject",
            error.category,
            error.code,
            error.message,
            error.path,
            error.recoverable,
            error.details,
        ));
    }

    tx.execute(
        "UPDATE candidate_transactions SET status = 'rejected' WHERE id = ?1",
        params![candidate_id],
    )?;
    tx.commit().context("committing candidate reject")?;
    let candidate = find_review_candidate(connection, candidate_id)?
        .expect("rejected candidate remains queryable");
    Ok(CandidateReviewResponse {
        schema_version: CANDIDATE_REVIEW_SCHEMA_VERSION,
        command: "candidates reject",
        ok: true,
        candidate: Some(candidate),
        candidates: Vec::new(),
        canonical_transaction: None,
        transaction_lines: Vec::new(),
        errors: Vec::new(),
    })
}

pub fn review_error_response(
    command: &'static str,
    category: &'static str,
    code: &'static str,
    message: String,
    path: &'static str,
    recoverable: bool,
    details: serde_json::Value,
) -> CandidateReviewResponse {
    CandidateReviewResponse {
        schema_version: CANDIDATE_REVIEW_SCHEMA_VERSION,
        command,
        ok: false,
        candidate: None,
        candidates: Vec::new(),
        canonical_transaction: None,
        transaction_lines: Vec::new(),
        errors: vec![ReviewError {
            category,
            code,
            message,
            path,
            recoverable,
            details,
        }],
    }
}

pub fn transfer_error_response(
    command: &'static str,
    category: &'static str,
    code: &'static str,
    message: String,
    path: &'static str,
    recoverable: bool,
    details: serde_json::Value,
) -> TransferReviewResponse {
    TransferReviewResponse {
        schema_version: TRANSFER_REVIEW_SCHEMA_VERSION,
        command,
        ok: false,
        transfer_pair: None,
        transfer_pairs: Vec::new(),
        canonical_transactions: Vec::new(),
        errors: vec![ReviewError {
            category,
            code,
            message,
            path,
            recoverable,
            details,
        }],
    }
}

fn likely_transfer_pairs_from_candidates(
    connection: &Connection,
    candidates: &[ReviewCandidate],
) -> Result<Vec<ReviewTransferPair>> {
    let mut pairs = Vec::new();
    for from_candidate in candidates {
        for to_candidate in candidates {
            if from_candidate.id == to_candidate.id {
                continue;
            }
            if let Ok(pair) =
                build_transfer_pair(connection, from_candidate.clone(), to_candidate.clone())
            {
                pairs.push(pair);
            }
        }
    }
    pairs.sort_by(|left, right| {
        left.posted_date
            .cmp(&right.posted_date)
            .then(left.amount_minor.cmp(&right.amount_minor))
            .then(left.from_candidate.id.cmp(&right.from_candidate.id))
            .then(left.to_candidate.id.cmp(&right.to_candidate.id))
    });
    Ok(pairs)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransferPairError {
    AccountUnresolved,
    AccountNotOwned,
    NotReviewable,
    NotMatching,
}

#[derive(Debug)]
enum TransferPairBuildError {
    Validation(TransferPairError),
    Storage(anyhow::Error),
}

impl From<TransferPairError> for TransferPairBuildError {
    fn from(error: TransferPairError) -> Self {
        Self::Validation(error)
    }
}

impl From<anyhow::Error> for TransferPairBuildError {
    fn from(error: anyhow::Error) -> Self {
        Self::Storage(error)
    }
}

impl TransferPairBuildError {
    fn code(&self) -> &'static str {
        match self {
            Self::Validation(error) => error.code(),
            Self::Storage(_) => "transfer_pair_validation_failed",
        }
    }

    fn reason(&self) -> String {
        match self {
            Self::Validation(error) => error.reason().to_string(),
            Self::Storage(error) => error.to_string(),
        }
    }
}

impl TransferPairError {
    fn code(self) -> &'static str {
        match self {
            Self::AccountUnresolved => "transfer_pair_account_unresolved",
            Self::AccountNotOwned => "transfer_pair_account_not_owned",
            Self::NotReviewable => "transfer_pair_not_reviewable",
            Self::NotMatching => "transfer_pair_not_matching",
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Self::AccountUnresolved => "candidate account is unresolved",
            Self::AccountNotOwned => "candidate account is not owned",
            Self::NotReviewable => {
                "transfer pair candidates must be unreviewed and not canonical-linked"
            }
            Self::NotMatching => {
                "transfer pair date, amount, currency, direction, or semantic hints do not match"
            }
        }
    }
}

fn build_transfer_pair(
    connection: &Connection,
    from_candidate: ReviewCandidate,
    to_candidate: ReviewCandidate,
) -> std::result::Result<ReviewTransferPair, TransferPairBuildError> {
    validate_transfer_pair_shape(&from_candidate, &to_candidate)?;
    let from_account_id = from_candidate
        .account_id
        .as_deref()
        .ok_or(TransferPairError::AccountUnresolved)?;
    let to_account_id = to_candidate
        .account_id
        .as_deref()
        .ok_or(TransferPairError::AccountUnresolved)?;
    if from_account_id == to_account_id {
        return Err(TransferPairError::NotMatching.into());
    }
    let from_account = owned_account_by_id(connection, from_account_id)?
        .ok_or(TransferPairError::AccountNotOwned)?;
    let to_account = owned_account_by_id(connection, to_account_id)?
        .ok_or(TransferPairError::AccountNotOwned)?;
    Ok(ReviewTransferPair {
        id: transfer_pair_id(&from_candidate.id, &to_candidate.id),
        transfer_kind: CARD_PAYMENT_TRANSFER_KIND,
        posted_date: from_candidate.posted_date.clone(),
        amount_minor: from_candidate.amount_minor.abs(),
        currency: from_candidate.currency.clone(),
        from_account,
        to_account,
        from_candidate,
        to_candidate,
        canonical_transaction_ids: Vec::new(),
    })
}

fn validate_transfer_pair_shape(
    from_candidate: &ReviewCandidate,
    to_candidate: &ReviewCandidate,
) -> std::result::Result<(), TransferPairError> {
    if !is_unreviewed_candidate_status(&from_candidate.status)
        || !is_unreviewed_candidate_status(&to_candidate.status)
    {
        return Err(TransferPairError::NotReviewable);
    }
    if from_candidate.canonical_transaction_id.is_some()
        || to_candidate.canonical_transaction_id.is_some()
    {
        return Err(TransferPairError::NotReviewable);
    }
    if from_candidate.semantic_hint.as_deref() != Some("bank_movement")
        || to_candidate.semantic_hint.as_deref() != Some("card_payment")
    {
        return Err(TransferPairError::NotMatching);
    }
    if from_candidate.direction_hint.as_deref() != Some("outflow")
        || from_candidate.amount_minor >= 0
    {
        return Err(TransferPairError::NotMatching);
    }
    if from_candidate.posted_date != to_candidate.posted_date {
        return Err(TransferPairError::NotMatching);
    }
    if normalized_currency(&from_candidate.currency) != normalized_currency(&to_candidate.currency)
    {
        return Err(TransferPairError::NotMatching);
    }
    if from_candidate.amount_minor.abs() != to_candidate.amount_minor.abs() {
        return Err(TransferPairError::NotMatching);
    }
    Ok(())
}

fn is_unreviewed_candidate_status(status: &CandidateStatus) -> bool {
    matches!(
        status,
        CandidateStatus::PendingReview | CandidateStatus::PossibleDuplicate
    )
}

pub fn create_manual_expense(
    connection: &mut Connection,
    input: ManualExpenseInput,
) -> Result<ManualTransactionResponse> {
    let command = "transactions add-expense";
    let account =
        match validate_manual_account(connection, &input.account_id, &input.currency, command)? {
            Ok(account) => account,
            Err(response) => return Ok(response),
        };
    if let Some(response) = validate_manual_fields(
        command,
        &input.posted_date,
        &input.description,
        input.amount_minor,
        false,
    ) {
        return Ok(response);
    }
    let lines = normalized_expense_lines(&input.lines, input.amount_minor, &input.currency);
    if let Some(response) = validate_expense_lines(
        connection,
        command,
        input.amount_minor,
        &input.currency,
        &lines,
    )? {
        return Ok(manual_from_review_error(response));
    }
    let entry_id = manual_entry_id(
        "expense",
        &input.account_id,
        &input.posted_date,
        input.amount_minor,
        &input.currency,
        &input.description,
    );
    let canonical_id = canonical_transaction_id(&entry_id);
    let tx = connection
        .transaction()
        .context("starting manual expense transaction")?;
    tx.execute(
        "INSERT INTO canonical_transactions (id, account_id, posted_date, description, amount_minor, currency, transaction_kind)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![canonical_id, input.account_id, input.posted_date, input.description.trim(), input.amount_minor, normalized_currency(&input.currency), EXPENSE_TRANSACTION_KIND],
    )?;
    insert_expense_lines(&tx, &canonical_id, &lines)?;
    insert_manual_fingerprint(&tx, &canonical_id, &account)?;
    insert_manual_provenance(&tx, &canonical_id, &entry_id)?;
    tx.commit().context("committing manual expense")?;
    let canonical = canonical_transaction(connection, &canonical_id)?
        .expect("manual expense remains queryable");
    let lines = transaction_lines_for_canonical(connection, &canonical_id)?;
    Ok(manual_success(
        command,
        vec![canonical],
        lines,
        None,
        vec![manual_provenance(entry_id)],
    ))
}

pub fn create_manual_income(
    connection: &mut Connection,
    input: ManualIncomeInput,
) -> Result<ManualTransactionResponse> {
    let command = "transactions add-income";
    let account =
        match validate_manual_account(connection, &input.account_id, &input.currency, command)? {
            Ok(account) => account,
            Err(response) => return Ok(response),
        };
    if let Some(response) = validate_manual_fields(
        command,
        &input.posted_date,
        &input.description,
        input.amount_minor,
        true,
    ) {
        return Ok(response);
    }
    let income_kind = normalize_income_kind(&input.income_kind);
    if !INCOME_KINDS.contains(&income_kind.as_str()) {
        return Ok(manual_error(
            command,
            "validation_failure",
            "invalid_income_kind",
            "Income kind is not supported.",
            "income_kind",
            serde_json::json!({ "income_kind": input.income_kind, "allowed_income_kinds": INCOME_KINDS }),
        ));
    }
    if income_source_by_id(connection, &input.income_source_id)?.is_none() {
        return Ok(manual_error(
            command,
            "not_found",
            "income_source_not_found",
            "Income source was not found.",
            "income_source_id",
            serde_json::json!({ "income_source_id": input.income_source_id }),
        ));
    }
    let entry_id = manual_entry_id(
        "income",
        &input.account_id,
        &input.posted_date,
        input.amount_minor,
        &input.currency,
        &input.description,
    );
    let canonical_id = canonical_transaction_id(&entry_id);
    let tx = connection
        .transaction()
        .context("starting manual income transaction")?;
    tx.execute(
        "INSERT INTO canonical_transactions (id, account_id, posted_date, description, amount_minor, currency, transaction_kind, income_source_id, income_kind)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![canonical_id, input.account_id, input.posted_date, input.description.trim(), input.amount_minor, normalized_currency(&input.currency), INCOME_TRANSACTION_KIND, input.income_source_id, income_kind],
    )?;
    insert_manual_fingerprint(&tx, &canonical_id, &account)?;
    insert_manual_provenance(&tx, &canonical_id, &entry_id)?;
    tx.commit().context("committing manual income")?;
    let canonical =
        canonical_transaction(connection, &canonical_id)?.expect("manual income remains queryable");
    Ok(manual_success(
        command,
        vec![canonical],
        Vec::new(),
        None,
        vec![manual_provenance(entry_id)],
    ))
}

pub fn create_manual_transfer(
    connection: &mut Connection,
    input: ManualTransferInput,
) -> Result<ManualTransactionResponse> {
    let command = "transactions add-transfer";
    let from_account = match validate_manual_account(
        connection,
        &input.from_account_id,
        &input.currency,
        command,
    )? {
        Ok(account) => account,
        Err(response) => return Ok(response),
    };
    let to_account = match validate_manual_account(
        connection,
        &input.to_account_id,
        &input.currency,
        command,
    )? {
        Ok(account) => account,
        Err(response) => return Ok(response),
    };
    if input.from_account_id == input.to_account_id {
        return Ok(manual_error(
            command,
            "validation_failure",
            "transfer_accounts_must_differ",
            "Manual transfer accounts must differ.",
            "to_account_id",
            serde_json::json!({}),
        ));
    }
    if let Some(response) = validate_manual_fields(
        command,
        &input.posted_date,
        &input.description,
        input.amount_minor,
        true,
    ) {
        return Ok(response);
    }
    let pair_id = manual_entry_id(
        "transfer",
        &format!("{}|{}", input.from_account_id, input.to_account_id),
        &input.posted_date,
        input.amount_minor,
        &input.currency,
        &input.description,
    );
    let from_id = canonical_transaction_id(&format!("{pair_id}|from"));
    let to_id = canonical_transaction_id(&format!("{pair_id}|to"));
    let tx = connection
        .transaction()
        .context("starting manual transfer transaction")?;
    for (canonical_id, account_id, amount_minor) in [
        (&from_id, &input.from_account_id, -input.amount_minor),
        (&to_id, &input.to_account_id, input.amount_minor),
    ] {
        tx.execute(
            "INSERT INTO canonical_transactions (id, account_id, posted_date, description, amount_minor, currency, transaction_kind)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![canonical_id, account_id, input.posted_date, input.description.trim(), amount_minor, normalized_currency(&input.currency), OWN_ACCOUNT_TRANSFER_KIND],
        )?;
        let account = if account_id == &input.from_account_id {
            &from_account
        } else {
            &to_account
        };
        insert_manual_fingerprint(&tx, canonical_id, account)?;
        insert_manual_provenance(&tx, canonical_id, &format!("{pair_id}|{canonical_id}"))?;
    }
    tx.execute(
        "INSERT INTO manual_transfer_pairs (id, posted_date, amount_minor, currency, from_account_id, to_account_id, from_canonical_transaction_id, to_canonical_transaction_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![pair_id, input.posted_date, input.amount_minor, normalized_currency(&input.currency), input.from_account_id, input.to_account_id, from_id, to_id],
    )?;
    tx.commit().context("committing manual transfer")?;
    let canonical_transactions = vec![
        canonical_transaction(connection, &from_id)?
            .expect("manual transfer outflow remains queryable"),
        canonical_transaction(connection, &to_id)?
            .expect("manual transfer inflow remains queryable"),
    ];
    let provenance = canonical_transactions
        .iter()
        .map(|transaction| manual_provenance(format!("{pair_id}|{}", transaction.id)))
        .collect();
    Ok(manual_success(
        command,
        canonical_transactions,
        Vec::new(),
        Some(ManualTransferPair {
            id: pair_id,
            transfer_kind: OWN_ACCOUNT_TRANSFER_KIND,
            posted_date: input.posted_date,
            amount_minor: input.amount_minor,
            currency: normalized_currency(&input.currency),
            from_account,
            to_account,
            canonical_transaction_ids: vec![from_id, to_id],
        }),
        provenance,
    ))
}

fn validate_manual_account(
    connection: &Connection,
    account_id: &str,
    currency: &str,
    command: &'static str,
) -> Result<Result<OwnedAccount, ManualTransactionResponse>> {
    if currency.len() != 3
        || !currency
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return Ok(Err(manual_error(
            command,
            "validation_failure",
            "invalid_currency",
            "Currency must use a three-letter code.",
            "currency",
            serde_json::json!({ "currency": currency }),
        )));
    }
    let Some(account) = owned_account_by_id(connection, account_id)? else {
        return Ok(Err(manual_error(
            command,
            "not_found",
            "owned_account_not_found",
            "Owned account was not found.",
            "account_id",
            serde_json::json!({ "account_id": account_id }),
        )));
    };
    if !account.currency.eq_ignore_ascii_case(currency) {
        return Ok(Err(manual_error(
            command,
            "validation_failure",
            "account_currency_mismatch",
            "Manual transaction currency must match the owned account currency.",
            "currency",
            serde_json::json!({ "account_id": account_id, "account_currency": account.currency, "currency": currency }),
        )));
    }
    Ok(Ok(account))
}

fn validate_manual_fields(
    command: &'static str,
    posted_date: &str,
    description: &str,
    amount_minor: i64,
    positive: bool,
) -> Option<ManualTransactionResponse> {
    if !is_valid_posted_date(posted_date) {
        return Some(manual_error(
            command,
            "validation_failure",
            "invalid_posted_date",
            "Posted date must use YYYY-MM-DD.",
            "posted_date",
            serde_json::json!({ "posted_date": posted_date }),
        ));
    }
    if description.trim().is_empty() {
        return Some(manual_error(
            command,
            "validation_failure",
            "description_required",
            "Description is required.",
            "description",
            serde_json::json!({}),
        ));
    }
    if (positive && amount_minor <= 0) || (!positive && amount_minor >= 0) {
        return Some(manual_error(
            command,
            "validation_failure",
            "invalid_amount_sign",
            if positive {
                "Amount must be positive."
            } else {
                "Expense amount must be negative."
            },
            "amount_minor",
            serde_json::json!({ "amount_minor": amount_minor }),
        ));
    }
    None
}

fn is_valid_posted_date(posted_date: &str) -> bool {
    let bytes = posted_date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return false;
    }
    let year = posted_date[0..4].parse::<u32>().ok();
    let month = posted_date[5..7].parse::<u32>().ok();
    let day = posted_date[8..10].parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 400 == 0 || year % 4 == 0 && year % 100 != 0 => 29,
        2 => 28,
        _ => return false,
    };
    day >= 1 && day <= max_day
}

fn insert_manual_provenance(
    connection: &Connection,
    canonical_id: &str,
    entry_id: &str,
) -> Result<()> {
    connection.execute("INSERT INTO manual_transaction_provenance (canonical_transaction_id, entry_id, source) VALUES (?1, ?2, 'manual_entry')", params![canonical_id, entry_id])?;
    Ok(())
}

fn insert_manual_fingerprint(
    connection: &Connection,
    canonical_id: &str,
    account: &OwnedAccount,
) -> Result<()> {
    let canonical = canonical_transaction(connection, canonical_id)?
        .expect("manual canonical transaction remains queryable before fingerprinting");
    let account_key = format!(
        "{}|{}",
        account.institution.to_ascii_lowercase(),
        account.label.to_ascii_lowercase()
    );
    let fingerprint = Sha256::digest(
        format!(
            "{account_key}|{}|{}|{}|{}",
            canonical.posted_date,
            canonical.amount_minor,
            normalized_currency(&canonical.currency),
            canonical.description.to_ascii_lowercase()
        )
        .as_bytes(),
    );
    connection.execute(
        "INSERT INTO transaction_fingerprints (
            id, fingerprint, canonical_transaction_id, duplicate_status,
            normalized_account_key, normalized_posted_date, normalized_amount_minor,
            normalized_currency, normalized_description
         ) VALUES (?1, ?2, ?3, 'unique', ?4, ?5, ?6, ?7, ?8)",
        params![
            fingerprint_row_id(canonical_id),
            format!("manual_{}", hex_digest(&fingerprint)),
            canonical_id,
            account_key,
            canonical.posted_date,
            canonical.amount_minor,
            normalized_currency(&canonical.currency),
            canonical.description.to_ascii_lowercase(),
        ],
    )?;
    Ok(())
}

fn manual_entry_id(
    kind: &str,
    account_key: &str,
    posted_date: &str,
    amount_minor: i64,
    currency: &str,
    description: &str,
) -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after Unix epoch")
        .as_nanos();
    let digest = Sha256::digest(
        format!(
            "{kind}|{account_key}|{posted_date}|{amount_minor}|{}|{}|{nonce}",
            normalized_currency(currency),
            description.trim()
        )
        .as_bytes(),
    );
    format!("manual_{}", hex_digest(&digest))
}

fn hex_digest(digest: &[u8]) -> String {
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn manual_provenance(entry_id: String) -> ManualProvenance {
    ManualProvenance {
        source: "manual_entry",
        entry_id,
    }
}

fn manual_success(
    command: &'static str,
    canonical_transactions: Vec<CanonicalTransaction>,
    transaction_lines: Vec<TransactionLine>,
    transfer_pair: Option<ManualTransferPair>,
    provenance: Vec<ManualProvenance>,
) -> ManualTransactionResponse {
    ManualTransactionResponse {
        schema_version: MANUAL_TRANSACTIONS_SCHEMA_VERSION,
        command,
        ok: true,
        canonical_transactions,
        transaction_lines,
        transfer_pair,
        provenance,
        errors: Vec::new(),
    }
}

pub fn manual_error(
    command: &'static str,
    category: &'static str,
    code: &'static str,
    message: &'static str,
    path: &'static str,
    details: serde_json::Value,
) -> ManualTransactionResponse {
    ManualTransactionResponse {
        schema_version: MANUAL_TRANSACTIONS_SCHEMA_VERSION,
        command,
        ok: false,
        canonical_transactions: Vec::new(),
        transaction_lines: Vec::new(),
        transfer_pair: None,
        provenance: Vec::new(),
        errors: vec![ReviewError {
            category,
            code,
            message: message.to_string(),
            path,
            recoverable: true,
            details,
        }],
    }
}

fn manual_from_review_error(response: CandidateReviewResponse) -> ManualTransactionResponse {
    ManualTransactionResponse {
        schema_version: MANUAL_TRANSACTIONS_SCHEMA_VERSION,
        command: response.command,
        ok: false,
        canonical_transactions: Vec::new(),
        transaction_lines: Vec::new(),
        transfer_pair: None,
        provenance: Vec::new(),
        errors: response.errors,
    }
}

fn insert_transfer_canonical_transaction(
    connection: &Connection,
    canonical_id: &str,
    candidate_id: &str,
    amount_minor: i64,
) -> Result<()> {
    connection.execute(
        "INSERT INTO canonical_transactions (
            id, account_id, posted_date, description, amount_minor, currency,
            balance_minor, transaction_kind, created_from_candidate_id
         )
         SELECT ?1, account_id, posted_date, description, ?2, currency,
                balance_minor, ?3, id
         FROM candidate_transactions
         WHERE id = ?4",
        params![
            canonical_id,
            amount_minor,
            OWN_ACCOUNT_TRANSFER_KIND,
            candidate_id
        ],
    )?;
    Ok(())
}

fn apply_transfer_pair_rows(
    connection: &Connection,
    pair: &ReviewTransferPair,
) -> Result<Vec<String>> {
    let from_candidate_id = &pair.from_candidate.id;
    let to_candidate_id = &pair.to_candidate.id;
    let from_canonical_id = canonical_transaction_id(from_candidate_id);
    let to_canonical_id = canonical_transaction_id(to_candidate_id);
    insert_transfer_canonical_transaction(
        connection,
        &from_canonical_id,
        from_candidate_id,
        -pair.amount_minor,
    )?;
    insert_transfer_canonical_transaction(
        connection,
        &to_canonical_id,
        to_candidate_id,
        pair.amount_minor,
    )?;
    connection.execute(
        "INSERT INTO canonical_transfer_pairs (
            id, transfer_kind, posted_date, amount_minor, currency,
            from_account_id, to_account_id, from_candidate_id, to_candidate_id,
            from_canonical_transaction_id, to_canonical_transaction_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            pair.id,
            CARD_PAYMENT_TRANSFER_KIND,
            pair.posted_date,
            pair.amount_minor,
            pair.currency,
            pair.from_account.id,
            pair.to_account.id,
            from_candidate_id,
            to_candidate_id,
            from_canonical_id,
            to_canonical_id,
        ],
    )?;
    mark_candidate_accepted_with_canonical(connection, from_candidate_id, &from_canonical_id)?;
    mark_candidate_accepted_with_canonical(connection, to_candidate_id, &to_canonical_id)?;
    Ok(vec![from_canonical_id, to_canonical_id])
}

fn candidate_rejection_error(candidate: &ReviewCandidate) -> Option<ReviewError> {
    if candidate.status == CandidateStatus::Accepted {
        return Some(ReviewError {
            category: "conflict",
            code: "candidate_already_accepted",
            message: "Accepted candidates cannot be rejected without a future reversal command."
                .to_string(),
            path: "candidate.status",
            recoverable: true,
            details: serde_json::json!({
                "candidate_id": candidate.id,
                "canonical_transaction_id": candidate.canonical_transaction_id,
            }),
        });
    }
    if candidate.status == CandidateStatus::Rejected {
        return Some(ReviewError {
            category: "conflict",
            code: "candidate_already_rejected",
            message: "Candidate transaction was already rejected.".to_string(),
            path: "candidate.status",
            recoverable: true,
            details: serde_json::json!({ "candidate_id": candidate.id }),
        });
    }
    None
}

fn mark_candidate_accepted_with_canonical(
    connection: &Connection,
    candidate_id: &str,
    canonical_id: &str,
) -> Result<()> {
    connection.execute(
        "UPDATE candidate_transactions
         SET status = 'accepted', canonical_transaction_id = ?1
         WHERE id = ?2",
        params![canonical_id, candidate_id],
    )?;
    connection.execute(
        "UPDATE provenance
         SET canonical_transaction_id = ?1
         WHERE candidate_transaction_id = ?2",
        params![canonical_id, candidate_id],
    )?;
    connection.execute(
        "UPDATE transaction_fingerprints
         SET candidate_transaction_id = NULL,
             canonical_transaction_id = ?1
         WHERE candidate_transaction_id = ?2",
        params![canonical_id, candidate_id],
    )?;
    Ok(())
}

fn is_income_candidate_shape(candidate: &ReviewCandidate) -> bool {
    candidate.amount_minor > 0
        && candidate.direction_hint.as_deref() == Some("inflow")
        && candidate.semantic_hint.as_deref() == Some("bank_movement")
}

fn is_expense_candidate_shape(candidate: &ReviewCandidate) -> bool {
    match candidate.semantic_hint.as_deref() {
        Some("bank_movement") => {
            candidate.direction_hint.as_deref() == Some("outflow") && candidate.amount_minor < 0
        }
        Some("card_charge") => {
            candidate.direction_hint.as_deref() == Some("outflow") && candidate.amount_minor != 0
        }
        _ => false,
    }
}

fn expense_amount_minor(candidate: &ReviewCandidate) -> i64 {
    -candidate.amount_minor.abs()
}

fn has_matching_owned_account_outflow(
    connection: &Connection,
    candidate: &ReviewCandidate,
) -> Result<bool> {
    let Some(candidate_account_id) = candidate.account_id.as_deref() else {
        return Ok(false);
    };
    if owned_account_by_id(connection, candidate_account_id)?.is_none() {
        return Ok(false);
    }
    let mut statement = connection.prepare(
        "SELECT c.account_id
         FROM candidate_transactions c
         WHERE c.id <> ?1
           AND c.status IN ('pending_review', 'possible_duplicate')
           AND c.canonical_transaction_id IS NULL
           AND c.account_id IS NOT NULL
           AND c.account_id <> ?2
           AND c.posted_date = ?3
           AND UPPER(c.currency) = UPPER(?4)
           AND ABS(c.amount_minor) = ?5
           AND c.amount_minor < 0
           AND c.direction_hint = 'outflow'
           AND c.semantic_hint = 'bank_movement'",
    )?;
    let rows = statement.query_map(
        params![
            candidate.id,
            candidate_account_id,
            candidate.posted_date,
            candidate.currency,
            candidate.amount_minor.abs()
        ],
        |row| row.get::<_, String>(0),
    )?;
    for row in rows {
        let account_id = row?;
        if owned_account_by_id(connection, &account_id)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn has_matching_owned_account_counterparty_candidate(
    connection: &Connection,
    candidate: &ReviewCandidate,
) -> Result<bool> {
    if candidate.semantic_hint.as_deref() != Some("bank_movement")
        || candidate.direction_hint.as_deref() != Some("outflow")
    {
        return Ok(false);
    }
    let Some(candidate_account_id) = candidate.account_id.as_deref() else {
        return Ok(false);
    };
    if owned_account_by_id(connection, candidate_account_id)?.is_none() {
        return Ok(false);
    }
    let mut statement = connection.prepare(
        "SELECT c.account_id
         FROM candidate_transactions c
         WHERE c.id <> ?1
           AND c.status IN ('pending_review', 'possible_duplicate')
           AND c.canonical_transaction_id IS NULL
           AND c.account_id IS NOT NULL
           AND c.account_id <> ?2
           AND c.posted_date = ?3
           AND UPPER(c.currency) = UPPER(?4)
           AND ABS(c.amount_minor) = ?5
           AND (
               c.semantic_hint = 'card_payment'
               OR (
                   c.semantic_hint = 'bank_movement'
                   AND c.direction_hint = 'inflow'
                   AND c.amount_minor > 0
               )
           )",
    )?;
    let rows = statement.query_map(
        params![
            candidate.id,
            candidate_account_id,
            candidate.posted_date,
            candidate.currency,
            candidate.amount_minor.abs()
        ],
        |row| row.get::<_, String>(0),
    )?;
    for row in rows {
        let account_id = row?;
        if owned_account_by_id(connection, &account_id)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn normalize_income_kind(kind: &str) -> String {
    stable_slug(kind)
}

fn validate_candidate_status_filter(status: &str) -> Result<()> {
    match status {
        "pending_review" | "possible_duplicate" | "accepted" | "rejected" => Ok(()),
        other => anyhow::bail!("unsupported candidate status filter: {other}"),
    }
}

fn parse_review_candidate_status(status: String) -> rusqlite::Result<CandidateStatus> {
    match status.as_str() {
        "pending_review" => Ok(CandidateStatus::PendingReview),
        "possible_duplicate" => Ok(CandidateStatus::PossibleDuplicate),
        "accepted" => Ok(CandidateStatus::Accepted),
        "rejected" => Ok(CandidateStatus::Rejected),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unsupported candidate status: {other}").into(),
        )),
    }
}

fn parse_review_duplicate_status(status: String) -> rusqlite::Result<DuplicateStatusState> {
    match status.as_str() {
        "not_checked" => Ok(DuplicateStatusState::NotChecked),
        "unique" => Ok(DuplicateStatusState::Unique),
        "possible_duplicate" => Ok(DuplicateStatusState::PossibleDuplicate),
        "exact_duplicate" => Ok(DuplicateStatusState::ExactDuplicate),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unsupported duplicate status: {other}").into(),
        )),
    }
}

fn find_review_candidate(
    connection: &Connection,
    candidate_id: &str,
) -> Result<Option<ReviewCandidate>> {
    let sql = format!("{} WHERE c.id = ?1", review_candidate_select_sql());
    let mut candidate = connection
        .query_row(&sql, params![candidate_id], review_candidate_from_row)
        .optional()?;
    if let Some(candidate) = &mut candidate {
        hydrate_duplicate_matches(connection, candidate)?;
    }
    Ok(candidate)
}

fn review_candidate_select_sql() -> String {
    "SELECT
        c.id, c.import_batch_id, c.source_document_id, c.status,
        c.duplicate_status, c.fingerprint, c.institution_id, c.institution_hint,
        c.account_id, c.account_label_hint, c.account_currency_hint, c.account_masked_identifier_hint,
        c.posted_date, c.description, c.amount_minor, c.currency, c.balance_minor,
        c.direction_hint, c.semantic_hint, c.confidence, c.validation_warnings_json, c.canonical_transaction_id,
        p.candidate_transaction_id, p.source_document_id, p.import_batch_id, p.page_number, p.row_index,
        p.evidence_redaction, p.evidence_text_redacted, p.raw_storage_policy,
        p.extractor_name, p.extractor_version, p.parser_id, p.parser_version,
        p.confidence, p.canonical_transaction_id
     FROM candidate_transactions c
     JOIN provenance p ON p.candidate_transaction_id = c.id"
        .to_string()
}

fn review_candidate_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewCandidate> {
    let validation_warnings_json: String = row.get(20)?;
    let validation_warnings = serde_json::from_str(&validation_warnings_json).unwrap_or_default();
    Ok(ReviewCandidate {
        id: row.get(0)?,
        import_batch_id: row.get(1)?,
        source_document_id: row.get(2)?,
        status: parse_review_candidate_status(row.get(3)?)?,
        duplicate_status: ReviewDuplicateStatus {
            status: parse_review_duplicate_status(row.get(4)?)?,
            fingerprint: row.get(5)?,
            matched_candidate_ids: Vec::new(),
            matched_canonical_transaction_ids: Vec::new(),
            reason: None,
        },
        institution_id: row.get(6)?,
        institution_hint: row.get(7)?,
        account_id: row.get(8)?,
        account_hint: ReviewAccountHint {
            label: row.get(9)?,
            currency: row.get(10)?,
            masked_identifier: row.get(11)?,
        },
        posted_date: row.get(12)?,
        description: row.get(13)?,
        amount_minor: row.get(14)?,
        currency: row.get(15)?,
        balance_minor: row.get(16)?,
        direction_hint: row.get(17)?,
        semantic_hint: row.get(18)?,
        confidence: row.get(19)?,
        provenance: ReviewProvenance {
            candidate_transaction_id: row.get(22)?,
            source_document_id: row.get(23)?,
            import_batch_id: row.get(24)?,
            page_number: row.get(25)?,
            row_index: row.get(26)?,
            evidence_redaction: row.get(27)?,
            evidence_text_redacted: row.get(28)?,
            raw_storage_policy: row.get(29)?,
            extractor_name: row.get(30)?,
            extractor_version: row.get(31)?,
            parser_id: row.get(32)?,
            parser_version: row.get(33)?,
            confidence: row.get(34)?,
            canonical_transaction_id: row.get(35)?,
        },
        validation_warnings,
        canonical_transaction_id: row.get(21)?,
    })
}

fn hydrate_duplicate_matches(
    connection: &Connection,
    candidate: &mut ReviewCandidate,
) -> Result<()> {
    let mut statement = connection.prepare(
        "SELECT matched_candidate_transaction_id, matched_canonical_transaction_id, reason
         FROM transaction_duplicate_markers
         WHERE candidate_transaction_id = ?1
         ORDER BY id",
    )?;
    let rows = statement.query_map(params![candidate.id], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    for row in rows {
        let (matched_candidate_id, matched_canonical_id, reason) = row?;
        if let Some(matched_candidate_id) = matched_candidate_id {
            candidate
                .duplicate_status
                .matched_candidate_ids
                .push(matched_candidate_id);
        }
        if let Some(matched_canonical_id) = matched_canonical_id {
            candidate
                .duplicate_status
                .matched_canonical_transaction_ids
                .push(matched_canonical_id);
        }
        if candidate.duplicate_status.reason.is_none() {
            candidate.duplicate_status.reason = reason;
        }
    }
    Ok(())
}

pub fn list_canonical_transactions(
    connection: &Connection,
    filter: TransactionListFilter<'_>,
) -> Result<TransactionLedgerResponse> {
    let mut sql = "SELECT id, account_id, posted_date, description, amount_minor, currency, balance_minor, transaction_kind, income_source_id, income_kind, created_from_candidate_id FROM canonical_transactions WHERE 1 = 1".to_string();
    let mut values: Vec<String> = Vec::new();
    if let Some(value) = filter.start_date {
        sql.push_str(" AND posted_date >= ?");
        values.push(value.to_string());
    }
    if let Some(value) = filter.end_date {
        sql.push_str(" AND posted_date <= ?");
        values.push(value.to_string());
    }
    if let Some(value) = filter.account_id {
        sql.push_str(" AND account_id = ?");
        values.push(value.to_string());
    }
    if let Some(value) = filter.income_source_id {
        sql.push_str(" AND income_source_id = ?");
        values.push(value.to_string());
    }
    if let Some(value) = filter.transaction_kind {
        sql.push_str(" AND transaction_kind = ?");
        values.push(value.to_string());
    }
    if let Some(value) = filter.category_id {
        sql.push_str(" AND EXISTS (SELECT 1 FROM transaction_lines tl WHERE tl.canonical_transaction_id = canonical_transactions.id AND tl.category_id = ?)");
        values.push(value.to_string());
    }
    sql.push_str(" ORDER BY posted_date, id");
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        rusqlite::params_from_iter(values.iter()),
        canonical_transaction_from_row,
    )?;
    let canonical_transactions = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(transaction_ledger_success(
        "transactions list",
        None,
        canonical_transactions,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
    ))
}

pub fn summarize_finances(
    connection: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<FinanceReportResponse> {
    let date_range = ReportDateRange {
        start_date: start_date.to_string(),
        end_date: end_date.to_string(),
    };
    if !is_valid_posted_date(start_date) {
        return Ok(finance_report_error(
            date_range,
            "invalid_start_date",
            "Start date must use YYYY-MM-DD.",
            "start_date",
        ));
    }
    if !is_valid_posted_date(end_date) {
        return Ok(finance_report_error(
            date_range,
            "invalid_end_date",
            "End date must use YYYY-MM-DD.",
            "end_date",
        ));
    }
    if start_date > end_date {
        return Ok(finance_report_error(
            date_range,
            "invalid_date_range",
            "Start date must be on or before end date.",
            "date_range",
        ));
    }

    let excluded_transfer_totals =
        finance_excluded_transfer_totals(connection, start_date, end_date)?;
    let totals =
        finance_currency_totals(connection, start_date, end_date, &excluded_transfer_totals)?;
    let category_totals = finance_category_totals(connection, start_date, end_date)?;
    let income_source_totals = finance_income_source_totals(connection, start_date, end_date)?;
    Ok(FinanceReportResponse {
        schema_version: FINANCE_REPORT_SCHEMA_VERSION,
        command: "reports summary",
        ok: true,
        date_range,
        totals,
        category_totals,
        income_source_totals,
        excluded_transfer_totals,
        errors: Vec::new(),
    })
}

fn finance_currency_totals(
    connection: &Connection,
    start_date: &str,
    end_date: &str,
    excluded_transfer_totals: &[ExcludedTransferTotal],
) -> Result<Vec<FinanceCurrencyTotal>> {
    let mut statement = connection.prepare(
        "SELECT UPPER(currency) AS currency,
                COALESCE(SUM(CASE WHEN transaction_kind = 'income' THEN amount_minor ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN transaction_kind = 'expense' THEN -amount_minor ELSE 0 END), 0)
         FROM canonical_transactions
         WHERE posted_date >= ?1 AND posted_date <= ?2
           AND transaction_kind IN ('income', 'expense')
         GROUP BY UPPER(currency)",
    )?;
    let rows = statement.query_map(params![start_date, end_date], |row| {
        let total_income_minor = row.get(1)?;
        let total_expenses_minor = row.get(2)?;
        let currency = row.get::<_, String>(0)?;
        Ok((
            currency.clone(),
            FinanceCurrencyTotal {
                currency,
                total_income_minor,
                total_expenses_minor,
                net_cash_flow_minor: total_income_minor - total_expenses_minor,
                excluded_transfer_total_minor: 0,
                excluded_transfer_count: 0,
            },
        ))
    })?;
    let mut totals = rows.collect::<rusqlite::Result<std::collections::BTreeMap<_, _>>>()?;
    for transfer_total in excluded_transfer_totals {
        let total = totals
            .entry(transfer_total.currency.clone())
            .or_insert_with(|| FinanceCurrencyTotal {
                currency: transfer_total.currency.clone(),
                total_income_minor: 0,
                total_expenses_minor: 0,
                net_cash_flow_minor: 0,
                excluded_transfer_total_minor: 0,
                excluded_transfer_count: 0,
            });
        total.excluded_transfer_total_minor += transfer_total.total_amount_minor;
        total.excluded_transfer_count += transfer_total.transfer_count;
    }
    Ok(totals.into_values().collect())
}

fn finance_category_totals(
    connection: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<CategoryTotal>> {
    let mut statement = connection.prepare(
        "SELECT c.id, c.name, UPPER(tl.currency), SUM(-tl.amount_minor)
         FROM transaction_lines tl
         JOIN canonical_transactions ct ON ct.id = tl.canonical_transaction_id
         JOIN categories c ON c.id = tl.category_id
         WHERE ct.transaction_kind = 'expense'
           AND ct.posted_date >= ?1 AND ct.posted_date <= ?2
         GROUP BY c.id, c.name, UPPER(tl.currency)
         ORDER BY UPPER(tl.currency), LOWER(c.name), c.id",
    )?;
    let rows = statement.query_map(params![start_date, end_date], |row| {
        Ok(CategoryTotal {
            category_id: row.get(0)?,
            category_name: row.get(1)?,
            currency: row.get(2)?,
            total_expenses_minor: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn finance_income_source_totals(
    connection: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<IncomeSourceTotal>> {
    let mut statement = connection.prepare(
        "SELECT income_sources.id, income_sources.name, UPPER(ct.currency), SUM(ct.amount_minor)
         FROM canonical_transactions ct
         JOIN income_sources ON income_sources.id = ct.income_source_id
         WHERE ct.transaction_kind = 'income'
           AND ct.posted_date >= ?1 AND ct.posted_date <= ?2
         GROUP BY income_sources.id, income_sources.name, UPPER(ct.currency)
         ORDER BY UPPER(ct.currency), LOWER(income_sources.name), income_sources.id",
    )?;
    let rows = statement.query_map(params![start_date, end_date], |row| {
        Ok(IncomeSourceTotal {
            income_source_id: row.get(0)?,
            income_source_name: row.get(1)?,
            currency: row.get(2)?,
            total_income_minor: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn finance_excluded_transfer_totals(
    connection: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<ExcludedTransferTotal>> {
    let mut statement = connection.prepare(
        "WITH transfer_rows AS (
             SELECT transfer_kind, posted_date, UPPER(currency) AS currency, amount_minor
             FROM canonical_transfer_pairs
             UNION ALL
             SELECT 'own_account_transfer' AS transfer_kind, posted_date,
                    UPPER(currency) AS currency, amount_minor
             FROM manual_transfer_pairs
         )
         SELECT transfer_kind, currency, SUM(amount_minor), COUNT(*)
         FROM transfer_rows
         WHERE posted_date >= ?1 AND posted_date <= ?2
         GROUP BY transfer_kind, currency
         ORDER BY currency, transfer_kind",
    )?;
    let rows = statement.query_map(params![start_date, end_date], |row| {
        Ok(ExcludedTransferTotal {
            transfer_kind: row.get(0)?,
            currency: row.get(1)?,
            total_amount_minor: row.get(2)?,
            transfer_count: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn finance_report_error_response(
    start_date: String,
    end_date: String,
    code: &'static str,
    message: &'static str,
    path: &'static str,
) -> FinanceReportResponse {
    finance_report_error(
        ReportDateRange {
            start_date,
            end_date,
        },
        code,
        message,
        path,
    )
}

fn finance_report_error(
    date_range: ReportDateRange,
    code: &'static str,
    message: &'static str,
    path: &'static str,
) -> FinanceReportResponse {
    FinanceReportResponse {
        schema_version: FINANCE_REPORT_SCHEMA_VERSION,
        command: "reports summary",
        ok: false,
        date_range,
        totals: Vec::new(),
        category_totals: Vec::new(),
        income_source_totals: Vec::new(),
        excluded_transfer_totals: Vec::new(),
        errors: vec![ReviewError {
            category: "validation_failure",
            code,
            message: message.to_string(),
            path,
            recoverable: true,
            details: serde_json::json!({}),
        }],
    }
}

pub fn inspect_canonical_transaction(
    connection: &Connection,
    canonical_id: &str,
) -> Result<TransactionLedgerResponse> {
    let Some(canonical) = canonical_transaction(connection, canonical_id)? else {
        return Ok(transaction_ledger_error(
            "transactions inspect",
            "not_found",
            "canonical_transaction_not_found",
            "Canonical transaction was not found.",
            "transaction_id",
            serde_json::json!({ "transaction_id": canonical_id }),
        ));
    };
    let candidate = match canonical.created_from_candidate_id.as_deref() {
        Some(id) => find_review_candidate(connection, id)?,
        None => None,
    };
    let provenance = transaction_provenance_for(connection, canonical_id, candidate.as_ref())?;
    let edits = transaction_edits_for(connection, canonical_id)?;
    let transfer = transaction_transfer_for(connection, canonical_id)?;
    let lines = transaction_lines_for_canonical(connection, canonical_id)?;
    Ok(transaction_ledger_success(
        "transactions inspect",
        Some(canonical),
        Vec::new(),
        candidate,
        lines,
        provenance,
        edits,
        transfer,
    ))
}

pub fn update_canonical_transaction(
    connection: &mut Connection,
    canonical_id: &str,
    input: TransactionUpdateInput,
) -> Result<TransactionLedgerResponse> {
    let command = "transactions update";
    let Some(canonical) = canonical_transaction(connection, canonical_id)? else {
        return Ok(transaction_ledger_error(
            command,
            "not_found",
            "canonical_transaction_not_found",
            "Canonical transaction was not found.",
            "transaction_id",
            serde_json::json!({ "transaction_id": canonical_id }),
        ));
    };
    let kind = canonical.transaction_kind.as_deref().unwrap_or("");
    if kind == OWN_ACCOUNT_TRANSFER_KIND
        && (input.income_source_id.is_some()
            || input.income_kind.is_some()
            || input.expense_lines.is_some())
    {
        return Ok(transaction_ledger_error(
            command,
            "conflict",
            "transfer_classification_immutable",
            "Transfers cannot be converted into income or expenses.",
            "transaction_kind",
            serde_json::json!({ "transaction_id": canonical_id }),
        ));
    }
    if (input.income_source_id.is_some() || input.income_kind.is_some())
        && kind != INCOME_TRANSACTION_KIND
    {
        return Ok(transaction_ledger_error(
            command,
            "conflict",
            "canonical_transaction_not_income",
            "Only income transactions can update income metadata.",
            "transaction_kind",
            serde_json::json!({ "transaction_id": canonical_id, "transaction_kind": kind }),
        ));
    }
    if input.expense_lines.is_some() && kind != EXPENSE_TRANSACTION_KIND {
        return Ok(transaction_ledger_error(
            command,
            "conflict",
            "canonical_transaction_not_expense",
            "Only expense transactions can update category lines.",
            "transaction_kind",
            serde_json::json!({ "transaction_id": canonical_id, "transaction_kind": kind }),
        ));
    }
    if input
        .description
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Ok(transaction_ledger_error(
            command,
            "validation_failure",
            "description_required",
            "Description cannot be empty.",
            "description",
            serde_json::json!({}),
        ));
    }
    if let Some(source_id) = input.income_source_id.as_deref() {
        if income_source_by_id(connection, source_id)?.is_none() {
            return Ok(transaction_ledger_error(
                command,
                "not_found",
                "income_source_not_found",
                "Income source was not found.",
                "income_source_id",
                serde_json::json!({ "income_source_id": source_id }),
            ));
        }
    }
    if let Some(income_kind) = input.income_kind.as_deref() {
        if !INCOME_KINDS.contains(&income_kind.trim().to_ascii_lowercase().as_str()) {
            return Ok(transaction_ledger_error(
                command,
                "validation_failure",
                "invalid_income_kind",
                "Income kind is not supported.",
                "income_kind",
                serde_json::json!({ "income_kind": income_kind, "allowed_income_kinds": INCOME_KINDS }),
            ));
        }
    }
    let previous_lines = transaction_lines_for_canonical(connection, canonical_id)?;
    let audit_change = serde_json::json!({
        "before": { "description": canonical.description, "income_source_id": canonical.income_source_id, "income_kind": canonical.income_kind, "transaction_lines": previous_lines },
        "after": { "description": input.description.as_deref().map(str::trim).unwrap_or(&canonical.description), "income_source_id": input.income_source_id.as_deref().or(canonical.income_source_id.as_deref()), "income_kind": input.income_kind.as_deref().map(|value| value.trim().to_ascii_lowercase()).or(canonical.income_kind.clone()), "transaction_lines_replaced": input.expense_lines.is_some() }
    });
    let tx = connection
        .transaction()
        .context("starting canonical transaction update")?;
    if let Some(description) = input.description.as_deref() {
        tx.execute(
            "UPDATE canonical_transactions SET description = ?1 WHERE id = ?2",
            params![description.trim(), canonical_id],
        )?;
    }
    if let Some(source_id) = input.income_source_id.as_deref() {
        tx.execute(
            "UPDATE canonical_transactions SET income_source_id = ?1 WHERE id = ?2",
            params![source_id, canonical_id],
        )?;
    }
    if let Some(income_kind) = input.income_kind.as_deref() {
        tx.execute(
            "UPDATE canonical_transactions SET income_kind = ?1 WHERE id = ?2",
            params![income_kind.trim().to_ascii_lowercase(), canonical_id],
        )?;
    }
    if let Some(lines) = input.expense_lines.as_deref() {
        let lines = normalized_expense_lines(lines, canonical.amount_minor, &canonical.currency);
        if let Some(response) = validate_expense_lines(
            &tx,
            command,
            canonical.amount_minor,
            &canonical.currency,
            &lines,
        )? {
            return Ok(transaction_ledger_from_review_error(response));
        }
        tx.execute(
            "DELETE FROM transaction_lines WHERE canonical_transaction_id = ?1",
            params![canonical_id],
        )?;
        insert_expense_lines(&tx, canonical_id, &lines)?;
    }
    tx.execute(
        "INSERT INTO canonical_transaction_edits (id, canonical_transaction_id, changed_fields_json) VALUES (printf('edit_%s_%s', ?1, lower(hex(randomblob(8)))), ?1, ?2)",
        params![canonical_id, audit_change.to_string()],
    )?;
    tx.commit()
        .context("committing canonical transaction update")?;
    inspect_canonical_transaction(connection, canonical_id).map(|mut response| {
        response.command = command;
        response
    })
}

fn canonical_transaction_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<CanonicalTransaction> {
    Ok(CanonicalTransaction {
        id: row.get(0)?,
        account_id: row.get(1)?,
        posted_date: row.get(2)?,
        description: row.get(3)?,
        amount_minor: row.get(4)?,
        currency: row.get(5)?,
        balance_minor: row.get(6)?,
        transaction_kind: row.get(7)?,
        income_source_id: row.get(8)?,
        income_kind: row.get(9)?,
        created_from_candidate_id: row.get(10)?,
    })
}

fn transaction_provenance_for(
    connection: &Connection,
    canonical_id: &str,
    candidate: Option<&ReviewCandidate>,
) -> Result<Vec<TransactionProvenance>> {
    if let Some(candidate) = candidate {
        return Ok(vec![TransactionProvenance {
            source: "pdf_import".to_string(),
            entry_id: None,
            candidate_provenance: Some(candidate.provenance.clone()),
        }]);
    }
    connection.query_row("SELECT entry_id, source FROM manual_transaction_provenance WHERE canonical_transaction_id = ?1", params![canonical_id], |row| Ok(TransactionProvenance { source: row.get(1)?, entry_id: Some(row.get(0)?), candidate_provenance: None })).optional().map(|value| value.into_iter().collect()).map_err(Into::into)
}

fn transaction_transfer_for(
    connection: &Connection,
    canonical_id: &str,
) -> Result<Option<TransactionTransferMetadata>> {
    connection.query_row("SELECT id, transfer_kind, from_account_id, to_account_id FROM canonical_transfer_pairs WHERE from_canonical_transaction_id = ?1 OR to_canonical_transaction_id = ?1 UNION ALL SELECT id, 'own_account_transfer', from_account_id, to_account_id FROM manual_transfer_pairs WHERE from_canonical_transaction_id = ?1 OR to_canonical_transaction_id = ?1 LIMIT 1", params![canonical_id], |row| Ok(TransactionTransferMetadata { id: row.get(0)?, transfer_kind: row.get(1)?, from_account_id: row.get(2)?, to_account_id: row.get(3)? })).optional().map_err(Into::into)
}

fn transaction_edits_for(
    connection: &Connection,
    canonical_id: &str,
) -> Result<Vec<TransactionEdit>> {
    let mut statement = connection.prepare("SELECT id, changed_fields_json, created_at FROM canonical_transaction_edits WHERE canonical_transaction_id = ?1 ORDER BY created_at, id")?;
    let rows = statement.query_map(params![canonical_id], |row| {
        Ok(TransactionEdit {
            id: row.get(0)?,
            changed_fields: serde_json::from_str::<serde_json::Value>(&row.get::<_, String>(1)?)
                .unwrap_or(serde_json::json!({})),
            created_at: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn transaction_ledger_success(
    command: &'static str,
    canonical_transaction: Option<CanonicalTransaction>,
    canonical_transactions: Vec<CanonicalTransaction>,
    candidate: Option<ReviewCandidate>,
    transaction_lines: Vec<TransactionLine>,
    provenance: Vec<TransactionProvenance>,
    edits: Vec<TransactionEdit>,
    transfer: Option<TransactionTransferMetadata>,
) -> TransactionLedgerResponse {
    TransactionLedgerResponse {
        schema_version: TRANSACTION_LEDGER_SCHEMA_VERSION,
        command,
        ok: true,
        canonical_transaction,
        canonical_transactions,
        candidate,
        transaction_lines,
        provenance,
        edits,
        transfer,
        errors: Vec::new(),
    }
}
fn transaction_ledger_error(
    command: &'static str,
    category: &'static str,
    code: &'static str,
    message: &'static str,
    path: &'static str,
    details: serde_json::Value,
) -> TransactionLedgerResponse {
    TransactionLedgerResponse {
        schema_version: TRANSACTION_LEDGER_SCHEMA_VERSION,
        command,
        ok: false,
        canonical_transaction: None,
        canonical_transactions: Vec::new(),
        candidate: None,
        transaction_lines: Vec::new(),
        provenance: Vec::new(),
        edits: Vec::new(),
        transfer: None,
        errors: vec![ReviewError {
            category,
            code,
            message: message.to_string(),
            path,
            recoverable: true,
            details,
        }],
    }
}
pub fn transaction_ledger_from_review_error(
    response: CandidateReviewResponse,
) -> TransactionLedgerResponse {
    TransactionLedgerResponse {
        schema_version: TRANSACTION_LEDGER_SCHEMA_VERSION,
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

fn canonical_transaction(
    connection: &Connection,
    canonical_id: &str,
) -> Result<Option<CanonicalTransaction>> {
    connection
        .query_row(
            "SELECT id, account_id, posted_date, description, amount_minor, currency, balance_minor,
                    transaction_kind, income_source_id, income_kind, created_from_candidate_id
             FROM canonical_transactions
             WHERE id = ?1",
            params![canonical_id],
            |row| {
                Ok(CanonicalTransaction {
                    id: row.get(0)?,
                    account_id: row.get(1)?,
                    posted_date: row.get(2)?,
                    description: row.get(3)?,
                    amount_minor: row.get(4)?,
                    currency: row.get(5)?,
                    balance_minor: row.get(6)?,
                    transaction_kind: row.get(7)?,
                    income_source_id: row.get(8)?,
                    income_kind: row.get(9)?,
                    created_from_candidate_id: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

fn transaction_lines_for_canonical(
    connection: &Connection,
    canonical_id: &str,
) -> Result<Vec<TransactionLine>> {
    let mut statement = connection.prepare(
        "SELECT tl.id, tl.canonical_transaction_id, tl.category_id, c.name,
                tl.amount_minor, tl.currency, tl.line_kind
         FROM transaction_lines tl
         JOIN categories c ON c.id = tl.category_id
         WHERE tl.canonical_transaction_id = ?1
         ORDER BY tl.id",
    )?;
    let rows = statement.query_map(params![canonical_id], |row| {
        Ok(TransactionLine {
            id: row.get(0)?,
            canonical_transaction_id: row.get(1)?,
            category_id: row.get(2)?,
            category_name: row.get(3)?,
            amount_minor: row.get(4)?,
            currency: row.get(5)?,
            line_kind: row.get(6)?,
        })
    })?;
    let mut lines = Vec::new();
    for row in rows {
        lines.push(row?);
    }
    Ok(lines)
}

fn validate_expense_lines(
    connection: &Connection,
    command: &'static str,
    canonical_amount_minor: i64,
    canonical_currency: &str,
    lines: &[ExpenseLineInput],
) -> Result<Option<CandidateReviewResponse>> {
    if lines.is_empty() {
        return Ok(Some(review_error_response(
            command,
            "validation_failure",
            "expense_lines_required",
            "At least one categorized expense line is required.".to_string(),
            "lines",
            true,
            serde_json::json!({}),
        )));
    }
    let mut total = 0_i64;
    let mut category_ids = std::collections::HashSet::new();
    for line in lines {
        if line.category_id.trim().is_empty() {
            return Ok(Some(review_error_response(
                command,
                "validation_failure",
                "expense_line_category_required",
                "Each expense line requires a category.".to_string(),
                "lines.category_id",
                true,
                serde_json::json!({}),
            )));
        }
        if !category_ids.insert(&line.category_id) {
            return Ok(Some(review_error_response(
                command,
                "validation_failure",
                "duplicate_expense_line_category",
                "Each expense line must use a distinct category.".to_string(),
                "lines.category_id",
                true,
                serde_json::json!({ "category_id": line.category_id }),
            )));
        }
        if category_by_id(connection, &line.category_id)?.is_none() {
            return Ok(Some(review_error_response(
                command,
                "not_found",
                "category_not_found",
                "Expense category was not found.".to_string(),
                "lines.category_id",
                true,
                serde_json::json!({ "category_id": line.category_id }),
            )));
        }
        if !line.currency.eq_ignore_ascii_case(canonical_currency) {
            return Ok(Some(review_error_response(
                command,
                "validation_failure",
                "expense_line_currency_mismatch",
                "Expense line currency must match the canonical transaction currency.".to_string(),
                "lines.currency",
                true,
                serde_json::json!({ "expected_currency": canonical_currency, "actual_currency": line.currency }),
            )));
        }
        total = match total.checked_add(line.amount_minor) {
            Some(total) => total,
            None => {
                return Ok(Some(review_error_response(
                    command,
                    "validation_failure",
                    "expense_lines_total_overflow",
                    "Expense line totals overflow minor units.".to_string(),
                    "lines.amount_minor",
                    true,
                    serde_json::json!({}),
                )));
            }
        };
    }
    if total != canonical_amount_minor {
        return Ok(Some(review_error_response(
            command,
            "validation_failure",
            "expense_lines_unbalanced",
            "Expense line total must equal the canonical transaction amount.".to_string(),
            "lines.amount_minor",
            true,
            serde_json::json!({
                "canonical_amount_minor": canonical_amount_minor,
                "lines_total_minor": total,
            }),
        )));
    }
    Ok(None)
}

fn normalized_expense_lines(
    lines: &[ExpenseLineInput],
    canonical_amount_minor: i64,
    canonical_currency: &str,
) -> Vec<ExpenseLineInput> {
    if let [line] = lines {
        if line.amount_minor == 0 && line.currency.is_empty() {
            return vec![ExpenseLineInput {
                category_id: line.category_id.clone(),
                amount_minor: canonical_amount_minor,
                currency: canonical_currency.to_string(),
            }];
        }
    }
    lines.to_vec()
}

fn insert_expense_lines(
    connection: &Connection,
    canonical_id: &str,
    lines: &[ExpenseLineInput],
) -> Result<()> {
    for line in lines {
        connection.execute(
            "INSERT INTO transaction_lines (
                id, canonical_transaction_id, category_id, amount_minor, currency, line_kind
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                transaction_line_id(canonical_id, &line.category_id, EXPENSE_LINE_KIND),
                canonical_id,
                line.category_id,
                line.amount_minor,
                line.currency.to_ascii_uppercase(),
                EXPENSE_LINE_KIND,
            ],
        )?;
    }
    Ok(())
}

fn canonical_transaction_id(candidate_id: &str) -> String {
    let digest = Sha256::digest(candidate_id.as_bytes());
    format!(
        "txn_{}",
        digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn transfer_pair_id(from_candidate_id: &str, to_candidate_id: &str) -> String {
    let digest = Sha256::digest(format!("{from_candidate_id}|{to_candidate_id}").as_bytes());
    format!(
        "xfer_{}",
        digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn transaction_line_id(canonical_id: &str, category_id: &str, line_kind: &str) -> String {
    let digest = Sha256::digest(format!("{canonical_id}|{category_id}|{line_kind}").as_bytes());
    format!(
        "line_{}",
        digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}
