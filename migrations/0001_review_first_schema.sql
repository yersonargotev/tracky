PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS institutions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    institution_id TEXT REFERENCES institutions(id),
    label TEXT NOT NULL,
    currency TEXT NOT NULL,
    masked_identifier TEXT,
    kind TEXT,
    is_owned INTEGER NOT NULL DEFAULT 0 CHECK (is_owned IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS source_documents (
    id TEXT PRIMARY KEY,
    input_name TEXT NOT NULL,
    content_sha256 TEXT NOT NULL UNIQUE,
    mime_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
    institution_id TEXT REFERENCES institutions(id),
    institution_hint TEXT,
    account_id TEXT REFERENCES accounts(id),
    account_label_hint TEXT,
    account_currency_hint TEXT,
    account_masked_identifier_hint TEXT,
    imported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    duplicate_of_source_document_id TEXT REFERENCES source_documents(id)
);

CREATE TABLE IF NOT EXISTS import_batches (
    id TEXT PRIMARY KEY,
    source_document_id TEXT NOT NULL REFERENCES source_documents(id),
    started_at TEXT NOT NULL,
    completed_at TEXT,
    status TEXT NOT NULL CHECK (status IN ('completed', 'completed_with_errors', 'failed')),
    candidate_count INTEGER NOT NULL DEFAULT 0 CHECK (candidate_count >= 0),
    error_count INTEGER NOT NULL DEFAULT 0 CHECK (error_count >= 0),
    duplicate_count INTEGER NOT NULL DEFAULT 0 CHECK (duplicate_count >= 0),
    error_details_json TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE IF NOT EXISTS income_sources (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS categories (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS canonical_transactions (
    id TEXT PRIMARY KEY,
    account_id TEXT REFERENCES accounts(id),
    posted_date TEXT NOT NULL,
    description TEXT NOT NULL,
    amount_minor INTEGER NOT NULL,
    currency TEXT NOT NULL,
    balance_minor INTEGER,
    transaction_kind TEXT,
    investment_allocation_status TEXT CHECK (investment_allocation_status IN ('pending_allocation')),
    income_source_id TEXT REFERENCES income_sources(id),
    income_kind TEXT CHECK (income_kind IN ('salary', 'freelance', 'client_payment', 'sale', 'interest', 'reimbursement', 'other')),
    investment_fee_component_id TEXT,
    external_reference TEXT,
    created_from_candidate_id TEXT UNIQUE REFERENCES candidate_transactions(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS transaction_lines (
    id TEXT PRIMARY KEY,
    canonical_transaction_id TEXT NOT NULL REFERENCES canonical_transactions(id),
    category_id TEXT NOT NULL REFERENCES categories(id),
    amount_minor INTEGER NOT NULL CHECK (amount_minor <> 0),
    currency TEXT NOT NULL,
    line_kind TEXT NOT NULL CHECK (line_kind IN ('expense')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS candidate_transactions (
    id TEXT PRIMARY KEY,
    import_batch_id TEXT NOT NULL REFERENCES import_batches(id),
    source_document_id TEXT NOT NULL REFERENCES source_documents(id),
    institution_id TEXT REFERENCES institutions(id),
    institution_hint TEXT,
    account_id TEXT REFERENCES accounts(id),
    account_label_hint TEXT,
    account_currency_hint TEXT,
    account_masked_identifier_hint TEXT,
    posted_date TEXT NOT NULL,
    description TEXT NOT NULL,
    amount_minor INTEGER NOT NULL,
    currency TEXT NOT NULL,
    balance_minor INTEGER,
    direction_hint TEXT CHECK (direction_hint IN ('inflow', 'outflow')),
    semantic_hint TEXT CHECK (semantic_hint IN ('bank_movement', 'card_charge', 'card_payment')),
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    status TEXT NOT NULL CHECK (status IN ('pending_review', 'possible_duplicate', 'accepted', 'rejected')),
    duplicate_status TEXT NOT NULL DEFAULT 'not_checked' CHECK (duplicate_status IN ('not_checked', 'unique', 'possible_duplicate', 'exact_duplicate')),
    fingerprint TEXT,
    validation_warnings_json TEXT NOT NULL DEFAULT '[]',
    canonical_transaction_id TEXT REFERENCES canonical_transactions(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS provenance (
    id TEXT PRIMARY KEY,
    candidate_transaction_id TEXT UNIQUE REFERENCES candidate_transactions(id),
    canonical_transaction_id TEXT REFERENCES canonical_transactions(id),
    investment_document_event_id TEXT UNIQUE REFERENCES investment_document_events(id),
    investment_snapshot_id TEXT REFERENCES investment_snapshots(id),
    source_document_id TEXT NOT NULL REFERENCES source_documents(id),
    import_batch_id TEXT REFERENCES import_batches(id),
    page_number INTEGER,
    row_index INTEGER,
    bbox_x REAL,
    bbox_y REAL,
    bbox_width REAL,
    bbox_height REAL,
    bbox_unit TEXT,
    extractor_name TEXT NOT NULL,
    extractor_version TEXT,
    parser_id TEXT NOT NULL,
    parser_version TEXT NOT NULL,
    evidence_redaction TEXT NOT NULL,
    evidence_text_redacted TEXT NOT NULL,
    raw_storage_policy TEXT NOT NULL CHECK (raw_storage_policy IN ('not_stored', 'local_only_optional', 'redacted_only')),
    raw_evidence_ref TEXT,
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (candidate_transaction_id IS NOT NULL OR canonical_transaction_id IS NOT NULL OR investment_document_event_id IS NOT NULL)
);

CREATE TABLE IF NOT EXISTS transaction_fingerprints (
    id TEXT PRIMARY KEY,
    fingerprint TEXT NOT NULL UNIQUE,
    candidate_transaction_id TEXT UNIQUE REFERENCES candidate_transactions(id),
    canonical_transaction_id TEXT UNIQUE REFERENCES canonical_transactions(id),
    duplicate_status TEXT NOT NULL CHECK (duplicate_status IN ('not_checked', 'unique', 'possible_duplicate', 'exact_duplicate')),
    normalized_account_key TEXT,
    normalized_posted_date TEXT NOT NULL,
    normalized_amount_minor INTEGER NOT NULL,
    normalized_currency TEXT NOT NULL,
    normalized_description TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (
        (candidate_transaction_id IS NOT NULL AND canonical_transaction_id IS NULL)
        OR (candidate_transaction_id IS NULL AND canonical_transaction_id IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS transaction_duplicate_markers (
    id TEXT PRIMARY KEY,
    candidate_transaction_id TEXT NOT NULL REFERENCES candidate_transactions(id),
    matched_candidate_transaction_id TEXT REFERENCES candidate_transactions(id),
    matched_canonical_transaction_id TEXT REFERENCES canonical_transactions(id),
    duplicate_status TEXT NOT NULL CHECK (duplicate_status IN ('not_checked', 'unique', 'possible_duplicate', 'exact_duplicate')),
    reason TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (
        matched_candidate_transaction_id IS NOT NULL
        OR matched_canonical_transaction_id IS NOT NULL
        OR duplicate_status IN ('not_checked', 'unique')
    )
);

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

CREATE TABLE IF NOT EXISTS investment_instruments (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    instrument_type TEXT NOT NULL CHECK (instrument_type IN ('fiat_currency', 'dollar_referenced_digital_asset', 'security', 'fixed_income', 'generic')),
    denomination_currency TEXT NOT NULL,
    provider TEXT NOT NULL,
    provider_identifier TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (instrument_type, denomination_currency, provider, provider_identifier, name)
);

CREATE TABLE IF NOT EXISTS investment_allocation_revisions (
    id TEXT PRIMARY KEY,
    allocation_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    contribution_transaction_id TEXT NOT NULL REFERENCES canonical_transactions(id),
    instrument_id TEXT NOT NULL REFERENCES investment_instruments(id),
    cash_amount_minor INTEGER NOT NULL CHECK (cash_amount_minor > 0),
    cash_currency TEXT NOT NULL,
    acquired_quantity TEXT NOT NULL,
    effective_date TEXT,
    fee_amount_minor INTEGER CHECK (fee_amount_minor > 0),
    fee_currency TEXT,
    fee_treatment TEXT CHECK (fee_treatment IN ('capitalized', 'separate')),
    fee_component_id TEXT,
    fee_expense_transaction_id TEXT REFERENCES canonical_transactions(id),
    provenance_source TEXT NOT NULL CHECK (provenance_source IN ('manual_entry')),
    correction_reason TEXT,
    replaces_revision_id TEXT REFERENCES investment_allocation_revisions(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (allocation_id, revision),
    CHECK (
        (fee_amount_minor IS NULL AND fee_currency IS NULL AND fee_treatment IS NULL AND fee_component_id IS NULL AND fee_expense_transaction_id IS NULL)
        OR
        (fee_amount_minor IS NOT NULL AND fee_currency IS NOT NULL AND fee_treatment = 'capitalized' AND fee_component_id IS NOT NULL AND fee_expense_transaction_id IS NULL)
        OR
        (fee_amount_minor IS NOT NULL AND fee_currency IS NOT NULL AND fee_treatment = 'separate' AND fee_component_id IS NOT NULL AND fee_expense_transaction_id IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS investment_allocation_heads (
    allocation_id TEXT PRIMARY KEY,
    current_revision_id TEXT NOT NULL UNIQUE REFERENCES investment_allocation_revisions(id)
);

CREATE TABLE IF NOT EXISTS cdt_positions (
    id TEXT PRIMARY KEY,
    instrument_id TEXT NOT NULL REFERENCES investment_instruments(id),
    account_id TEXT NOT NULL REFERENCES accounts(id),
    constituent_allocation_id TEXT NOT NULL UNIQUE REFERENCES investment_allocation_heads(allocation_id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS cdt_operation_revisions (
    id TEXT PRIMARY KEY,
    operation_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    cdt_position_id TEXT NOT NULL REFERENCES cdt_positions(id),
    operation_type TEXT NOT NULL CHECK (operation_type IN ('constitution', 'renewal', 'redemption')),
    effective_date TEXT NOT NULL,
    currency TEXT NOT NULL,
    principal_before_minor INTEGER NOT NULL CHECK (principal_before_minor >= 0),
    principal_after_minor INTEGER NOT NULL CHECK (principal_after_minor >= 0),
    principal_returned_minor INTEGER NOT NULL DEFAULT 0 CHECK (principal_returned_minor >= 0),
    external_capital_minor INTEGER NOT NULL DEFAULT 0 CHECK (external_capital_minor >= 0),
    capitalized_interest_minor INTEGER NOT NULL DEFAULT 0 CHECK (capitalized_interest_minor >= 0),
    gross_interest_minor INTEGER NOT NULL DEFAULT 0 CHECK (gross_interest_minor >= 0),
    withholding_minor INTEGER NOT NULL DEFAULT 0 CHECK (withholding_minor >= 0),
    other_deductions_minor INTEGER NOT NULL DEFAULT 0 CHECK (other_deductions_minor >= 0),
    net_cash_received_minor INTEGER NOT NULL DEFAULT 0 CHECK (net_cash_received_minor >= 0),
    funding_allocation_id TEXT REFERENCES investment_allocation_heads(allocation_id),
    maturity_date TEXT NOT NULL,
    agreed_rate TEXT,
    payment_mode TEXT,
    payment_periodicity TEXT,
    renewal_terms TEXT,
    contract_identifier TEXT,
    allows_partial_redemption INTEGER NOT NULL DEFAULT 0 CHECK (allows_partial_redemption IN (0, 1)),
    deduction_component_id TEXT,
    deduction_expense_transaction_id TEXT REFERENCES canonical_transactions(id),
    provenance_source TEXT NOT NULL CHECK (provenance_source IN ('manual_entry')),
    correction_reason TEXT,
    replaces_revision_id TEXT REFERENCES cdt_operation_revisions(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (operation_id, revision)
);

CREATE TABLE IF NOT EXISTS cdt_operation_heads (
    operation_id TEXT PRIMARY KEY,
    current_revision_id TEXT NOT NULL UNIQUE REFERENCES cdt_operation_revisions(id)
);

CREATE TABLE IF NOT EXISTS investment_allocation_consumptions (
    allocation_id TEXT PRIMARY KEY REFERENCES investment_allocation_heads(allocation_id),
    consumer_kind TEXT NOT NULL CHECK (consumer_kind IN ('cdt_constitution', 'cdt_additional_capital', 'brokerage_deposit')),
    cdt_position_id TEXT REFERENCES cdt_positions(id),
    consumer_operation_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS brokerage_accounts (
    account_id TEXT PRIMARY KEY REFERENCES accounts(id),
    opened_date TEXT NOT NULL,
    provenance_source TEXT NOT NULL CHECK (provenance_source = 'manual_entry'),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS brokerage_operation_revisions (
    id TEXT PRIMARY KEY,
    operation_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    account_id TEXT NOT NULL REFERENCES brokerage_accounts(account_id),
    operation_type TEXT NOT NULL CHECK (operation_type IN ('deposit','buy','sell','dividend','withdrawal')),
    effective_date TEXT NOT NULL,
    currency TEXT NOT NULL,
    instrument_id TEXT REFERENCES investment_instruments(id),
    quantity TEXT,
    gross_amount_minor INTEGER NOT NULL DEFAULT 0 CHECK (gross_amount_minor >= 0),
    historical_cost_minor INTEGER NOT NULL DEFAULT 0 CHECK (historical_cost_minor >= 0),
    realized_result_minor INTEGER NOT NULL DEFAULT 0,
    fee_minor INTEGER NOT NULL DEFAULT 0 CHECK (fee_minor >= 0),
    fee_treatment TEXT CHECK (fee_treatment IN ('capitalized','separate')),
    withholding_minor INTEGER NOT NULL DEFAULT 0 CHECK (withholding_minor >= 0),
    other_deductions_minor INTEGER NOT NULL DEFAULT 0 CHECK (other_deductions_minor >= 0),
    net_cash_minor INTEGER NOT NULL DEFAULT 0,
    funding_allocation_id TEXT REFERENCES investment_allocation_heads(allocation_id),
    destination_account_id TEXT REFERENCES accounts(id),
    linked_transaction_id TEXT REFERENCES canonical_transactions(id),
    component_id TEXT,
    provenance_source TEXT NOT NULL CHECK (provenance_source = 'manual_entry'),
    correction_reason TEXT,
    replaces_revision_id TEXT REFERENCES brokerage_operation_revisions(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (operation_id, revision)
);

CREATE TABLE IF NOT EXISTS brokerage_operation_heads (
    operation_id TEXT PRIMARY KEY,
    current_revision_id TEXT NOT NULL UNIQUE REFERENCES brokerage_operation_revisions(id)
);
CREATE TABLE IF NOT EXISTS brokerage_buy_funding_attributions (
    operation_revision_id TEXT PRIMARY KEY REFERENCES brokerage_operation_revisions(id),
    external_capital_minor INTEGER NOT NULL CHECK (external_capital_minor >= 0),
    existing_cash_minor INTEGER NOT NULL CHECK (existing_cash_minor >= 0),
    reinvested_minor INTEGER NOT NULL CHECK (reinvested_minor >= 0),
    investment_income_minor INTEGER NOT NULL CHECK (investment_income_minor >= 0),
    unattributed_minor INTEGER NOT NULL CHECK (unattributed_minor >= 0)
);
CREATE INDEX IF NOT EXISTS idx_brokerage_component ON brokerage_operation_revisions(component_id) WHERE component_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_brokerage_account ON brokerage_operation_revisions(account_id);

CREATE TABLE IF NOT EXISTS investment_snapshots (
    id TEXT PRIMARY KEY,
    observed_at TEXT NOT NULL,
    provider_effective_date TEXT,
    source TEXT NOT NULL,
    external_reference TEXT,
    provenance_source TEXT NOT NULL CHECK (length(trim(provenance_source)) > 0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (source, external_reference)
);
CREATE TABLE IF NOT EXISTS investment_snapshot_positions (
    snapshot_id TEXT NOT NULL REFERENCES investment_snapshots(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    instrument_id TEXT REFERENCES investment_instruments(id),
    quantity TEXT,
    currency TEXT NOT NULL,
    observed_cash_minor INTEGER,
    observed_value_minor INTEGER CHECK (observed_value_minor IS NULL OR observed_value_minor >= 0),
    valuation_currency TEXT,
    observed_price TEXT,
    PRIMARY KEY (snapshot_id, account_id, instrument_id, currency),
    CHECK (quantity IS NOT NULL OR observed_cash_minor IS NOT NULL),
    CHECK (observed_cash_minor IS NULL OR observed_cash_minor >= 0),
    CHECK ((observed_value_minor IS NULL AND valuation_currency IS NULL) OR (observed_value_minor IS NOT NULL AND valuation_currency IS NOT NULL))
);
CREATE TABLE IF NOT EXISTS investment_adjustment_revisions (
    id TEXT PRIMARY KEY,
    adjustment_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    snapshot_id TEXT NOT NULL REFERENCES investment_snapshots(id),
    account_id TEXT NOT NULL REFERENCES accounts(id),
    instrument_id TEXT REFERENCES investment_instruments(id),
    currency TEXT NOT NULL,
    quantity_delta TEXT,
    cash_delta_minor INTEGER,
    historical_cost_delta_minor INTEGER NOT NULL DEFAULT 0,
    effective_date TEXT NOT NULL,
    reason TEXT NOT NULL CHECK (length(trim(reason)) > 0),
    provenance_source TEXT NOT NULL CHECK (length(trim(provenance_source)) > 0),
    correction_reason TEXT,
    replaces_revision_id TEXT REFERENCES investment_adjustment_revisions(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (adjustment_id, revision),
    CHECK (quantity_delta IS NOT NULL OR cash_delta_minor IS NOT NULL),
    CHECK (instrument_id IS NOT NULL OR quantity_delta IS NULL)
);
CREATE TABLE IF NOT EXISTS investment_adjustment_heads (
    adjustment_id TEXT PRIMARY KEY,
    current_revision_id TEXT NOT NULL UNIQUE REFERENCES investment_adjustment_revisions(id)
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_investment_adjustment_origin
ON investment_adjustment_revisions(snapshot_id, account_id, IFNULL(instrument_id, ''), currency)
WHERE revision = 1;
CREATE TABLE IF NOT EXISTS investment_snapshot_baselines (
    snapshot_id TEXT NOT NULL REFERENCES investment_snapshots(id),
    account_id TEXT NOT NULL REFERENCES accounts(id),
    instrument_id TEXT REFERENCES investment_instruments(id),
    currency TEXT NOT NULL,
    status TEXT NOT NULL,
    quantity_difference TEXT,
    cash_difference_minor INTEGER,
    derived_historical_cost_minor INTEGER,
    derived_value_minor INTEGER,
    value_difference_minor INTEGER,
    PRIMARY KEY(snapshot_id, account_id, instrument_id, currency)
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_investment_snapshot_baseline_key
ON investment_snapshot_baselines(snapshot_id, account_id, IFNULL(instrument_id, ''), currency);
CREATE INDEX IF NOT EXISTS idx_investment_snapshot_observed ON investment_snapshots(observed_at);

CREATE TABLE IF NOT EXISTS investment_document_events (
    id TEXT PRIMARY KEY,
    source_document_id TEXT NOT NULL REFERENCES source_documents(id),
    import_batch_id TEXT NOT NULL REFERENCES import_batches(id),
    account_id TEXT REFERENCES accounts(id),
    provider TEXT NOT NULL CHECK (provider IN ('nu','wenia','plenti')),
    parser_id TEXT NOT NULL,
    parser_version TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN ('deposit','withdrawal','cdt_opening','cdt_return','observed_cash','observed_position')),
    provider_effective_date TEXT NOT NULL,
    currency TEXT NOT NULL,
    amount_minor INTEGER,
    instrument_hint TEXT,
    quantity TEXT,
    external_reference TEXT,
    page_number INTEGER NOT NULL CHECK (page_number > 0),
    row_index INTEGER NOT NULL CHECK (row_index > 0),
    evidence_redaction TEXT NOT NULL,
    fingerprint TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'pending_review' CHECK (status IN ('pending_review','accepted','rejected')),
    decision TEXT CHECK (decision IS NULL OR decision IN ('reconcile_deposit','reconcile_withdrawal','accept_snapshot','reject')),
    reconciled_kind TEXT CHECK (reconciled_kind IS NULL OR reconciled_kind IN ('canonical_transaction','provider_event','investment_snapshot')),
    reconciled_id TEXT,
    accepted_snapshot_id TEXT UNIQUE REFERENCES investment_snapshots(id),
    reviewed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (amount_minor IS NOT NULL OR quantity IS NOT NULL),
    CHECK ((status = 'pending_review' AND decision IS NULL AND reviewed_at IS NULL) OR
           (status <> 'pending_review' AND decision IS NOT NULL AND reviewed_at IS NOT NULL))
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_investment_document_provider_reference
ON investment_document_events(provider, external_reference)
WHERE external_reference IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_investment_document_reconciliation
ON investment_document_events(reconciled_kind, reconciled_id)
WHERE reconciled_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_investment_document_batch ON investment_document_events(import_batch_id);
CREATE TRIGGER IF NOT EXISTS validate_investment_document_event_insert
BEFORE INSERT ON investment_document_events
WHEN NEW.provider NOT IN ('nu','wenia','plenti')
  OR (NEW.decision IS NOT NULL AND NEW.decision NOT IN ('reconcile_deposit','reconcile_withdrawal','accept_snapshot','reject'))
  OR (NEW.reconciled_kind IS NOT NULL AND NEW.reconciled_kind NOT IN ('canonical_transaction','provider_event','investment_snapshot'))
BEGIN SELECT RAISE(ABORT,'invalid investment document vocabulary'); END;
CREATE TRIGGER IF NOT EXISTS validate_investment_document_event_update
BEFORE UPDATE ON investment_document_events
WHEN NEW.provider NOT IN ('nu','wenia','plenti')
  OR (NEW.decision IS NOT NULL AND NEW.decision NOT IN ('reconcile_deposit','reconcile_withdrawal','accept_snapshot','reject'))
  OR (NEW.reconciled_kind IS NOT NULL AND NEW.reconciled_kind NOT IN ('canonical_transaction','provider_event','investment_snapshot'))
BEGIN SELECT RAISE(ABORT,'invalid investment document vocabulary'); END;

CREATE INDEX IF NOT EXISTS idx_accounts_owned_institution_currency ON accounts(is_owned, institution_id, currency);
CREATE INDEX IF NOT EXISTS idx_income_sources_name ON income_sources(name);
CREATE INDEX IF NOT EXISTS idx_categories_name ON categories(name);
CREATE INDEX IF NOT EXISTS idx_transaction_lines_canonical ON transaction_lines(canonical_transaction_id);
CREATE INDEX IF NOT EXISTS idx_transaction_lines_category ON transaction_lines(category_id);
CREATE INDEX IF NOT EXISTS idx_source_documents_content_sha256 ON source_documents(content_sha256);
CREATE INDEX IF NOT EXISTS idx_import_batches_source_document_id ON import_batches(source_document_id);
CREATE INDEX IF NOT EXISTS idx_candidate_transactions_batch ON candidate_transactions(import_batch_id);
CREATE INDEX IF NOT EXISTS idx_candidate_transactions_source_document ON candidate_transactions(source_document_id);
CREATE INDEX IF NOT EXISTS idx_candidate_transactions_status ON candidate_transactions(status);
CREATE INDEX IF NOT EXISTS idx_candidate_transactions_fingerprint ON candidate_transactions(fingerprint);
CREATE INDEX IF NOT EXISTS idx_provenance_candidate_transaction ON provenance(candidate_transaction_id);
CREATE INDEX IF NOT EXISTS idx_transaction_duplicate_markers_candidate ON transaction_duplicate_markers(candidate_transaction_id);
CREATE INDEX IF NOT EXISTS idx_canonical_transfer_pairs_from_candidate ON canonical_transfer_pairs(from_candidate_id);
CREATE INDEX IF NOT EXISTS idx_canonical_transfer_pairs_to_candidate ON canonical_transfer_pairs(to_candidate_id);
CREATE INDEX IF NOT EXISTS idx_investment_instruments_provider ON investment_instruments(provider, provider_identifier);
CREATE INDEX IF NOT EXISTS idx_investment_allocation_revisions_contribution ON investment_allocation_revisions(contribution_transaction_id);
CREATE INDEX IF NOT EXISTS idx_investment_allocation_revisions_instrument ON investment_allocation_revisions(instrument_id);
CREATE INDEX IF NOT EXISTS idx_cdt_positions_instrument ON cdt_positions(instrument_id);
CREATE INDEX IF NOT EXISTS idx_cdt_positions_account ON cdt_positions(account_id);
CREATE INDEX IF NOT EXISTS idx_cdt_operation_revisions_position ON cdt_operation_revisions(cdt_position_id);
CREATE INDEX IF NOT EXISTS idx_cdt_funding_allocation ON cdt_operation_revisions(funding_allocation_id);
CREATE INDEX IF NOT EXISTS idx_cdt_deduction_component ON cdt_operation_revisions(deduction_component_id);
CREATE INDEX IF NOT EXISTS idx_cdt_deduction_expense ON cdt_operation_revisions(deduction_expense_transaction_id);
CREATE INDEX IF NOT EXISTS idx_investment_allocation_consumptions_position ON investment_allocation_consumptions(cdt_position_id);

PRAGMA user_version = 1;
